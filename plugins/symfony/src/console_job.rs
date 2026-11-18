//! One captured `bin/console` run per project, polled without blocking the view (adapted from
//! the Laravel Artisan job): 64 KiB stdout, 16 KiB stderr, stopped after five minutes.
use openidea_plugin::{Error, Project, host};
use serde_json::{Value, json};
use std::time::Instant;

const STDOUT_LIMIT: usize = 64 * 1024;
const STDERR_LIMIT: usize = 16 * 1024;
const TIME_LIMIT_SECS: u64 = 300;

pub(crate) struct Job {
    child: Option<host::Child>,
    pub id: String,
    pub command: String,
    pub values: Value,
    argv: Vec<String>,
    runner: &'static str,
    target_label: Option<String>,
    started: Instant,
    stdout: String,
    stderr: String,
    status: Option<i32>,
    cancelled: bool,
    truncated: bool,
    elapsed: u128,
}

impl Job {
    pub fn start(project: &Project, id: String, command: String, values: Value, argv: Vec<String>) -> Result<Self, Error> {
        let target = crate::docker_target::current(project)?;
        let child = crate::console_runner::start(project, &argv)?;
        Ok(Self {
            child: Some(child),
            id,
            command,
            values,
            argv,
            runner: if target.is_some() { "docker" } else { "php" },
            target_label: target.map(|target| target.label()),
            started: Instant::now(),
            stdout: String::new(),
            stderr: String::new(),
            status: None,
            cancelled: false,
            truncated: false,
            elapsed: 0,
        })
    }

    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    pub fn succeeded(&self) -> bool {
        !self.running() && !self.cancelled && self.status == Some(0)
    }

    pub fn status(&self) -> Option<i32> {
        self.status
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    pub fn poll(&mut self, cancel: bool) -> Result<(), Error> {
        let Some(child) = self.child.as_ref() else { return Ok(()) };
        if cancel || self.started.elapsed().as_secs() >= TIME_LIMIT_SECS {
            child.kill()?;
            self.cancelled = true;
        }
        for _ in 0..8 {
            let chunk = match self.child.as_ref().map(|child| child.poll(16 * 1024)) {
                Some(Ok(chunk)) => chunk,
                Some(Err(error)) => {
                    append(&mut self.stderr, &error.to_string(), STDERR_LIMIT, &mut self.truncated);
                    self.status = Some(-1);
                    self.elapsed = self.started.elapsed().as_millis();
                    self.child = None;
                    return Err(error);
                }
                None => break,
            };
            let drained = chunk.stdout.is_empty() && chunk.stderr.is_empty();
            self.record(chunk);
            if self.child.is_none() || drained {
                break;
            }
        }
        Ok(())
    }

    /// Mock processes only end when killed; this records a clean exit with `stdout`.
    #[cfg(test)]
    pub fn finish(&mut self, stdout: &str) {
        self.record(host::Chunk { status: Some(0), stdout: stdout.into(), stderr: String::new() });
    }

    fn record(&mut self, chunk: host::Chunk) {
        let drained = chunk.stdout.is_empty() && chunk.stderr.is_empty();
        append(&mut self.stdout, &chunk.stdout, STDOUT_LIMIT, &mut self.truncated);
        append(&mut self.stderr, &chunk.stderr, STDERR_LIMIT, &mut self.truncated);
        self.elapsed = self.started.elapsed().as_millis();
        if let Some(status) = chunk.status {
            self.status = Some(status);
        }
        // An exit status does not mean the host's buffered pipes have drained.
        if self.status.is_some() && drained {
            self.child = None;
        }
    }

    pub fn json(&self) -> Value {
        let elapsed = if self.running() { self.started.elapsed().as_millis() } else { self.elapsed };
        json!({"runner":self.runner,"targetLabel":self.target_label,"id":self.id,"command":self.command,"values":self.values,"argv":self.argv,"stdout":self.stdout,"stderr":self.stderr,"running":self.running(),"status":self.status,"cancelled":self.cancelled,"truncated":self.truncated,"elapsedMs":elapsed})
    }
}

fn append(target: &mut String, text: &str, limit: usize, truncated: &mut bool) {
    let mut end = limit.saturating_sub(target.len()).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&text[..end]);
    *truncated |= end < text.len();
}

/// Project-relative files a MakerBundle run reports as `created: <path>`, at most five.
pub(crate) fn created_files(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("created:"))
        .map(str::trim)
        .filter(|path| {
            !path.is_empty()
                && path.len() <= 512
                && !path.starts_with(['/', '\\'])
                && !path.contains(':')
                && !path.chars().any(char::is_control)
                && path.split(['/', '\\']).all(|part| !part.is_empty() && part != "." && part != "..")
        })
        .take(5)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn job(host: &MockHost) -> Job {
        Job::start(&host.invocation().project().unwrap(), "job".into(), "app:check".into(), json!({}), vec!["bin/console".into(), "app:check".into()]).unwrap()
    }

    #[test]
    fn keeps_failure_output_and_cancel_is_idempotent() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        let mut job = job(&host);
        job.record(host::Chunk { status: Some(7), stdout: "partial".into(), stderr: "failure".into() });
        job.record(host::Chunk { status: Some(7), ..Default::default() });
        assert_eq!(job.json()["status"], 7);
        assert_eq!(job.json()["stderr"], "failure");
        assert!(!job.running() && !job.succeeded());
        job.poll(true).unwrap();
        assert_eq!(job.json()["runner"], "php");
    }

    #[test]
    fn output_is_clipped_at_the_limits_on_a_char_boundary() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        let mut job = job(&host);
        job.record(host::Chunk { status: None, stdout: "é".repeat(40 * 1024), stderr: "x".repeat(20 * 1024) });
        assert!(job.stdout.len() <= STDOUT_LIMIT && job.stderr.len() == STDERR_LIMIT);
        assert_eq!(job.json()["truncated"], true);
        let mut text = String::new();
        let mut clipped = false;
        append(&mut text, "a💡b", 4, &mut clipped);
        assert_eq!((text.as_str(), clipped), ("a", true));
    }

    #[test]
    fn created_files_are_relative_project_paths() {
        let output = " created: src/Controller/BlogController.php\n created: templates/blog/index.html.twig\n updated: config/routes.yaml\n created: /etc/passwd\n created: ../outside.php\n created: C:\\x.php\n";
        assert_eq!(created_files(output), ["src/Controller/BlogController.php", "templates/blog/index.html.twig"]);
    }
}
