//! Docker target for console commands, ported from the Laravel plugin's Docker target.
//! Labels only name candidates; the mount proof decides. Docker runs with argv only
//! (no shell), never starts or creates containers, and a fresh inspect is compared
//! with the saved proof before every console run.
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Project, host};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_SOCKET: &str = "unix:///var/run/docker.sock";
const DDEV_WORKDIR: &str = "/var/www/html";
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_CONTAINERS: usize = 128;
const MAX_SAVED: usize = 64 * 1024;
const FEATURE: &str = "Running console commands in Docker";
const CHOOSE: &str = "Symfony: Choose Docker Target…";
const PROOF_FORMAT: &str = r#"{"Id":{{json .Id}},"Image":{{json .Image}},"Running":{{json .State.Running}},"Paused":{{json .State.Paused}},"Restarting":{{json .State.Restarting}},"StartedAt":{{json .State.StartedAt}},"Mounts":{{json .Mounts}},"DdevRoot":{{json (index .Config.Labels "com.ddev.approot")}}}"#;
const SCAN_FORMAT: &str = r#"{"Id":{{json .Id}},"Image":{{json .Image}},"Running":{{json .State.Running}},"Paused":{{json .State.Paused}},"Restarting":{{json .State.Restarting}},"StartedAt":{{json .State.StartedAt}},"Mounts":{{json .Mounts}},"Name":{{json .Name}},"WorkingDir":{{json .Config.WorkingDir}},"ComposeProject":{{json (index .Config.Labels "com.docker.compose.project")}},"ComposeService":{{json (index .Config.Labels "com.docker.compose.service")}},"DdevRoot":{{json (index .Config.Labels "com.ddev.approot")}},"DdevName":{{json (index .Config.Labels "com.ddev.site-name")}}}"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    socket: String,
    container: String,
    workdir: String,
    label: Option<String>,
}

impl Target {
    fn validate(socket: &str, container: &str, workdir: &str) -> Result<Self, Error> {
        let socket_path = socket.strip_prefix("unix://").unwrap_or_default();
        if socket.len() > 1024
            || !canonical(socket_path)
            || socket.chars().any(char::is_control)
            || !container_id(container)
            || workdir.len() > 1024
            || !canonical(workdir)
            || workdir.chars().any(char::is_control)
        {
            return Err(Error::Invalid("The Docker settings are invalid. Use a unix:///absolute/path socket, a full 64-character container ID and an absolute project folder in Settings › Plugins › Symfony.".into()));
        }
        Ok(Self { socket: socket.into(), container: container.into(), workdir: workdir.into(), label: None })
    }

    fn from_json(value: &Value) -> Result<Self, Error> {
        Self::validate(
            value["socket"].as_str().unwrap_or_default(),
            value["container"].as_str().unwrap_or_default(),
            value["workdir"].as_str().unwrap_or_default(),
        )
    }

    fn to_json(&self) -> Value {
        json!({"socket": self.socket, "container": self.container, "workdir": self.workdir})
    }

    fn inspect_args(&self) -> Vec<String> {
        ["--host", &self.socket, "inspect", "--type", "container", "--format", PROOF_FORMAT, &self.container].map(str::to_owned).to_vec()
    }

    /// `docker --host S exec [-it] --workdir W <id> php <php_args…>`.
    pub(crate) fn exec_args(&self, php_args: &[String], terminal: bool) -> Vec<String> {
        let mut args = vec!["--host".to_owned(), self.socket.clone(), "exec".to_owned()];
        if terminal {
            args.push("-it".to_owned());
        }
        args.extend(["--workdir".to_owned(), self.workdir.clone(), self.container.clone(), "php".to_owned()]);
        args.extend_from_slice(php_args);
        args
    }

    pub(crate) fn socket(&self) -> &str {
        &self.socket
    }

    pub(crate) fn container(&self) -> &str {
        &self.container
    }

    /// A path inside the container's project folder as the same file below `root` on this machine.
    pub(crate) fn host_path(&self, root: &str, path: &str) -> Option<String> {
        map_path(path, &self.workdir, root)
    }

    /// "Compose · shop / php (abcdef012345…, /var/www/html)"; never runs Docker.
    pub(crate) fn label(&self) -> String {
        let place = format!("{}…, {}", &self.container[..12], self.workdir);
        match &self.label {
            Some(label) => format!("{label} ({place})"),
            None => place,
        }
    }
}

fn container_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn canonical(path: &str) -> bool {
    path.starts_with('/') && !path.contains(['\\', '\0']) && path.split('/').skip(1).all(|part| !matches!(part, "" | "." | ".."))
}

fn absolute(path: &str) -> bool {
    (path == "/" || canonical(path)) && !path.chars().any(char::is_control)
}

/// Maps a path below one absolute root to the same place below another, keeping component boundaries.
fn map_path(path: &str, from: &str, to: &str) -> Option<String> {
    if !absolute(path) || !absolute(from) || !absolute(to) {
        return None;
    }
    let suffix = if path == from {
        ""
    } else if from == "/" {
        path.strip_prefix('/')?
    } else {
        path.strip_prefix(from)?.strip_prefix('/')?
    };
    let mapped = match (suffix.is_empty(), to == "/") {
        (true, _) => to.to_owned(),
        (false, true) => format!("/{suffix}"),
        (false, false) => format!("{to}/{suffix}"),
    };
    absolute(&mapped).then_some(mapped)
}

fn key(root: &str) -> String {
    let hash = root.bytes().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3));
    format!("symfony-docker-target-v1:{hash:016x}")
}

fn settings() -> Value {
    json!({
        "socket": host::setting_string("dockerSocket").filter(|s| !s.is_empty()).unwrap_or_else(|| DEFAULT_SOCKET.into()),
        "container": host::setting_string("dockerContainer").unwrap_or_default(),
        "workdir": host::setting_string("dockerWorkdir").unwrap_or_default(),
    })
}

fn manual() -> Result<Option<Target>, Error> {
    let settings = settings();
    if settings["container"] == "" && settings["workdir"] == "" {
        return Ok(None);
    }
    Target::from_json(&settings).map(Some)
}

/// The chosen target and its saved proof, ignored once the Docker settings are edited.
fn saved(project: &Project) -> Result<Option<(Target, Value)>, Error> {
    let root = project.root()?;
    let raw = match host::storage::get(&key(&root)) {
        Ok(Some(raw)) => raw,
        Ok(None) | Err(Error::Denied(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    let value = crate::strict_json::parse(&raw, MAX_SAVED)?;
    if value["schema"] != 1 || value["root"] != root {
        return Err(Error::Invalid(format!("The saved Docker target belongs to another project. Run {CHOOSE} again.")));
    }
    if value["settings"] != settings() {
        return Ok(None);
    }
    let mut target = Target::from_json(&value["target"])?;
    target.label = value["label"].as_str().filter(|l| !l.is_empty() && l.len() <= 256 && !l.chars().any(char::is_control)).map(str::to_owned);
    let proof = proof(&value["proof"].to_string(), &root, &target)?;
    Ok(Some((target, proof)))
}

/// The target console commands use in this project: the chosen one, else the Docker settings, else none (local PHP). Never runs Docker.
pub(crate) fn current(project: &Project) -> Result<Option<Target>, Error> {
    match saved(project)? {
        Some((target, _)) => Ok(Some(target)),
        None => manual(),
    }
}

/// Like [`current`], after a fresh `docker inspect` proves it is still the same running container mounted from this project.
pub(crate) fn verify(project: &Project) -> Result<Option<Target>, Error> {
    let root = project.root()?;
    let (target, saved_proof) = match saved(project)? {
        Some((target, proof)) => (target, Some(proof)),
        None => match manual()? {
            Some(target) => (target, None),
            None => return Ok(None),
        },
    };
    let output = run(project, &target.inspect_args()).map_err(|error| match error {
        Error::Failed(reason) => Error::Invalid(format!("Docker couldn't check container {}: {reason}. Start the container, or run {CHOOSE} again.", target.label())),
        other => other,
    })?;
    let fresh = proof(&output, &root, &target).map_err(|error| stale(&target, error))?;
    if saved_proof.is_some_and(|saved| saved != fresh) {
        return Err(Error::Invalid(format!(
            "Docker container {} was restarted or its image or mounts changed since you chose it. Run {CHOOSE} again.",
            target.label()
        )));
    }
    Ok(Some(target))
}

fn stale(target: &Target, error: Error) -> Error {
    match error {
        Error::Invalid(reason) | Error::Failed(reason) => {
            Error::Invalid(format!("Docker container {} no longer matches this project: {reason} Run {CHOOSE} again.", target.label()))
        }
        other => other,
    }
}

/// Checks one inspect result (or a saved proof) against the target and project root; returns the proof to compare later.
fn proof(raw: &str, root: &str, target: &Target) -> Result<Value, Error> {
    let value = crate::strict_json::parse(raw, MAX_OUTPUT)?;
    if !canonical(root) || value["Id"] != target.container || value["Running"] != true || value["Paused"] != false || value["Restarting"] != false {
        return Err(Error::Invalid("the container is not running, or is paused or restarting.".into()));
    }
    let started = value["StartedAt"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 128 && !v.chars().any(char::is_control))
        .ok_or("Docker didn't report when the container started.")?;
    let image = value["Image"]
        .as_str()
        .filter(|v| v.strip_prefix("sha256:").is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())))
        .ok_or("Docker didn't report the container's image.")?;
    let mounts = value["Mounts"].as_array().filter(|v| v.len() <= 128).ok_or("Docker didn't report the container's mounts, or it has over 128.")?;
    // Docker applies the most specific covering mount; nested binds must keep the same project mapping.
    let mut covering: Vec<(&str, &str, &str)> = Vec::new();
    let mut descendants: Vec<(&str, &str, &str)> = Vec::new();
    for mount in mounts {
        let destination = mount["Destination"].as_str().filter(|p| absolute(p) && p.len() <= 4096).ok_or("a mount has an unusual container path.")?;
        let source = mount["Source"].as_str().filter(|s| s.len() <= 4096 && !s.chars().any(char::is_control)).ok_or("a mount has no source.")?;
        let kind = mount["Type"].as_str().ok_or("a mount has no type.")?;
        if destination == target.workdir || destination == "/" || crate::under(&target.workdir, destination) {
            covering.push((destination, source, kind));
        } else if crate::under(destination, &target.workdir) {
            descendants.push((destination, source, kind));
        }
    }
    covering.sort_by_key(|(destination, _, _)| std::cmp::Reverse(destination.len()));
    let Some(&(destination, source, kind)) = covering.first() else {
        return Err(Error::Invalid("no mount maps the container folder to this project.".into()));
    };
    let bind = kind == "bind" && absolute(source) && map_path(&target.workdir, destination, source).as_deref() == Some(root);
    let ddev_volume = kind == "volume" && target.workdir == DDEV_WORKDIR && value["DdevRoot"].as_str() == Some(root);
    if covering.get(1).is_some_and(|(other, _, _)| *other == destination) || !(bind || ddev_volume) {
        return Err(Error::Invalid("the container folder is not mounted from this project (a bind mount, or a DDEV project volume).".into()));
    }
    let mut destinations = BTreeSet::new();
    for (destination, source, kind) in descendants {
        if !destinations.insert(destination) || kind != "bind" || !absolute(source) || map_path(destination, &target.workdir, root).as_deref() != Some(source) {
            return Err(Error::Invalid("another mount hides part of the project folder.".into()));
        }
    }
    // Docker lists mounts in a different order on each inspect. Only identity and mounts are kept: never environment.
    let mut sorted = mounts.clone();
    sorted.sort_by_key(|mount| ["Destination", "Source", "Type"].map(|key| mount[key].as_str().unwrap_or_default().to_owned()));
    let mut proof = json!({"Id": target.container, "Image": image, "Running": true, "Paused": false, "Restarting": false, "StartedAt": started, "Mounts": sorted});
    if ddev_volume {
        proof["DdevRoot"] = json!(root);
    }
    Ok(proof)
}

/// Runs the Docker client to its end; a non-zero exit becomes `Failed` with Docker's last error line.
pub(crate) fn run(project: &Project, args: &[String]) -> Result<String, Error> {
    let output = project.run_process("docker", args, None).map_err(|error| ui_kit::denied::map(error, FEATURE, "process:docker", "Symfony"))?;
    if output.status != Some(0) {
        let reason = output.stderr.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no error output").trim();
        return Err(Error::Failed(reason.chars().take(300).collect()));
    }
    if output.stdout.len() > MAX_OUTPUT {
        return Err(Error::Invalid("Docker's answer is over 4 MiB.".into()));
    }
    Ok(output.stdout)
}

#[derive(Clone)]
struct Candidate {
    target: Target,
    proof: Value,
    detail: String,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str().filter(|text| !text.is_empty() && text.len() <= 256 && !text.chars().any(char::is_control))
}

fn candidate_label(value: &Value, root: &str) -> String {
    if let (Some(_), Some(name)) = (text(value, "DdevRoot").filter(|r| *r == root), text(value, "DdevName")) {
        return format!("DDEV · {name}");
    }
    if let (Some(project), Some(service)) = (text(value, "ComposeProject"), text(value, "ComposeService")) {
        return format!("Compose · {project} / {service}");
    }
    match text(value, "Name").map(|name| name.trim_start_matches('/')).filter(|name| !name.is_empty()) {
        Some(name) => format!("Container · {name}"),
        None => "Unnamed container".into(),
    }
}

fn candidates(root: &str, socket: &str, values: &[Value]) -> Vec<Candidate> {
    let mut found = BTreeMap::new();
    for value in values {
        let (Some(id), Some(mounts)) = (text(value, "Id"), value["Mounts"].as_array()) else { continue };
        if mounts.len() > 128 {
            continue;
        }
        let mut directories = BTreeSet::new();
        if let Some(path) = text(value, "WorkingDir").filter(|path| canonical(path)) {
            directories.insert(path.to_owned());
        }
        for mount in mounts.iter().filter(|mount| mount["Type"] == "bind") {
            if let (Some(source), Some(destination)) = (mount["Source"].as_str(), mount["Destination"].as_str()) {
                directories.extend(map_path(root, source, destination));
            }
        }
        for workdir in directories {
            let Ok(mut target) = Target::validate(socket, id, &workdir) else { continue };
            let Ok(proof) = proof(&value.to_string(), root, &target) else { continue };
            target.label = Some(candidate_label(value, root));
            let detail = format!("{workdir} · {}…", &id[..12]);
            found.insert(format!("{id}:{workdir}"), Candidate { target, proof, detail });
        }
    }
    let mut candidates: Vec<_> = found.into_values().collect();
    candidates.sort_by(|a, b| (&a.target.label, &a.target.workdir, &a.target.container).cmp(&(&b.target.label, &b.target.workdir, &b.target.container)));
    candidates
}

fn ps_args(socket: &str) -> Vec<String> {
    ["--host", socket, "ps", "--no-trunc", "--filter", "status=running", "--format", "{{.ID}}"].map(str::to_owned).to_vec()
}

fn ids(raw: &str) -> Result<Vec<String>, Error> {
    let mut found = BTreeSet::new();
    for id in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !container_id(id) || !found.insert(id.to_owned()) {
            return Err(Error::Invalid("Docker listed an invalid or duplicate container ID. Try again.".into()));
        }
    }
    if found.len() > MAX_CONTAINERS {
        return Err(Error::Invalid("Over 128 containers are running. Set the container ID and project folder in Settings › Plugins › Symfony instead.".into()));
    }
    Ok(found.into_iter().collect())
}

fn scan_args(socket: &str, ids: &[String]) -> Vec<String> {
    let mut args = ["--host", socket, "inspect", "--type", "container", "--format", SCAN_FORMAT].map(str::to_owned).to_vec();
    args.extend_from_slice(ids);
    args
}

fn values(raw: &str, requested: &[String]) -> Result<Vec<Value>, Error> {
    let requested: BTreeSet<_> = requested.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    let mut values = vec![];
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let value = crate::strict_json::parse(line, MAX_OUTPUT)?;
        let id = value["Id"].as_str().ok_or(Error::Stale)?;
        if !requested.contains(id) || !seen.insert(id.to_owned()) {
            return Err(Error::Stale);
        }
        values.push(value);
    }
    if seen.len() != requested.len() {
        return Err(Error::Stale);
    }
    Ok(values)
}

fn owner(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Symfony project first.")?;
    let root = project.root()?;
    if !canonical(&root) || !host::projects()?.iter().any(|project| project.root().ok().as_deref() == Some(&root)) {
        return Err(Error::Invalid("Docker targets work only for projects in a local macOS or Linux folder.".into()));
    }
    Ok((project, root))
}

fn storage_denied(error: Error) -> Error {
    ui_kit::denied::map(error, "Saving a Docker target", "storage", "Symfony")
}

/// Symfony: Choose Docker Target… — lists running containers that mount this project and saves the picked one for it.
pub(crate) fn choose(ctx: &Invocation) -> Result<Option<String>, Error> {
    let _task = host::begin_task("Looking for Docker containers", true);
    choose_with(ctx, run).map_err(|error| match error {
        Error::Stale => Error::Invalid("The project, the Docker settings or the containers changed while choosing. Try again.".into()),
        other => other,
    })
}

fn choose_with(ctx: &Invocation, mut run: impl FnMut(&Project, &[String]) -> Result<String, Error>) -> Result<Option<String>, Error> {
    let (project, root) = owner(ctx)?;
    let baseline = settings();
    let socket = baseline["socket"].as_str().unwrap_or_default().to_owned();
    Target::validate(&socket, &"a".repeat(64), "/workspace")?;
    let before = host::storage::get(&key(&root)).map_err(storage_denied)?;
    let unchanged = || -> Result<bool, Error> {
        Ok(owner(ctx)?.1 == root && settings() == baseline && host::storage::get(&key(&root)).map_err(storage_denied)? == before)
    };
    let docker_error = |error: Error| match error {
        Error::Failed(reason) => Error::Invalid(format!("Docker didn't answer on {socket} ({reason}). Start Docker, or change the Docker socket in Settings › Plugins › Symfony.")),
        other => other,
    };
    let ids = ids(&run(&project, &ps_args(&socket)).map_err(docker_error)?)?;
    if !unchanged()? {
        return Err(Error::Stale);
    }
    if ids.is_empty() {
        return Err(Error::Invalid(format!("No containers are running on {socket}. Start the project's containers, then try again.")));
    }
    let values = values(&run(&project, &scan_args(&socket, &ids)).map_err(docker_error)?, &ids)?;
    if !unchanged()? {
        return Err(Error::Stale);
    }
    let candidates = candidates(&root, &socket, &values);
    if candidates.is_empty() {
        return Err(Error::Invalid("No running container mounts this project. Compose and DDEV bind mounts and DDEV project volumes work; start such a container, or set the container ID and project folder in Settings › Plugins › Symfony.".into()));
    }
    let items: Vec<_> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| PickItem::new(&format!("target:{index}"), candidate.target.label.as_deref().unwrap_or_default()).description(&candidate.detail))
        .collect();
    let Some(chosen) = host::pick("Run console commands in which container?", &items)? else {
        return Ok(None);
    };
    let candidate = chosen.strip_prefix("target:").and_then(|index| index.parse::<usize>().ok()).and_then(|index| candidates.get(index)).ok_or(Error::Stale)?;
    if !unchanged()? {
        return Err(Error::Stale);
    }
    let target = &candidate.target;
    let fresh = proof(&run(&project, &target.inspect_args()).map_err(docker_error)?, &root, target)?;
    if fresh != candidate.proof || !unchanged()? {
        return Err(Error::Stale);
    }
    let envelope = json!({"schema": 1, "root": root, "settings": baseline, "target": target.to_json(), "label": target.label, "proof": fresh});
    let raw = envelope.to_string();
    if raw.len() > MAX_SAVED {
        return Err(Error::Invalid("This container has too many mounts to save (over 64 KiB). Set it in Settings › Plugins › Symfony instead.".into()));
    }
    host::storage::set(&key(&root), &raw).map_err(storage_denied)?;
    host::notify(
        NotificationLevel::Info,
        crate::SOURCE,
        &format!("Console commands now run in Docker container {}. Run Symfony: Clear Docker Target to run them with local PHP again.", target.label()),
    );
    Ok(None)
}

/// Symfony: Clear Docker Target — forgets this project's chosen container.
pub(crate) fn clear(ctx: &Invocation) -> Result<Option<String>, Error> {
    let (_, root) = owner(ctx)?;
    host::storage::delete(&key(&root)).map_err(|error| ui_kit::denied::map(error, "Clearing the Docker target", "storage", "Symfony"))?;
    let next = match manual() {
        Ok(None) => "Console commands run with local PHP again.",
        _ => "The Docker container in Settings › Plugins › Symfony applies again.",
    };
    host::notify(NotificationLevel::Info, crate::SOURCE, &format!("Cleared this project's Docker target. {next}"));
    Ok(None)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    pub(crate) const ID: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    pub(crate) fn raw() -> Value {
        json!({"Id": ID, "Image": format!("sha256:{}", "a".repeat(64)), "Running": true, "Paused": false, "Restarting": false, "StartedAt": "2026-11-10T00:00:00Z", "Mounts": [{"Type": "bind", "Source": "/project", "Destination": "/app"}], "Name": "/shop-php-1", "WorkingDir": "/app", "ComposeProject": "shop", "ComposeService": "php"})
    }

    fn target(workdir: &str) -> Target {
        Target::validate(DEFAULT_SOCKET, ID, workdir).unwrap()
    }

    /// Picks the first candidate through a mocked Docker client answering `ps` then `inspect`.
    pub(crate) fn choose_first(host: &MockHost, inspect: Value) {
        host.answer_next_prompt(Some(&["target:0"]));
        let mut calls = 0;
        choose_with(&host.invocation(), |_, args| {
            calls += 1;
            assert_eq!(&args[..2], ["--host", DEFAULT_SOCKET]);
            assert!(!args.iter().any(|arg| ["exec", "run", "start", "sh", "php"].contains(&arg.as_str())));
            Ok(if calls == 1 { format!("{ID}\n") } else { inspect.to_string() })
        })
        .unwrap();
        assert_eq!(calls, 3);
    }

    #[test]
    fn validation_rejects_remote_sockets_short_ids_and_unnormalized_paths() {
        for (socket, id, dir) in [
            ("tcp://daemon:2375", ID, "/app"),
            ("ssh://remote", ID, "/app"),
            ("unix:///var/../run/docker.sock", ID, "/app"),
            (DEFAULT_SOCKET, "123abc", "/app"),
            (DEFAULT_SOCKET, &ID.to_uppercase(), "/app"),
            (DEFAULT_SOCKET, ID, "/app/../other"),
            (DEFAULT_SOCKET, ID, "relative"),
            (DEFAULT_SOCKET, ID, "/app\nother"),
        ] {
            assert!(Target::validate(socket, id, dir).is_err(), "{socket} {id} {dir}");
        }
        assert!(Target::validate(DEFAULT_SOCKET, ID, "/app").is_ok());
    }

    #[test]
    fn proof_accepts_an_exact_bind_and_rejects_other_roots_and_hidden_folders() {
        assert!(proof(&raw().to_string(), "/project", &target("/app")).is_ok());
        assert!(proof(&raw().to_string(), "/other", &target("/app")).is_err());
        for change in 0..6 {
            let mut value = raw();
            match change {
                0 => value["Running"] = json!(false),
                1 => value["Paused"] = json!(true),
                2 => value["Mounts"][0]["Type"] = json!("volume"),
                3 => value["Mounts"].as_array_mut().unwrap().push(json!({"Type": "volume", "Source": "/var/lib/v", "Destination": "/app/vendor"})),
                4 => value["Mounts"].as_array_mut().unwrap().push(json!({"Type": "bind", "Source": "/project", "Destination": "/app"})),
                _ => value["Id"] = json!("b".repeat(64)),
            }
            assert!(proof(&value.to_string(), "/project", &target("/app")).is_err(), "{change}");
        }
    }

    #[test]
    fn proof_accepts_a_ddev_volume_only_for_its_own_project_at_the_ddev_folder() {
        let mut ddev = raw();
        ddev["DdevRoot"] = json!("/project");
        ddev["Mounts"] = json!([{"Type": "volume", "Source": "/var/lib/docker/volumes/shop_mutagen/_data", "Destination": "/var/www"}]);
        let found = proof(&ddev.to_string(), "/project", &target(DDEV_WORKDIR)).unwrap();
        assert_eq!(found["DdevRoot"], "/project");
        assert!(proof(&ddev.to_string(), "/elsewhere", &target(DDEV_WORKDIR)).is_err());
        assert!(proof(&ddev.to_string(), "/project", &target("/var/www/html/web")).is_err());
    }

    #[test]
    fn mount_order_does_not_change_the_proof() {
        let mut first = raw();
        first["Mounts"] = json!([{"Type": "volume", "Source": "/var/lib/cache", "Destination": "/cache"}, {"Type": "bind", "Source": "/project", "Destination": "/app"}]);
        let mut second = first.clone();
        second["Mounts"].as_array_mut().unwrap().reverse();
        assert_eq!(proof(&first.to_string(), "/project", &target("/app")).unwrap(), proof(&second.to_string(), "/project", &target("/app")).unwrap());
    }

    #[test]
    fn labels_name_candidates_but_only_mounts_admit_them() {
        let found = candidates("/project", DEFAULT_SOCKET, &[raw()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target.label(), "Compose · shop / php (abcdef012345…, /app)");
        let mut foreign = raw();
        foreign["Mounts"][0]["Source"] = json!("/elsewhere");
        assert!(candidates("/project", DEFAULT_SOCKET, &[foreign]).is_empty());
        let mut parent = raw();
        parent["Mounts"] = json!([{"Type": "bind", "Source": "/projects", "Destination": "/srv"}]);
        assert_eq!(candidates("/projects/shop", DEFAULT_SOCKET, &[parent])[0].target.workdir, "/srv/shop");
    }

    #[test]
    fn discovery_refuses_invalid_duplicate_and_foreign_answers() {
        assert!(ids("short\n").is_err());
        assert!(ids(&format!("{ID}\n{ID}\n")).is_err());
        assert!(ids(&(0..129).map(|i| format!("{i:064x}")).collect::<Vec<_>>().join("\n")).is_err());
        assert!(values(&json!({"Id": "b".repeat(64)}).to_string(), &[ID.into()]).is_err());
        assert_eq!(map_path("/projects/shop", "/projects", "/srv"), Some("/srv/shop".into()));
        assert!(map_path("/projects-shop", "/projects", "/srv").is_none());
    }

    #[test]
    fn inspect_templates_render_one_json_object() {
        for template in [PROOF_FORMAT, SCAN_FORMAT] {
            let mut rendered = String::new();
            let mut rest = template;
            while let Some(start) = rest.find("{{") {
                rendered.push_str(&rest[..start]);
                rest = &rest[start + rest[start..].find("}}").unwrap() + 2..];
                rendered.push_str("null");
            }
            rendered.push_str(rest);
            assert!(serde_json::from_str::<Value>(&rendered).is_ok(), "{rendered}");
        }
    }

    #[test]
    fn choosing_saves_per_project_and_editing_settings_or_clearing_drops_it() {
        let host = MockHost::install();
        choose_first(&host, raw());
        let project = host.invocation().project().unwrap();
        assert_eq!(current(&project).unwrap().unwrap().workdir, "/app");
        assert!(host.notifications()[0].body.starts_with("Console commands now run in Docker container Compose · shop / php"));
        assert!(host.process_runs().is_empty() && host.terminals().is_empty());

        host.set_setting("dockerWorkdir", "\"/manual\"");
        assert!(current(&project).is_err(), "half-filled settings replace the chosen target and are refused");
        host.set_setting("dockerWorkdir", "\"\"");
        clear(&host.invocation()).unwrap();
        assert!(host.storage().is_empty());
        assert_eq!(current(&project).unwrap(), None);
        assert!(host.notifications().last().unwrap().body.ends_with("Console commands run with local PHP again."));
    }

    #[test]
    fn a_restart_between_listing_and_saving_saves_nothing() {
        let host = MockHost::install();
        host.answer_next_prompt(Some(&["target:0"]));
        let mut calls = 0;
        let result = choose_with(&host.invocation(), |_, _| {
            calls += 1;
            let mut value = raw();
            if calls == 3 {
                value["StartedAt"] = json!("changed");
            }
            Ok(if calls == 1 { format!("{ID}\n") } else { value.to_string() })
        });
        assert_eq!(result, Err(Error::Stale));
        assert!(host.storage().is_empty());
    }

    #[test]
    fn verify_refuses_a_gone_or_changed_container_with_the_next_step() {
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:docker"]);
        choose_first(&host, raw());
        let project = host.invocation().project().unwrap();

        host.on_process("docker", |_, _| Ok(openidea_plugin::ProcessOutput { status: Some(1), stdout: String::new(), stderr: "Error: No such object: abc\n".into() }));
        let gone = verify(&project).unwrap_err().to_string();
        assert!(gone.contains("No such object") && gone.ends_with("run Symfony: Choose Docker Target… again."), "{gone}");

        host.on_process("docker", |_, _| {
            let mut value = raw();
            value["StartedAt"] = json!("2026-11-11T00:00:00Z");
            Ok(openidea_plugin::ProcessOutput { status: Some(0), stdout: value.to_string(), stderr: String::new() })
        });
        let restarted = verify(&project).unwrap_err().to_string();
        assert!(restarted.contains("was restarted"), "{restarted}");

        host.on_process("docker", |_, _| {
            let mut value = raw();
            value["Mounts"][0]["Source"] = json!("/elsewhere");
            Ok(openidea_plugin::ProcessOutput { status: Some(0), stdout: value.to_string(), stderr: String::new() })
        });
        let moved = verify(&project).unwrap_err().to_string();
        assert!(moved.contains("no longer matches this project"), "{moved}");
    }

    #[test]
    fn choosing_needs_the_docker_and_storage_permissions() {
        let host = MockHost::install().with_capabilities(&["project.read", "storage"]);
        let error = choose(&host.invocation()).unwrap_err().to_string();
        assert!(error.contains("needs the process:docker permission"), "{error}");
        let host = MockHost::install().with_capabilities(&["project.read", "process:docker"]);
        let error = choose(&host.invocation()).unwrap_err().to_string();
        assert!(error.contains("needs the storage permission"), "{error}");
        assert!(host.process_runs().is_empty());
    }
}
