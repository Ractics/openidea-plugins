//! The one place project PHP becomes a process: local `php`, or `docker exec` into
//! this project's Docker target after a fresh check that it is still the same container.
use openidea_plugin::{Error, ProcessOutput, Project, host};
use std::time::{Duration, Instant};

/// `("php", php_args)`, or `("docker", ["--host", S, "exec", ["-it",] "--workdir", W, <id>, "php", php_args…])`
/// when this project has a Docker target. A Docker target is verified first; a gone or
/// changed container is refused with the reason and the next step.
pub(crate) fn command(project: &Project, php_args: &[String], terminal: bool) -> Result<(&'static str, Vec<String>), Error> {
    let target = match crate::docker_target::current(project)? {
        Some(_) => {
            let _task = host::begin_task("Checking the Docker container", false);
            crate::docker_target::verify(project)?
        }
        None => None,
    };
    Ok(match target {
        Some(target) => ("docker", target.exec_args(php_args, terminal)),
        None => ("php", php_args.to_vec()),
    })
}

/// Runs `php <php_args…>` to its end through [`command`], moving a long `php -r` script to stdin.
pub(crate) fn run(project: &Project, php_args: &[String]) -> Result<ProcessOutput, Error> {
    let (program, args) = command(project, php_args, false)?;
    let (args, stdin) = spill_script(args, program == "docker");
    let stdin = Some(stdin.as_str()).filter(|stdin| !stdin.is_empty());
    windowless(project, program, |program| project.run_process(program, &args, stdin)).map_err(|error| denied(error, program))
}

/// Starts `php <php_args…>` through [`command`] without waiting, its stdin already sent and closed,
/// so the caller can poll it, stop it on cancel and bound its time and output.
pub(crate) fn start(project: &Project, php_args: &[String]) -> Result<host::Child, Error> {
    let (program, args) = command(project, php_args, false)?;
    let (args, stdin) = spill_script(args, program == "docker");
    let child = windowless(project, program, |program| host::start(&host::Command::new(program).args(&args), Some(project))).map_err(|error| denied(error, program))?;
    let sent = if stdin.is_empty() { Ok(()) } else { child.write(&stdin) };
    if let Err(error) = sent.and_then(|()| child.close_stdin()) {
        let _ = child.kill();
        return Err(error);
    }
    Ok(child)
}

/// Polls a started process to its end, stopping it when `cancelled`, after `budget` or past
/// `max_output` bytes of stdout; `what` names it in those messages ("The request").
pub(crate) fn collect(child: &host::Child, cancelled: impl Fn() -> bool, budget: Duration, max_output: usize, what: &str) -> Result<String, Error> {
    let started = Instant::now();
    let mut output = String::new();
    loop {
        if cancelled() {
            let _ = child.kill();
            return Err(Error::Cancelled);
        }
        if started.elapsed() > budget {
            let _ = child.kill();
            return Err(Error::Failed(format!("{what} ran longer than {} seconds and was stopped.", budget.as_secs())));
        }
        let chunk = child.wait(250)?;
        output.push_str(&chunk.stdout);
        if output.len() > max_output {
            let _ = child.kill();
            return Err(Error::Invalid(format!("{what} printed more than {} MiB and was stopped.", max_output / (1024 * 1024))));
        }
        if chunk.status.is_some() && chunk.stdout.is_empty() && chunk.stderr.is_empty() {
            return Ok(output);
        }
    }
}

const WINDOWLESS_PHP: &str = "php-win";

fn is_windows_root(root: &str) -> bool {
    root.as_bytes().get(1) == Some(&b':') || root.starts_with("\\\\")
}

fn windowless_unavailable(error: &Error) -> bool {
    match error {
        Error::Denied(_) | Error::NotFound(_) => true,
        Error::Failed(message) => message.starts_with(&format!("process:{WINDOWLESS_PHP}")),
        _ => false,
    }
}

/// Local `php` on a Windows project tries `php-win` first, so a background run opens no console
/// window; a denied or missing `php-win` falls back to `php`. Docker runs are unchanged.
fn windowless<T>(project: &Project, program: &str, launch: impl Fn(&str) -> Result<T, Error>) -> Result<T, Error> {
    if program == "php" && project.root().is_ok_and(|root| is_windows_root(&root)) {
        match launch(WINDOWLESS_PHP) {
            Err(error) if windowless_unavailable(&error) => {}
            other => return other,
        }
    }
    launch(program)
}

/// Opens `php <php_args…>` in a terminal tab through [`command`].
pub(crate) fn open_terminal(project: &Project, title: &str, php_args: &[String]) -> Result<(), Error> {
    let (program, args) = command(project, php_args, true)?;
    host::open_terminal(title, program, &args).map_err(|error| denied(error, program))
}

/// The standard permission message for a denied `php` or `docker` launch.
pub(crate) fn denied(error: Error, program: &str) -> Error {
    match (error, program) {
        (Error::Denied(_), "php") => crate::PHP_DENIED.into(),
        (error, _) => ui_kit::denied::map(error, "Running console commands in Docker", "process:docker", "Symfony"),
    }
}

const STDIN_LOADER: &str = "eval((static function(){global $argv,$argc;$p=json_decode(stream_get_contents(STDIN),true,512,JSON_THROW_ON_ERROR);$argv=array_merge([$argv[0]],$p['input']);$argc=count($argv);return $p['script'];})());";

/// Index of the first PHP option: 0 locally, after `exec [-i|-it] --workdir W <id> php` for Docker.
fn php_options_at(args: &[String], docker: bool) -> Option<usize> {
    if !docker {
        return Some(0);
    }
    let at = if args.get(3).is_some_and(|arg| arg == "-it" || arg == "-i") { 8 } else { 7 };
    (args.get(2)? == "exec" && args.get(at - 1)? == "php").then_some(at)
}

/// The host refuses an argument over 4096 bytes, so `php -r <script> <input…>` sends the
/// script and its input as JSON over stdin (`docker exec` gets `-i` for that). Returns the
/// argv and the stdin payload, empty when nothing moved. A `-r` after other PHP arguments
/// (a console option) is left alone.
pub(crate) fn spill_script(args: Vec<String>, docker: bool) -> (Vec<String>, String) {
    let Some(flag) = php_options_at(&args, docker).filter(|at| args.get(*at).is_some_and(|arg| arg == "-r")) else {
        return (args, String::new());
    };
    let Some(script) = args.get(flag + 1) else {
        return (args, String::new());
    };
    let payload = serde_json::json!({"script": script, "input": &args[flag + 2..]}).to_string();
    let mut spilled = args[..flag].to_vec();
    if docker && args[3] != "-it" && args[3] != "-i" {
        spilled.insert(3, "-i".to_owned());
    }
    spilled.extend(["-r".to_owned(), STDIN_LOADER.to_owned()]);
    (spilled, payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker_target::tests::{ID, choose_first, raw};
    use openidea_plugin::mock::MockHost;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn docker(php: &[&str]) -> Vec<String> {
        let mut args = strings(&["--host", "unix:///var/run/docker.sock", "exec", "--workdir", "/app", ID, "php"]);
        args.extend(strings(php));
        args
    }

    #[test]
    fn local_without_a_target_and_docker_exec_with_one() {
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:docker"]);
        let project = host.invocation().project().unwrap();
        let console = strings(&["bin/console", "about"]);
        assert_eq!(command(&project, &console, false).unwrap(), ("php", console.clone()));

        choose_first(&host, raw());
        host.on_process("docker", |args, _| {
            assert_eq!(args[2], "inspect");
            Ok(ProcessOutput { status: Some(0), stdout: raw().to_string(), stderr: String::new() })
        });
        assert_eq!(command(&project, &console, false).unwrap(), ("docker", docker(&["bin/console", "about"])));
        let (_, terminal) = command(&project, &console, true).unwrap();
        assert_eq!(&terminal[..4], ["--host", "unix:///var/run/docker.sock", "exec", "-it"]);
        assert!(!terminal.iter().any(|arg| arg == "sh" || arg == "-c"));
    }

    fn started_program(host: &MockHost, root: &str) -> String {
        host.set_project_root(Some(root));
        let project = host::projects().unwrap().remove(0);
        start(&project, &["-v".to_owned()]).unwrap().wait(0).unwrap().stdout
    }

    #[test]
    fn windows_project_uses_php_win_when_granted() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php", "process:php-win"]);
        assert_eq!(started_program(&host, "C:\\Users\\dev\\shop"), "started: php-win\n");
        host.on_process("php-win", |_, _| Ok(ProcessOutput { status: Some(0), stdout: "windowless".into(), stderr: String::new() }));
        assert_eq!(run(&host::projects().unwrap().remove(0), &["-v".to_owned()]).unwrap().stdout, "windowless");
    }

    #[test]
    fn windows_project_falls_back_to_php_without_the_grant() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        assert_eq!(started_program(&host, "C:\\Users\\dev\\shop"), "started: php\n");
        host.on_process("php", |_, _| Ok(ProcessOutput { status: Some(0), stdout: "console".into(), stderr: String::new() }));
        assert_eq!(run(&host::projects().unwrap().remove(0), &["-v".to_owned()]).unwrap().stdout, "console");
    }

    #[test]
    fn unix_project_keeps_php() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php", "process:php-win"]);
        assert_eq!(started_program(&host, "/home/dev/app"), "started: php\n");
        assert!(is_windows_root("\\\\?\\UNC\\wsl\\app"));
    }

    #[test]
    fn inline_script_moves_to_stdin_and_docker_gets_interactive() {
        let (spilled, payload) = spill_script(strings(&["-r", "echo $argv[1];", "mode"]), false);
        assert_eq!(spilled, ["-r", STDIN_LOADER]);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap(), serde_json::json!({"script": "echo $argv[1];", "input": ["mode"]}));

        let (spilled, payload) = spill_script(docker(&["-r", "echo 1;"]), true);
        let mut expected = docker(&["-r", STDIN_LOADER]);
        expected.insert(3, "-i".to_owned());
        assert_eq!(spilled, expected);
        assert!(payload.contains("echo 1;"));
    }

    #[test]
    fn console_r_option_is_not_spilled() {
        let local = strings(&["bin/console", "app:x", "-r", "value"]);
        assert_eq!(spill_script(local.clone(), false), (local, String::new()));
        let remote = docker(&["bin/console", "app:x", "-r", "value"]);
        assert_eq!(spill_script(remote.clone(), true), (remote, String::new()));
    }

    #[test]
    fn real_php_runs_the_spilled_script_with_its_input() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let script = "echo $argc, '|', $argv[1], '|', strlen($argv[2]);";
        let (args, payload) = spill_script(strings(&["-r", script, "mode", &"x".repeat(20_000)]), false);
        let Ok(mut child) = Command::new("php").args(&args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() else {
            eprintln!("php is not on PATH; skipping the real loader check");
            return;
        };
        child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3|mode|20000");
    }
}
