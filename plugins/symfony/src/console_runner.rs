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

const STDERR_TAIL: usize = 16 * 1024;

/// What an ended process printed: its stdout, the last 16 KiB of its stderr, and its exit status.
pub(crate) struct Finished {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
}

/// A started process read without blocking the plugin: each [`Running::step`] takes what is ready,
/// and stops it after `budget` or past `max_output` bytes of stdout. Dropping it stops the process.
pub(crate) struct Running {
    child: host::Child,
    started: Instant,
    budget: Duration,
    max_output: usize,
    /// Names the process in the limit messages ("The request").
    what: &'static str,
    stdout: String,
    stderr: String,
    status: Option<i32>,
    /// Stopped by [`Running::reap`]; the next step reports the time limit.
    reaped: bool,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Running {
    /// Stops the process; what it printed so far stays readable through [`Running::step`].
    pub fn stop(&self) {
        let _ = self.child.kill();
    }

    /// Waits up to `wait_ms` for output; `Some` once the process has ended and its pipes are drained.
    /// A process that ended between two late steps still counts as finished.
    pub fn step(&mut self, wait_ms: u32) -> Result<Option<Finished>, Error> {
        if self.reaped {
            return Err(self.over_time());
        }
        let chunk = self.child.wait(wait_ms)?;
        if let Some(finished) = self.record(chunk)? {
            return Ok(Some(finished));
        }
        if self.started.elapsed() > self.budget {
            self.stop();
            return Err(self.over_time());
        }
        Ok(None)
    }

    /// Stops the process once it is past its time limit, without reading it, for a caller that
    /// nobody polls: the limit error the first time, `None` before the limit or after. No host
    /// call before the limit. The output limit needs no reaping: a step stops it as it reads.
    pub fn reap(&mut self) -> Option<Error> {
        if self.reaped || self.started.elapsed() <= self.budget {
            return None;
        }
        self.stop();
        self.reaped = true;
        Some(self.over_time())
    }

    fn over_time(&self) -> Error {
        Error::Failed(format!("{} ran longer than {} seconds and was stopped.", self.what, self.budget.as_secs()))
    }

    fn record(&mut self, chunk: host::Chunk) -> Result<Option<Finished>, Error> {
        let drained = chunk.stdout.is_empty() && chunk.stderr.is_empty();
        self.stdout.push_str(&chunk.stdout);
        if self.stdout.len() > self.max_output {
            self.stop();
            return Err(Error::Failed(format!("{} printed more than {} MiB and was stopped.", self.what, self.max_output / (1024 * 1024))));
        }
        self.stderr.push_str(&chunk.stderr);
        if self.stderr.len() > STDERR_TAIL {
            let mut cut = self.stderr.len() - STDERR_TAIL;
            while !self.stderr.is_char_boundary(cut) {
                cut += 1;
            }
            self.stderr.drain(..cut);
        }
        self.status = self.status.or(chunk.status);
        // An exit status does not mean the host's buffered pipes have drained.
        if self.status.is_none() || !drained {
            return Ok(None);
        }
        Ok(Some(Finished { stdout: std::mem::take(&mut self.stdout), stderr: std::mem::take(&mut self.stderr), status: self.status }))
    }

    /// Mock processes only end when killed; this records a clean exit with `stdout` in place of
    /// what the mock printed.
    #[cfg(test)]
    pub fn finish(&mut self, stdout: &str) {
        let _ = self.child.wait(0);
        self.stdout.clear();
        let _ = self.record(host::Chunk { status: Some(0), stdout: stdout.into(), stderr: String::new() });
    }

    /// Puts the start back past the time limit.
    #[cfg(test)]
    pub fn expire(&mut self) {
        self.started = Instant::now().checked_sub(self.budget + Duration::from_secs(1)).unwrap();
    }
}

#[cfg(test)]
thread_local! {
    /// What [`spawn`] reports for the PHP arguments it gets, in place of a mock process that only prints `started:`.
    static SCRIPT: std::cell::RefCell<Option<Box<dyn Fn(&[String]) -> ProcessOutput>>> = const { std::cell::RefCell::new(None) };
}

/// Answers every following [`spawn`] on this thread with `answer(php_args)`.
#[cfg(test)]
pub(crate) fn script(answer: impl Fn(&[String]) -> ProcessOutput + 'static) {
    SCRIPT.with(|script| *script.borrow_mut() = Some(Box::new(answer)));
}

/// Starts `php <php_args…>` through [`start`] as a [`Running`] bounded by `budget` and `max_output`.
pub(crate) fn spawn(project: &Project, php_args: &[String], budget: Duration, max_output: usize, what: &'static str) -> Result<Running, Error> {
    let child = start(project, php_args)?;
    #[allow(unused_mut)]
    let mut running = Running { child, started: Instant::now(), budget, max_output, what, stdout: String::new(), stderr: String::new(), status: None, reaped: false };
    #[cfg(test)]
    if let Some(output) = SCRIPT.with(|script| script.borrow().as_ref().map(|answer| answer(php_args))) {
        running.child.wait(0)?;
        running.record(host::Chunk { status: output.status, stdout: output.stdout, stderr: output.stderr })?;
    }
    Ok(running)
}

/// Waits for a [`Running`] process to end, stopping it when `cancelled` or at its limits.
pub(crate) fn collect_full(mut running: Running, cancelled: impl Fn() -> bool) -> Result<Finished, Error> {
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(finished) = running.step(250)? {
            return Ok(finished);
        }
    }
}

/// [`collect_full`]'s stdout.
pub(crate) fn collect(running: Running, cancelled: impl Fn() -> bool) -> Result<String, Error> {
    collect_full(running, cancelled).map(|finished| finished.stdout)
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
        (error, _) => ui_kit::denied::map(error, "Running PHP in Docker", "process:docker", "Symfony"),
    }
}

/// Parts of a key name that mark its value as secret, as runtime_logs masks log context.
const SECRET_NAMES: [&str; 11] = ["password", "passwd", "secret", "token", "key", "auth", "dsn", "credential", "cookie", "private", "salt"];

/// `scheme://user:pass@host` → `scheme://•••@host` (database.rs `scrub`).
fn scrub_urls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        out.push_str(&rest[..at + 3]);
        rest = &rest[at + 3..];
        let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '/' | '"' | '\'')).unwrap_or(rest.len());
        if let Some(credentials) = rest[..end].rfind('@') {
            out.push_str("•••");
            rest = &rest[credentials..];
        }
    }
    out.push_str(rest);
    out
}

/// Console output fit for an agent: URL credentials and the values of `KEY=value`,
/// `key: value` and `"key": value` pairs whose key names a secret are replaced.
pub(crate) fn redact(text: &str) -> String {
    let text = scrub_urls(text);
    let bytes = text.as_bytes();
    let name = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.');
    let space = |at: usize| bytes.get(at).is_some_and(|b| *b == b' ' || *b == b'\t');
    let mut out = String::new();
    let (mut cursor, mut at) = (0, 0);
    while at < bytes.len() {
        if !name(bytes[at]) || (at > 0 && name(bytes[at - 1])) {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < bytes.len() && name(bytes[end]) {
            end += 1;
        }
        let key = text[at..end].to_ascii_lowercase();
        at = end;
        let mut value = end + usize::from(matches!(bytes.get(end), Some(b'"' | b'\'')));
        while space(value) {
            value += 1;
        }
        let separator = bytes.get(value).copied();
        // `Class::method` and `scheme://` are not pairs.
        if !matches!(separator, Some(b'=' | b':')) || (separator == Some(b':') && matches!(bytes.get(value + 1), Some(b':' | b'/'))) || !SECRET_NAMES.iter().any(|secret| key.contains(secret)) {
            continue;
        }
        value += 1;
        while space(value) {
            value += 1;
        }
        if value >= bytes.len() || matches!(bytes[value], b'\n' | b'\r' | b'{' | b'[') {
            continue;
        }
        let finish = match bytes[value] {
            quote @ (b'"' | b'\'') => {
                let mut finish = value + 1;
                while finish < bytes.len() && bytes[finish] != quote && bytes[finish] != b'\n' {
                    finish += if bytes[finish] == b'\\' { 2 } else { 1 };
                }
                if bytes.get(finish) == Some(&quote) { finish + 1 } else { finish.min(bytes.len()) }
            }
            _ => value + bytes[value..].iter().position(|b| matches!(b, b'\n' | b'\r' | b',' | b'}' | b']' | b';')).unwrap_or(bytes.len() - value),
        };
        out.push_str(&text[cursor..value]);
        out.push_str("[redacted]");
        (cursor, at) = (finish, finish);
    }
    out.push_str(&text[cursor..]);
    out
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
    fn a_finished_run_keeps_its_stdout_its_stderr_tail_and_its_exit_status() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        let project = host.invocation().project().unwrap();
        script(|_| ProcessOutput { status: Some(3), stdout: "out".into(), stderr: format!("{}tail", "é".repeat(STDERR_TAIL)) });
        let finished = collect_full(spawn(&project, &strings(&["-v"]), Duration::from_secs(5), 1024, "PHP").unwrap(), || false).unwrap();
        assert_eq!((finished.stdout.as_str(), finished.status), ("out", Some(3)), "a later wait does not replace the exit status");
        assert!(finished.stderr.len() <= STDERR_TAIL && finished.stderr.ends_with("tail"));
        let cancelled = collect_full(spawn(&project, &strings(&["-v"]), Duration::from_secs(5), 1024, "PHP").unwrap(), || true);
        assert!(matches!(cancelled, Err(Error::Cancelled)));
    }

    #[test]
    fn stop_kills_the_process() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        let mut running = spawn(&host.invocation().project().unwrap(), &strings(&["-v"]), Duration::from_secs(5), 1024, "PHP").unwrap();
        assert!(running.step(0).unwrap().is_none());
        running.stop();
        let finished = (0..3).find_map(|_| running.step(0).unwrap()).expect("the process ended");
        assert_eq!(finished.status, Some(-1));
    }

    #[test]
    fn reap_stops_an_overdue_process_once_and_its_next_step_reports_the_limit() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        let mut running = spawn(&host.invocation().project().unwrap(), &strings(&["-v"]), Duration::from_secs(5), 1024, "PHP").unwrap();
        assert!(running.reap().is_none(), "within its limit");
        running.expire();
        assert_eq!(running.reap().map(|error| error.to_string()).as_deref(), Some("PHP ran longer than 5 seconds and was stopped."));
        assert!(running.reap().is_none(), "reaped once");
        assert!(matches!(running.step(0), Err(error) if error.to_string().contains("ran longer than 5 seconds")));
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
