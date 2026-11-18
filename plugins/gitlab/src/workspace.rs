//! Multi-repository workspaces: a workspace root can hold several independent Git
//! repositories (the window's own, plus ones in child folders). The merge request
//! views show the repository picked with the list's Repository button; without a
//! pick the window's own repository wins, and a workspace whose window repository
//! has no GitLab remote falls back to its only GitLab repository.

use std::cell::RefCell;
use std::collections::BTreeMap;

use openidea_plugin::host::storage;
use openidea_plugin::{Error, PickItem, Project, RepositoryStatus};

use crate::api;
use crate::branch;
use crate::Repo;

/// The prefix of the stored picks: `workspace-repo:<workspace root>` is the absolute root of the
/// workspace repository the merge request views show. One pick per workspace root, because the
/// plugin's storage is shared by every window of the plugin instance.
const SELECTED_KEY: &str = "workspace-repo";

/// Child folders probed with `git` per discovery run; bigger workspaces keep one window per repository.
const MAX_CHILDREN: usize = 32;

/// A GitLab repository of the workspace: the window's own or an independent one in a child folder.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceRepo {
    /// Absolute repository root.
    pub root: String,
    /// `group/name` on the configured GitLab server.
    pub path: String,
    /// The git remote pointing at it (`origin` first, as git does).
    pub remote: String,
    /// The checked-out branch, when it could be read (`None` on a detached head).
    pub branch: Option<String>,
    /// The checked-out commit, when it could be read.
    pub commit: Option<String>,
}

impl WorkspaceRepo {
    fn into_repo(self) -> Repo {
        Repo {
            status: RepositoryStatus { root: self.root, branch: self.branch, commit: self.commit, ..RepositoryStatus::default() },
            path: self.path,
            remote: self.remote,
        }
    }
}

/// The storage key of the workspace rooted at `workspace_root`'s pick.
fn selected_key(workspace_root: &str) -> String {
    format!("{SELECTED_KEY}:{}", workspace_root.trim_end_matches(['/', '\\']))
}

/// The picked repository root of the workspace rooted at `workspace_root`, or `None` for the window's own.
pub fn selected(workspace_root: &str) -> Option<String> {
    storage::get(&selected_key(workspace_root)).ok().flatten().filter(|root| !root.is_empty())
}

/// Stores the pick of the workspace rooted at `workspace_root`; `None` or an empty root clears it.
pub fn select(workspace_root: &str, picked: Option<&str>) -> Result<(), Error> {
    match picked.filter(|picked| !picked.is_empty()) {
        Some(picked) => storage::set(&selected_key(workspace_root), picked),
        None => storage::delete(&selected_key(workspace_root)),
    }
}

thread_local! {
    /// The GitLab repositories found, keyed by (workspace root, server); dropped by [`forget`].
    static CACHE: RefCell<BTreeMap<(String, String), Vec<WorkspaceRepo>>> = const { RefCell::new(BTreeMap::new()) };
}

/// Drops the discovery cache: the next use walks the workspace again.
pub fn forget() {
    CACHE.with(|cache| cache.borrow_mut().clear());
}

/// The GitLab repositories of the workspace, the window's own first. Cached per workspace root and
/// server until [`forget`] runs, so two windows of different workspaces keep their entries instead
/// of evicting each other's.
pub fn repositories(project: &Project, root: &str) -> Vec<WorkspaceRepo> {
    let key = (root.to_owned(), api::server());
    if let Some(repos) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return repos;
    }

    let repos = discover(project, root);
    CACHE.with(|cache| cache.borrow_mut().insert(key, repos.clone()));
    repos
}

/// The repository the merge request views show: the pick while it is still a
/// workspace repository, else the window's own, else the workspace's only GitLab
/// repository (a window repository without a GitLab remote defers to it).
pub fn resolve(project: &Project) -> Result<Repo, Error> {
    // Without `project.read` there is no workspace discovery; the window's own repository still works.
    let root = project.root().ok();
    let status = project.vcs_status()?;
    let in_repository = status.is_some();

    if let Some(workspace) = root.as_deref()
        && let Some(picked) = selected(workspace)
    {
        let picked = repositories(project, workspace).into_iter().find(|repo| same_root(&repo.root, &picked));
        match picked {
            Some(repo) => return Ok(repo.into_repo()),
            // The picked repository is gone or left the configured server: fall back to the default.
            None => select(workspace, None)?,
        }
    }

    if let Some((remote, path)) = gitlab_remote(&project.vcs_remotes()?) {
        return Ok(WorkspaceRepo {
            root: status.as_ref().map(|status| status.root.clone()).unwrap_or_default(),
            path,
            remote,
            branch: status.as_ref().and_then(|status| status.branch.clone()),
            commit: status.as_ref().and_then(|status| status.commit.clone()),
        }
        .into_repo());
    }

    let workspace = root.as_deref().map(|root| repositories(project, root)).unwrap_or_default();
    match workspace.as_slice() {
        [only] => Ok(only.clone().into_repo()),
        [] => Err(if in_repository { crate::NO_REMOTE } else { crate::NO_REPOSITORY }.into()),
        _ => Err("This workspace has several GitLab repositories. Pick one with Repository in the Merge Requests tab.".into()),
    }
}

/// Whether the workspace has repositories to pick from, so an unavailable list still offers the picker.
pub fn pickable(project: &Project) -> bool {
    project.root().map(|root| !repositories(project, &root).is_empty()).unwrap_or(false)
}

/// The pick list: every workspace repository, the window's own first.
pub fn pick_items(project: &Project, root: &str) -> Vec<PickItem> {
    repositories(project, root).iter().map(|repo| PickItem::new(&repo.root, &label(repo, root))).collect()
}

/// `path (place)` with the window's repository marked and child ones named by folder.
fn label(repo: &WorkspaceRepo, window_root: &str) -> String {
    let place = match same_root(&repo.root, window_root) {
        true => "this window".to_owned(),
        false => repo.root.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(&repo.root).to_owned(),
    };

    format!("{} ({place})", repo.path)
}

/// The workspace repository containing `path` (an editor file or selection), longest root first.
/// `None` without a pick for a path of the window's own repository: the window's repository serves
/// it, so single-repository workspaces never walk the workspace for a link.
pub fn containing(project: &Project, path: &str) -> Option<Repo> {
    let root = project.root().ok()?;
    let path = path.replace('\\', "/");
    if selected(&root).is_none() && in_window_repository(project, &path) {
        return None;
    }

    repositories(project, &root)
        .iter()
        .filter(|repo| within(&repo.root, &path))
        .max_by_key(|repo| repo.root.chars().count())
        .cloned()
        .map(WorkspaceRepo::into_repo)
}

/// Whether `path` is under the window's repository root with no `.git` entry (a nested
/// repository) between them.
fn in_window_repository(project: &Project, path: &str) -> bool {
    let Ok(Some(status)) = project.vcs_status() else {
        return false;
    };
    let root = status.root.replace('\\', "/");
    if !within(&root, path) {
        return false;
    }

    let mut dir = path;
    while !same_root(dir, &root) {
        if project.exists(&format!("{dir}/.git")).unwrap_or(false) {
            return false;
        }
        let Some((parent, _)) = dir.rsplit_once('/') else {
            break;
        };
        dir = parent;
    }

    true
}

/// `repo` with the commit checked out now. Discovery keeps each repository's commit until the
/// next VCS change event, which a window without events never gets, so links read it again.
pub fn with_live_commit(project: &Project, mut repo: Repo) -> Repo {
    match project.vcs_status() {
        Ok(Some(status)) if same_root(&status.root, &repo.status.root) => repo.status.commit = status.commit,
        _ => {
            if let Ok(commit) = branch::git_ok(project, &["-C", &repo.status.root, "rev-parse", "HEAD"]) {
                repo.status.commit = Some(commit.trim().to_owned()).filter(|commit| !commit.is_empty());
            }
        }
    }

    repo
}

/// Whether `root` is `path` itself or an ancestor of it, on either separator.
pub fn within(root: &str, path: &str) -> bool {
    let root = root.trim_end_matches(['/', '\\']).replace('\\', "/");
    let path = path.replace('\\', "/");

    path == root || path.starts_with(&format!("{root}/"))
}

/// Equality of two repository roots, ignoring trailing separators.
pub fn same_root(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\']) == b.trim_end_matches(['/', '\\'])
}

/// The window's repository (`origin` first, as git does) when it is on the configured GitLab server.
fn gitlab_remote(remotes: &[(String, String)]) -> Option<(String, String)> {
    remotes
        .iter()
        .filter_map(|(name, url)| api::project_path(url).map(|path| (name.clone(), path)))
        .min_by_key(|(name, _)| name != "origin")
}

/// The window's repository, then the independent child repositories with a GitLab remote.
fn discover(project: &Project, root: &str) -> Vec<WorkspaceRepo> {
    let root = root.trim_end_matches(['/', '\\']).to_owned();
    let mut repos = Vec::new();

    let status = project.vcs_status().ok().flatten();
    if let Some((remote, path)) = gitlab_remote(&project.vcs_remotes().unwrap_or_default()) {
        repos.push(WorkspaceRepo {
            root: status.as_ref().map(|status| status.root.clone()).unwrap_or_else(|| root.clone()),
            path,
            remote,
            branch: status.as_ref().and_then(|status| status.branch.clone()),
            commit: status.as_ref().and_then(|status| status.commit.clone()),
        });
    }

    let mut probed = 0;
    for child in child_candidates(project, &root, status.as_ref().map(|status| status.root.as_str())) {
        if probed >= MAX_CHILDREN {
            break;
        }
        if !project.exists(&format!("{child}/.git")).unwrap_or(false) {
            continue;
        }
        probed += 1;

        let remotes = match child_remotes(project, &child) {
            Ok(remotes) => remotes,
            // Without the git permission child repositories stay undiscovered; the window's own still works.
            Err(Error::Denied(_)) => break,
            Err(_) => continue,
        };
        if let Some((remote, path)) = gitlab_remote(&remotes) {
            let (branch, commit) = child_head(project, &child).unwrap_or((None, None));
            repos.push(WorkspaceRepo { root: format!("{root}/{child}"), path, remote, branch, commit });
        }
    }

    repos
}

/// Direct child folders that may hold an independent repository: every first-level
/// folder the project index lists files under, plus the untracked and ignored
/// folders of the window's own repository (child clones are usually ignored there).
/// `repository_root` is that repository's root, which may be above the project root.
fn child_candidates(project: &Project, root: &str, repository_root: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for glob in ["*/*", "*/*/*"] {
        for path in project.find_files(glob, 4096).unwrap_or_default() {
            if let Some(name) = first_segment(&path, root) {
                names.push(name.to_owned());
            }
        }
    }

    if let Some(repository_root) = repository_root
        && let Ok(status) = branch::git_ok(project, &["status", "--porcelain", "-z", "--ignored", "--untracked-files=normal"])
    {
        // `XY path` entries, relative to the repository root; a rename or copy is followed by its source path alone.
        let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
        while let Some(entry) = entries.next() {
            let (Some(xy), Some(path)) = (entry.get(..2), entry.get(3..)) else {
                continue;
            };
            if xy.contains(['R', 'C']) {
                entries.next();
            }
            let path = format!("{}/{path}", repository_root.trim_end_matches(['/', '\\']));
            if within(root, &path)
                && let Some(name) = first_segment(&path, root)
            {
                names.push(name.to_owned());
            }
        }
    }

    names.retain(|name| !name.starts_with('.') && name != "..");
    names.sort();
    names.dedup();
    names
}

/// The first path segment of `path`, absolute under `root` or project-relative.
fn first_segment<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let relative = path.strip_prefix(root.trim_end_matches(['/', '\\'])).map(|rest| rest.trim_start_matches(['/', '\\'])).unwrap_or(path);

    relative.split(['/', '\\']).next().filter(|name| !name.is_empty())
}

/// The remotes of an independent child repository, as `git -C` reports them; a `fetch` line per remote.
fn child_remotes(project: &Project, child: &str) -> Result<Vec<(String, String)>, Error> {
    let out = branch::git_ok(project, &["-C", child, "remote", "-v"])?;

    Ok(out
        .lines()
        .filter_map(|line| {
            let (name, url) = line.split_once('\t')?;
            url.strip_suffix(" (fetch)").map(|url| (name.to_owned(), url.to_owned()))
        })
        .collect())
}

/// The branch and commit of an independent child repository; a detached head is no branch.
fn child_head(project: &Project, child: &str) -> Result<(Option<String>, Option<String>), Error> {
    let branch = branch::git_ok(project, &["-C", child, "rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_owned();
    let commit = branch::git_ok(project, &["-C", child, "rev-parse", "HEAD"])?.trim().to_owned();

    Ok((Some(branch).filter(|branch| !branch.is_empty() && branch != "HEAD"), Some(commit).filter(|commit| !commit.is_empty())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::ProcessOutput;

    const CAPS: &[&str] = &["vcs.read", "net:gitlab.com", "storage", "project.read", "process:git"];

    fn git(status: Option<i32>, stdout: &str) -> Result<ProcessOutput, Error> {
        Ok(ProcessOutput { status, stdout: stdout.to_owned(), stderr: String::new() })
    }

    fn root_status(root: &str) -> RepositoryStatus {
        RepositoryStatus { root: root.into(), branch: Some("main".into()), commit: Some("abc123".into()), ..RepositoryStatus::default() }
    }

    /// A workspace root with `svc-a` (GitLab), `svc-b` (GitHub), `svc-c` (a plain folder) and `svc-d` (GitLab, unlisted by the index).
    fn workspace() -> MockHost {
        let host = MockHost::install().with_capabilities(CAPS);
        host.set_project_root(Some("/work"));
        host.set_vcs_status(Some(root_status("/work")));
        host.set_vcs_remotes(&[("origin", "https://gitlab.com/g/p.git")]);
        host.add_file("README.md", "x");
        host.add_dir("svc-a/.git");
        host.add_file("svc-a/README.md", "x");
        host.add_dir("svc-b/.git");
        host.add_file("svc-b/nested/lib.rs", "x");
        host.add_file("svc-c/notes.txt", "x");
        host.add_dir("svc-d/.git");
        host.on_process("git", |args, _| match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["status", "--porcelain", "-z", "--ignored", "--untracked-files=normal"] => git(Some(0), "!! svc-d/\0"),
            ["-C", "svc-a", "remote", "-v"] => git(Some(0), "origin\thttps://gitlab.com/g/svc-a.git (fetch)\norigin\thttps://gitlab.com/g/svc-a.git (push)\n"),
            ["-C", "svc-b", "remote", "-v"] => git(Some(0), "origin\tgit@github.com:x/y.git (fetch)\n"),
            ["-C", "svc-d", "remote", "-v"] => git(Some(0), "upstream\thttps://gitlab.com/g/svc-d.git (fetch)\n"),
            ["-C", "svc-a", "rev-parse", "--abbrev-ref", "HEAD"] => git(Some(0), "feature\n"),
            ["-C", "svc-a", "rev-parse", "HEAD"] => git(Some(0), "def456\n"),
            _ => git(Some(128), ""),
        });
        host
    }

    fn project_of(host: &MockHost) -> Project {
        host.invocation().project().unwrap()
    }

    #[test]
    fn finds_the_window_and_the_child_repositories_with_a_gitlab_remote() {
        let host = workspace();
        let project = project_of(&host);

        assert_eq!(
            repositories(&project, "/work"),
            [
                WorkspaceRepo {
                    root: "/work".into(),
                    path: "g/p".into(),
                    remote: "origin".into(),
                    branch: Some("main".into()),
                    commit: Some("abc123".into())
                },
                WorkspaceRepo {
                    root: "/work/svc-a".into(),
                    path: "g/svc-a".into(),
                    remote: "origin".into(),
                    branch: Some("feature".into()),
                    commit: Some("def456".into())
                },
                WorkspaceRepo {
                    root: "/work/svc-d".into(),
                    path: "g/svc-d".into(),
                    remote: "upstream".into(),
                    branch: None,
                    commit: None
                },
            ]
        );
    }

    #[test]
    fn the_cache_serves_until_the_root_or_server_changes_or_forget_runs() {
        let host = workspace();
        let project = project_of(&host);
        repositories(&project, "/work");

        host.on_process("git", |_, _| git(Some(128), ""));
        assert_eq!(repositories(&project, "/work").len(), 3, "cached");
        assert_eq!(repositories(&project, "/other").len(), 1, "another root rediscovers, and only the window's own is under it");

        forget();
        assert_eq!(repositories(&project, "/work").len(), 1, "forget rediscovers: the window's own, without git answers");
    }

    #[test]
    fn the_pick_switches_the_views_and_a_stale_pick_falls_back() {
        let host = workspace();
        let project = project_of(&host);
        select("/work", Some("/work/svc-a")).unwrap();
        let repo = resolve(&project).unwrap();
        assert_eq!(repo.path, "g/svc-a");
        assert_eq!(repo.status.root, "/work/svc-a");
        assert_eq!(repo.status.branch.as_deref(), Some("feature"));

        select("/work", Some("/work/gone")).unwrap();
        let repo = resolve(&project).unwrap();
        assert_eq!(repo.path, "g/p", "a stale pick is cleared");
        assert_eq!(selected("/work"), None);
    }

    #[test]
    fn windows_of_different_roots_keep_their_own_picks_and_caches() {
        let host = workspace();
        let project = project_of(&host);
        select("/work", Some("/work/svc-a")).unwrap();
        assert_eq!(resolve(&project).unwrap().path, "g/svc-a");

        // A second window over another workspace root resolves without a pick of
        // its own — and must not touch /work's pick or evict its cache entry.
        host.set_project_root(Some("/elsewhere"));
        host.set_vcs_status(Some(root_status("/elsewhere")));
        host.set_vcs_remotes(&[("origin", "https://gitlab.com/o/other.git")]);
        host.add_dir("svc-x/.git");
        host.add_file("svc-x/README.md", "x");
        host.on_process("git", |args, _| match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["-C", "svc-x", "remote", "-v"] => git(Some(0), "origin\thttps://gitlab.com/o/svc-x.git (fetch)\n"),
            _ => git(Some(128), ""),
        });
        let other = project_of(&host);
        assert_eq!(resolve(&other).unwrap().path, "o/other", "the other window falls back to its own repository");
        select("/elsewhere", Some("/elsewhere/svc-x")).unwrap();
        assert_eq!(resolve(&other).unwrap().path, "o/svc-x");

        // The first window again: its pick is intact and its repositories come
        // from the cache (git answers nothing anymore).
        host.set_project_root(Some("/work"));
        host.set_vcs_status(Some(root_status("/work")));
        host.set_vcs_remotes(&[("origin", "https://gitlab.com/g/p.git")]);
        host.on_process("git", |_, _| git(Some(128), ""));
        let again = project_of(&host);
        assert_eq!(
            resolve(&again).unwrap().path,
            "g/svc-a",
            "the first root's pick survives the other window"
        );
        assert_eq!(selected("/work").as_deref(), Some("/work/svc-a"));
        assert_eq!(selected("/elsewhere").as_deref(), Some("/elsewhere/svc-x"));
    }

    #[test]
    fn a_workspace_of_only_children_serves_its_single_gitlab_repository_or_asks_for_a_pick() {
        let host = MockHost::install().with_capabilities(CAPS);
        host.set_project_root(Some("/work"));
        host.set_vcs_status(None);
        host.add_file("one/README.md", "x");
        host.add_file("two/README.md", "x");
        host.add_dir("one/.git");
        host.add_dir("two/.git");
        host.on_process("git", |args, _| match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["-C", "one", "remote", "-v"] => git(Some(0), "origin\thttps://gitlab.com/g/one.git (fetch)\n"),
            ["-C", "two", "remote", "-v"] => git(Some(0), "origin\thttps://gitlab.com/g/two.git (fetch)\n"),
            _ => git(Some(128), ""),
        });
        let project = project_of(&host);

        let error = match resolve(&project) {
            Err(error) => error.to_string(),
            Ok(repo) => panic!("a workspace of two child repositories needs a pick, got {}", repo.path),
        };
        assert!(error.contains("Pick one with Repository in the Merge Requests tab"), "{error}");
        assert!(pickable(&project));

        select("/work", Some("/work/two")).unwrap();
        assert_eq!(resolve(&project).unwrap().path, "g/two");
    }

    #[test]
    fn status_paths_are_read_from_the_repository_root_and_rename_sources_are_skipped() {
        let host = MockHost::install().with_capabilities(CAPS);
        host.set_project_root(Some("/work/sub"));
        host.on_process("git", |_, _| git(Some(0), "!! sub/svc-d/\0R  sub/new.txt\0sub/docs/old.txt\0?? other/\0"));
        let project = project_of(&host);
        assert_eq!(child_candidates(&project, "/work/sub", Some("/work")), ["new.txt", "svc-d"], "a window on a repository subfolder");

        host.set_project_root(Some("/work"));
        host.on_process("git", |_, _| git(Some(0), "R  new.txt\0abcd/old.txt\0C  copy.txt\0wxyz/src.txt\0"));
        let project = project_of(&host);
        assert_eq!(child_candidates(&project, "/work", Some("/work")), ["copy.txt", "new.txt"], "rename and copy sources are not entries");
    }

    #[test]
    fn plain_folders_do_not_use_up_the_child_repository_limit() {
        let host = MockHost::install().with_capabilities(CAPS);
        host.set_project_root(Some("/work"));
        host.set_vcs_status(None);
        for n in 0..=MAX_CHILDREN {
            host.add_file(&format!("a{n:02}/notes.txt"), "x");
        }
        host.add_file("zz/README.md", "x");
        host.add_dir("zz/.git");
        host.on_process("git", |args, _| match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["-C", "zz", "remote", "-v"] => git(Some(0), "origin\thttps://gitlab.com/g/zz.git (fetch)\n"),
            _ => git(Some(128), ""),
        });

        let repos = repositories(&project_of(&host), "/work");

        assert_eq!(repos.iter().map(|repo| repo.path.as_str()).collect::<Vec<_>>(), ["g/zz"]);
    }

    #[test]
    fn files_follow_the_repository_containing_them() {
        let host = workspace();
        let project = project_of(&host);
        select("/work", Some("/work/svc-a")).unwrap();

        assert_eq!(containing(&project, "/work/svc-a/src/lib.rs").unwrap().path, "g/svc-a");
        assert_eq!(containing(&project, "/work/README.md").unwrap().path, "g/p");
        assert!(containing(&project, "/elsewhere/x.rs").is_none());
    }

    #[test]
    fn paths_compare_across_separators_and_trailing_slashes() {
        assert!(within("/work", "/work/svc-a/x"));
        assert!(within("/work/", "/work"));
        assert!(within("C:\\work", "C:\\work\\svc\\x"));
        assert!(!within("/work", "/workspace/x"));
        assert!(same_root("/work/", "/work"));
        assert_eq!(first_segment("/work/svc-a/file", "/work"), Some("svc-a"));
        assert_eq!(first_segment("svc-a/file", "/work"), Some("svc-a"));
        assert_eq!(first_segment("", "/work"), None);
    }

    #[test]
    fn the_pick_labels_name_the_place() {
        let repos = [
            WorkspaceRepo { root: "/work".into(), path: "g/p".into(), remote: "origin".into(), branch: None, commit: None },
            WorkspaceRepo { root: "/work/svc-a".into(), path: "g/svc-a".into(), remote: "origin".into(), branch: None, commit: None },
        ];

        assert_eq!(label(&repos[0], "/work"), "g/p (this window)");
        assert_eq!(label(&repos[1], "/work"), "g/svc-a (svc-a)");
    }
}
