//! Run configurations (Run tool window) for Symfony projects: serve, tests and workers.
//! PHP ones run through the project's Docker target when one is chosen. Listing runs no
//! process, so the container is not re-inspected here; `docker exec` fails if it is gone.
use openidea_plugin::{PluginRunConfiguration, Project, RunConfigurations, host};

const MARKERS: [&str; 2] = ["bin/console", "symfony.lock"];
const PHPUNIT_SCRIPTS: [&str; 2] = ["bin/phpunit", "vendor/bin/phpunit"];
const PHPUNIT_CONFIGS: [&str; 3] = ["phpunit.xml", "phpunit.xml.dist", "phpunit.dist.xml"];
const MESSENGER_CONFIGS: [&str; 2] = ["config/packages/messenger.yaml", "config/packages/messenger.yml"];
const MAX_TRANSPORTS: usize = 8;

pub(crate) fn configurations(root: &str) -> Vec<PluginRunConfiguration> {
    let Some(project) = host::project_of(root).filter(|project| project.root().ok().as_deref() == Some(root)) else {
        return Vec::new();
    };
    let exists = |file: &str| project.exists(file).unwrap_or(false);
    if !MARKERS.iter().any(|file| exists(file)) {
        return Vec::new();
    }
    let mut runs = Vec::new();
    if let Ok((program, args)) = crate::processes::argv("serve", "") {
        runs.push(run("serve", "Symfony: Serve", program, args, false));
    }
    // Invalid Docker settings: offer no PHP run rather than silently running locally.
    let Ok(docker) = crate::docker_target::current(&project) else {
        return runs;
    };
    let php = |id: &str, name: &str, php_args: Vec<String>, test: bool| match &docker {
        Some(target) => run(id, name, "docker", target.exec_args(&php_args, false), test),
        None => run(id, name, "php", php_args, test),
    };
    let address = if docker.is_some() { "0.0.0.0:8000" } else { "127.0.0.1:8000" };
    runs.push(php("serve-php", "Symfony: Serve (PHP built-in server)", strings(&["-S", address, "-t", "public"]), false));
    let script = PHPUNIT_SCRIPTS.into_iter().find(|file| exists(file));
    let config = PHPUNIT_CONFIGS.into_iter().find(|file| exists(file));
    if script.is_some() || config.is_some() {
        let mut args = vec![script.unwrap_or(PHPUNIT_SCRIPTS[0]).to_owned()];
        args.extend(config.map(|file| ["--configuration".to_owned(), file.to_owned()]).into_iter().flatten());
        runs.push(php("test", "Symfony: Test", args, true));
    }
    if !exists("bin/console") {
        return runs;
    }
    if let Ok((_, args)) = crate::processes::argv("messenger", &transports(&project).join(" ")) {
        runs.push(php("messenger", "Symfony: Messenger Worker", args, false));
    }
    let scheduler = exists("vendor/symfony/scheduler") || project.read_file("composer.json").is_ok_and(|json| json.contains("\"symfony/scheduler\""));
    if let Some(Ok((_, args))) = scheduler.then(|| crate::processes::argv("scheduler", "")) {
        runs.push(php("scheduler", "Symfony: Scheduler Worker", args, false));
    }
    runs
}

fn run(id: &str, name: &str, program: &str, args: Vec<String>, test: bool) -> PluginRunConfiguration {
    PluginRunConfiguration { id: id.into(), name: name.into(), program: program.into(), args, cwd: None, test, framework: None }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn transports(project: &Project) -> Vec<String> {
    MESSENGER_CONFIGS.iter().find_map(|file| project.read_file(file).ok()).map(|yaml| parse_transports(&yaml)).unwrap_or_default()
}

fn plain(value: &str) -> &str {
    value.split(" #").next().unwrap_or_default().trim().trim_matches(['\'', '"'])
}

/// Names in the first `transports:` block of messenger.yaml, without the failure transport
/// and `sync://` ones. ponytail: indentation scan, not a YAML parser; anchors and flow maps are skipped.
fn parse_transports(yaml: &str) -> Vec<String> {
    let mut failure = None;
    let mut names: Vec<(String, bool)> = Vec::new();
    let (mut parent, mut child, mut done) = (None::<usize>, None::<usize>, false);
    for line in yaml.lines() {
        let text = plain(line);
        if text.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let at = line.len() - line.trim_start().len();
        if let Some(value) = text.strip_prefix("failure_transport:") {
            failure.get_or_insert_with(|| plain(value).to_owned());
        }
        match parent {
            None if !done && text == "transports:" => parent = Some(at),
            Some(top) if at <= top => (parent, done) = (None, true),
            Some(_) => {
                let (key, value) = text.split_once(':').unwrap_or((text, ""));
                if at == *child.get_or_insert(at) {
                    names.push((plain(key).to_owned(), plain(value).starts_with("sync://")));
                } else if plain(key) == "dsn" && plain(value).starts_with("sync://") {
                    if let Some(last) = names.last_mut() {
                        last.1 = true;
                    }
                }
            }
            None => {}
        }
    }
    let mut kept: Vec<String> = Vec::new();
    for (name, sync) in names {
        if !sync && failure.as_deref() != Some(name.as_str()) && crate::processes::transport_name(&name) && !kept.contains(&name) {
            kept.push(name);
        }
    }
    kept.truncate(MAX_TRANSPORTS);
    kept
}

impl RunConfigurations for crate::Main {
    fn configurations(&mut self, project_root: &str) -> Vec<PluginRunConfiguration> {
        configurations(project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const MESSENGER: &str = "framework:\n    messenger:\n        failure_transport: failed\n\n        transports:\n            # https://symfony.com/doc/current/messenger.html#transport-configuration\n            async:\n                dsn: '%env(MESSENGER_TRANSPORT_DSN)%'\n                options:\n                    use_notify: true\n            'high': '%env(HIGH_DSN)%' # fast lane\n            failed: 'doctrine://default?queue_name=failed'\n            sync: 'sync://'\n            inline:\n                dsn: \"sync://\"\n\n        routing:\n            App\\Message\\Mail: async\n\nwhen@test:\n    framework:\n        messenger:\n            transports:\n                async: 'in-memory://'\n                other: 'in-memory://'\n";

    fn find<'a>(runs: &'a [PluginRunConfiguration], id: &str) -> Option<&'a PluginRunConfiguration> {
        runs.iter().find(|run| run.id == id)
    }

    #[test]
    fn symfony_projects_get_serve_and_a_messenger_worker_on_async() {
        let host = MockHost::install();
        assert!(configurations("/project").is_empty());
        host.add_file("bin/console", "#!/usr/bin/env php");
        let runs = configurations("/project");
        let ids: Vec<&str> = runs.iter().map(|run| run.id.as_str()).collect();
        assert_eq!(ids, ["serve", "serve-php", "messenger"]);
        assert_eq!((runs[0].program.as_str(), runs[0].args.clone()), ("symfony", strings(&["serve", "--no-tls"])));
        assert_eq!(runs[1].args, ["-S", "127.0.0.1:8000", "-t", "public"]);
        assert_eq!(runs[2].args, ["bin/console", "messenger:consume", "async", "-vv"]);
        assert!(runs.iter().all(|run| run.cwd.is_none() && !run.test));
        assert!(configurations("/other").is_empty());
        host.set_project_root(None);
        assert!(configurations("/project").is_empty());
    }

    #[test]
    fn symfony_lock_alone_offers_no_console_workers() {
        let host = MockHost::install();
        host.add_file("symfony.lock", "{}");
        let runs = configurations("/project");
        assert!(find(&runs, "serve").is_some() && find(&runs, "messenger").is_none());
    }

    #[test]
    fn test_prefers_bin_phpunit_and_passes_the_phpunit_configuration() {
        let host = MockHost::install();
        host.add_file("bin/console", "");
        assert!(find(&configurations("/project"), "test").is_none());
        host.add_file("phpunit.xml.dist", "<phpunit/>");
        assert_eq!(find(&configurations("/project"), "test").unwrap().args, ["bin/phpunit", "--configuration", "phpunit.xml.dist"]);
        host.add_file("vendor/bin/phpunit", "");
        host.add_file("phpunit.xml", "<phpunit/>");
        let test = configurations("/project").into_iter().find(|run| run.id == "test").unwrap();
        assert_eq!((test.program.as_str(), test.test), ("php", true));
        assert_eq!(test.args, ["vendor/bin/phpunit", "--configuration", "phpunit.xml"]);
        host.add_file("bin/phpunit", "");
        assert_eq!(find(&configurations("/project"), "test").unwrap().args[0], "bin/phpunit");
    }

    #[test]
    fn messenger_transports_skip_failure_and_sync_ones() {
        assert_eq!(parse_transports(MESSENGER), ["async", "high"]);
        assert!(parse_transports("framework:\n    messenger:\n        transports: {}\n").is_empty());
        assert!(parse_transports("framework:\n  messenger:\n    transports:\n      --evil: 'x://'\n").is_empty());
        let host = MockHost::install();
        host.add_file("bin/console", "");
        host.add_file("config/packages/messenger.yaml", MESSENGER);
        assert_eq!(find(&configurations("/project"), "messenger").unwrap().args, ["bin/console", "messenger:consume", "async", "high", "-vv"]);
    }

    #[test]
    fn scheduler_worker_only_when_symfony_scheduler_is_installed() {
        let host = MockHost::install();
        host.add_file("bin/console", "");
        assert!(find(&configurations("/project"), "scheduler").is_none());
        host.add_file("composer.json", r#"{"require":{"symfony/scheduler":"^7.1"}}"#);
        assert_eq!(find(&configurations("/project"), "scheduler").unwrap().args, ["bin/console", "messenger:consume", "scheduler_default", "-vv"]);
    }

    #[test]
    fn php_runs_go_through_the_docker_target_without_inspecting_it() {
        use crate::docker_target::tests::{ID, choose_first, raw};
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:docker"]);
        host.add_file("bin/console", "");
        host.add_file("phpunit.xml.dist", "");
        choose_first(&host, raw());
        let before = host.process_runs().len();
        let runs = configurations("/project");
        assert_eq!(host.process_runs().len(), before, "listing runs no process");
        assert_eq!(find(&runs, "serve").unwrap().program, "symfony");
        let prefix = strings(&["--host", "unix:///var/run/docker.sock", "exec", "--workdir", "/app", ID, "php"]);
        for (id, rest) in [("serve-php", strings(&["-S", "0.0.0.0:8000", "-t", "public"])), ("messenger", strings(&["bin/console", "messenger:consume", "async", "-vv"]))] {
            let run = find(&runs, id).unwrap();
            assert_eq!(run.program, "docker");
            assert_eq!(run.args, [prefix.clone(), rest].concat());
        }
        assert!(find(&runs, "test").unwrap().args.ends_with(&strings(&["php", "bin/phpunit", "--configuration", "phpunit.xml.dist"])));
    }
}
