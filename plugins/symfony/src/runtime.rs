//! Runtime tab: the tail of `var/log/<env>.log` with error stack frames (read as a file, no PHP),
//! and Messenger failed messages through `bin/console messenger:failed:*`.
use crate::runtime_logs::{self, Entry, Frame};
use openidea_plugin::views::{Body, Button, View, ViewEvent};
use openidea_plugin::{Error, FileKind, Invocation, NotificationLevel, PickItem, Project, host};
use serde_json::{Value, json};

pub(crate) const VIEW: &str = "symfony-runtime";
const TAIL_BYTES: usize = 256 * 1024;
/// The most the host lets a plugin read from one file.
const MAX_FILE: u64 = 16 << 20;
const MAX_FAILED: usize = 100;
const MAX_OUTPUT: usize = 256 * 1024;
const NO_TRANSPORT: &str = "Set framework.messenger.failure_transport (for example failure_transport: failed, with a failed transport) in config/packages/messenger.yaml, then Refresh. This needs symfony/messenger.";

#[derive(Clone, Copy, Default, PartialEq)]
enum Section {
    #[default]
    Logs,
    Failed,
}

struct Logs {
    file: String,
    /// Newest first.
    entries: Vec<Entry>,
    total: usize,
    cut: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct FailedMessage {
    id: String,
    class: String,
    failed_at: String,
    error: String,
}

enum Failed {
    Messages(Vec<FailedMessage>),
    NoTransport,
    Problem(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Retry,
    Remove,
}

#[derive(Default)]
pub(crate) struct Runtime {
    root: String,
    section: Section,
    /// Project-relative log file picked with Log File…; the APP_ENV one otherwise.
    log_file: Option<String>,
    logs: Option<Result<Logs, String>>,
    vendor_frames: bool,
    failed: Option<Failed>,
    /// Changes on each read so row keys of an earlier read are refused.
    generation: u64,
}

fn plain(error: Error) -> String {
    match error {
        Error::Invalid(message) | Error::Failed(message) | Error::NotFound(message) => message,
        Error::Denied(capability) => ui_kit::denied::message("Reading failed messages", &capability, "Symfony"),
        other => other.to_string(),
    }
}

fn env_name(value: &str) -> Option<&str> {
    let value = value.trim().trim_matches(['"', '\'']);
    (!value.is_empty() && value.len() <= 32 && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))).then_some(value)
}

/// `var/log/<APP_ENV>.log`, APP_ENV from `.env.local` then `.env`, else `dev`.
fn default_log(project: &Project) -> String {
    let env = [".env.local", ".env"]
        .iter()
        .filter_map(|file| project.read_file(file).ok())
        .find_map(|text| text.lines().rev().find_map(|line| line.trim().strip_prefix("APP_ENV=").and_then(env_name).map(str::to_owned)))
        .unwrap_or_else(|| "dev".into());
    format!("var/log/{env}.log")
}

fn read_log(project: &Project, file: &str) -> Result<Logs, String> {
    let missing = format!("{file} doesn't exist yet. Load a page or run a command in that environment, or pick another file with Log File…, then Refresh.");
    if !project.exists(file).map_err(plain)? {
        return Err(missing);
    }
    let stat = project.stat(file).map_err(plain)?;
    if stat.kind != FileKind::File {
        return Err(format!("{file} is not a file. Pick a log with Log File…."));
    }
    if stat.size > MAX_FILE {
        return Err(format!("{file} is over 16 MiB, more than the IDE lets plugins read. Empty or rotate it, then Refresh."));
    }
    let bytes = project.read_bytes(file).map_err(|error| if matches!(error, Error::NotFound(_)) { missing.clone() } else { plain(error) })?;
    let start = bytes.len().saturating_sub(TAIL_BYTES);
    let (entries, total) = runtime_logs::parse(&String::from_utf8_lossy(&bytes[start..]));
    Ok(Logs { file: file.to_owned(), entries: entries.into_iter().rev().collect(), total, cut: start > 0 })
}

/// The log files in `var/log` (the host may not list ignored folders, so the usual names are checked too).
fn log_files(project: &Project) -> Vec<String> {
    let mut files: Vec<String> = project.find_files("var/log/*.log", 50).unwrap_or_default();
    let root = project.root().unwrap_or_default();
    for file in &mut files {
        if let Some(relative) = file.strip_prefix(&format!("{root}/")) {
            *file = relative.to_owned();
        }
    }
    for env in ["dev", "test", "prod"] {
        let file = format!("var/log/{env}.log");
        if !files.contains(&file) && project.exists(&file).unwrap_or(false) {
            files.push(file);
        }
    }
    files.retain(|file| file.starts_with("var/log/") && !file.contains(".."));
    files.sort();
    files.dedup();
    files
}

fn is_vendor(relative: &str) -> bool {
    relative.starts_with("vendor/") || relative.contains("/vendor/")
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// Valid `messenger:failed:*` ids: digits, letters, `-`, `_`, `.`; never an option.
fn message_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.starts_with('-') && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The table of `messenger:failed:show`: column spans come from its `----` border line.
fn parse_failed_table(stdout: &str) -> Vec<FailedMessage> {
    let lines: Vec<&str> = stdout.lines().take(4 * MAX_FAILED + 50).collect();
    let border = |line: &str| !line.trim().is_empty() && line.chars().all(|c| c == '-' || c == ' ');
    let Some(header) = lines.iter().position(|line| {
        let line = line.trim();
        line.starts_with("Id ") && line.contains("Class") && line.contains("Failed at")
    }) else {
        return Vec::new();
    };
    let Some(spans) = lines.get(header + 1).filter(|line| border(line)).map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut spans = Vec::new();
        let mut at = 0;
        while at < chars.len() {
            if chars[at] == '-' {
                let start = at;
                while at < chars.len() && chars[at] == '-' {
                    at += 1;
                }
                spans.push((start, at));
            }
            at += 1;
        }
        spans
    }) else {
        return Vec::new();
    };
    if spans.len() != 4 {
        return Vec::new();
    }
    let mut messages: Vec<FailedMessage> = Vec::new();
    for line in lines.iter().skip(header + 2).take_while(|line| !border(line)) {
        let chars: Vec<char> = line.chars().collect();
        let cell = |column: usize| -> String {
            let (start, end) = spans[column];
            let end = if column == 3 { chars.len() } else { end.min(chars.len()) };
            chars.get(start.saturating_sub(1).min(end)..end).map_or_else(String::new, |cell| cell.iter().collect::<String>().trim().to_owned())
        };
        let id = cell(0);
        if id.is_empty() {
            // A line break inside a cell: it continues the previous row's error.
            if let Some(last) = messages.last_mut() {
                let more = cell(3);
                if !more.is_empty() && last.error.len() < 1024 {
                    last.error.push(' ');
                    last.error.push_str(&more);
                }
            }
            continue;
        }
        if message_id(&id) && messages.len() < MAX_FAILED {
            messages.push(FailedMessage { id, class: cell(1), failed_at: cell(2), error: cell(3) });
        }
    }
    messages
}

/// The `Name   value` rows of `messenger:failed:show <id>`.
fn parse_details(stdout: &str) -> Vec<(&'static str, String)> {
    const KEYS: [&str; 7] = ["Error Class", "Error Code", "Message Id", "Failed at", "Transport", "Class", "Error"];
    let mut found: Vec<(&'static str, String)> = Vec::new();
    for line in stdout.lines().take(400) {
        let line = line.trim();
        let Some((key, value)) = KEYS.iter().find_map(|key| line.strip_prefix(key).filter(|rest| rest.is_empty() || rest.starts_with("  ")).map(|rest| (*key, rest.trim()))) else {
            continue;
        };
        if !found.iter().any(|(known, _)| *known == key) {
            found.push((key, runtime_logs::display(value, 512)));
        }
    }
    found
}

/// The first error block Symfony's console prints (`In File.php line 12:` then its message), else the last line.
fn console_error(output: &str) -> String {
    let lines: Vec<&str> = output.lines().map(str::trim).collect();
    let block = lines.iter().position(|line| line.starts_with("In ") && line.ends_with(':')).map(|at| {
        lines[at + 1..].iter().skip_while(|line| line.is_empty()).take_while(|line| !line.is_empty()).copied().collect::<Vec<_>>().join(" ")
    });
    let text = block.filter(|text| !text.is_empty()).or_else(|| lines.iter().rev().find(|line| !line.is_empty()).map(|line| (*line).to_owned())).unwrap_or_default();
    runtime_logs::display(&text, 300)
}

fn no_transport(output: &str) -> bool {
    output.contains("No default failure transport is defined")
        || output.contains("\"messenger:failed:show\" is not defined")
        || output.contains("There are no commands defined in the \"messenger")
}

fn console(project: &Project, args: &[&str]) -> Result<openidea_plugin::ProcessOutput, Error> {
    if !project.exists("bin/console")? {
        return Err(Error::Invalid("This project has no bin/console.".into()));
    }
    let mut args: Vec<String> = std::iter::once("bin/console").chain(args.iter().copied()).map(str::to_owned).collect();
    args.extend(["--no-interaction".to_owned(), "--no-ansi".to_owned()]);
    let mut output = crate::console_runner::run(project, &args)?;
    for stream in [&mut output.stdout, &mut output.stderr] {
        if stream.len() > MAX_OUTPUT {
            stream.truncate(MAX_OUTPUT - (0..4).find(|back| stream.is_char_boundary(MAX_OUTPUT - back)).unwrap_or(0));
        }
    }
    Ok(output)
}

fn list_failed(project: &Project) -> Failed {
    let _task = host::begin_task("Reading failed messages", false);
    let max = format!("--max={MAX_FAILED}");
    match console(project, &["messenger:failed:show", &max]) {
        Err(error) => Failed::Problem(plain(error)),
        Ok(output) if output.status != Some(0) => {
            let text = format!("{}\n{}", output.stderr, output.stdout);
            if no_transport(&text) { Failed::NoTransport } else { Failed::Problem(format!("bin/console messenger:failed:show failed: {}", console_error(&text))) }
        }
        Ok(output) => Failed::Messages(parse_failed_table(&output.stdout)),
    }
}

impl Runtime {
    /// Forgets everything read for another project.
    fn scope(&mut self, root: &str) {
        if self.root != root {
            *self = Self { root: root.to_owned(), ..Self::default() };
        }
    }

    fn reload_logs(&mut self, project: &Project) {
        let file = self.log_file.clone().unwrap_or_else(|| default_log(project));
        self.logs = Some(read_log(project, &file));
        self.generation += 1;
    }

    fn reload_failed(&mut self, project: &Project) {
        self.failed = Some(list_failed(project));
        self.generation += 1;
    }

    fn row_index(&self, key: &str, kind: &str) -> Option<usize> {
        let rest = key.strip_prefix(kind)?.strip_prefix(':')?;
        let (generation, index) = rest.split_once('-')?;
        (generation.parse() == Ok(self.generation)).then_some(())?;
        index.parse().ok()
    }

    fn entries(&self) -> &[Entry] {
        match &self.logs {
            Some(Ok(logs)) => &logs.entries,
            _ => &[],
        }
    }

    fn messages(&self) -> &[FailedMessage] {
        match &self.failed {
            Some(Failed::Messages(messages)) => messages,
            _ => &[],
        }
    }

    /// Frames of an entry as (project path, 1-based line, call), app frames only unless vendor frames are on
    /// (or every frame is in vendor). Container paths map back through the Docker target.
    fn frames(&self, project: &Project, entry: &Entry) -> Vec<(String, u32, String)> {
        let docker = crate::docker_target::current(project).ok().flatten();
        let root = &self.root;
        let resolve = |frame: &Frame| -> Option<(String, u32, String)> {
            let path = if crate::under(&frame.path, root) { frame.path.clone() } else { docker.as_ref()?.host_path(root, &frame.path)? };
            let relative = path.strip_prefix(&format!("{root}/"))?.to_owned();
            if relative.split('/').any(|part| matches!(part, "" | "." | "..")) || !project.exists(&path).unwrap_or(false) {
                return None;
            }
            Some((relative, frame.line, frame.call.clone()))
        };
        let all: Vec<_> = entry.frames.iter().filter_map(resolve).collect();
        let app: Vec<_> = all.iter().filter(|(relative, _, _)| !is_vendor(relative)).cloned().collect();
        if self.vendor_frames || app.is_empty() { all } else { app }
    }

    fn details(&self, entry: &Entry, file: &str) -> Vec<Value> {
        let mut details = vec![json!({"label": "Message", "value": entry.message})];
        if !entry.exceptions.is_empty() {
            let chain: Vec<String> = entry.exceptions.iter().enumerate().map(|(at, e)| format!("{}{}: {}", if at == 0 { "" } else { "Caused by " }, e.class, e.message)).collect();
            details.push(json!({"label": "Exception", "value": chain.join("\n")}));
        }
        if !entry.frames.is_empty() {
            let shown: Vec<String> = entry
                .frames
                .iter()
                .map(|frame| (frame.path.strip_prefix(&format!("{}/", self.root)).unwrap_or(&frame.path), frame))
                .filter(|(path, _)| self.vendor_frames || !is_vendor(path))
                .map(|(path, frame)| if frame.call.is_empty() { format!("{path}:{}", frame.line) } else { format!("{path}:{} {}", frame.line, frame.call) })
                .collect();
            let hidden = entry.frames.len() - shown.len();
            let mut value = shown.join("\n");
            if hidden > 0 {
                value.push_str(&format!("{}{hidden} vendor frames hidden", if value.is_empty() { "" } else { "\n" }));
            }
            details.push(json!({"label": "Stack frames", "value": value}));
        }
        for (label, value) in [("Context", &entry.context), ("Extra", &entry.extra)] {
            if !value.is_empty() {
                details.push(json!({"label": label, "value": value}));
            }
        }
        details.push(json!({"label": "File", "value": file}));
        details
    }

    fn log_view(&mut self, project: &Project) -> View {
        if self.logs.is_none() {
            self.reload_logs(project);
        }
        let refresh = Button::new("runtime-refresh", "Refresh").icon("refresh");
        let toolbar = |view: View| {
            view.button(refresh.clone())
                .button(Button::new("runtime-log-file", "Log File…").icon("file-log"))
                .button(Button::new("runtime-vendor-frames", if self.vendor_frames { "Hide Vendor Frames" } else { "Show Vendor Frames" }).icon("library"))
                .button(Button::new("runtime-show-failed", "Failed Messages").icon("warning"))
        };
        let (items, empty_title, empty_text, status) = match self.logs.as_ref() {
            Some(Ok(logs)) => {
                let items: Vec<Value> = logs
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(at, entry)| {
                        let kind = if entry.frames.is_empty() { "log" } else { "error" };
                        let message = first_line(&entry.message);
                        json!({"key": format!("{kind}:{}-{at}", self.generation), "label": message, "cells": [entry.level, entry.channel, entry.time, message], "activationEnabled": kind == "error", "details": self.details(entry, &logs.file)})
                    })
                    .collect();
                let shown = if logs.total > logs.entries.len() || logs.cut { format!("the newest {} entries (last 256 KiB)", logs.entries.len()) } else { format!("{} entries", logs.entries.len()) };
                let frames = if self.vendor_frames { "vendor frames shown" } else { "vendor frames hidden" };
                (items, "No log entries", format!("{} has no Monolog entries. Pick another file with Log File…, or Refresh after the app logs something.", logs.file), format!("{} · {shown} · {frames}. Refresh to update.", logs.file))
            }
            Some(Err(reason)) => (Vec::new(), "No log to show", reason.clone(), reason.clone()),
            None => (Vec::new(), "No log to show", String::new(), String::new()),
        };
        let data = json!({
            "columns": ["Level", "Channel", "Time", "Message"],
            "columnStyles": ["text", "code", "muted", "text"],
            "columnWeights": [1, 1, 2, 6],
            "facets": [0, 1],
            "searchPlaceholder": "Filter log entries…",
            "items": items,
            "emptyTitle": empty_title,
            "emptyText": empty_text,
            "emptyAction": "runtime-refresh",
        });
        toolbar(View::new(Body::Other { kind: "resources".into(), data: data.to_string() })).status(&status)
    }

    fn failed_view(&self) -> View {
        let (items, empty_title, empty_text, status): (Vec<Value>, &str, String, String) = match &self.failed {
            None => (Vec::new(), "Failed messages not read yet", "Click Refresh to read them with bin/console messenger:failed:show.".into(), String::new()),
            Some(Failed::NoTransport) => (Vec::new(), "No failure transport", NO_TRANSPORT.into(), "No failure transport is configured.".into()),
            Some(Failed::Problem(reason)) => (Vec::new(), "Couldn't read failed messages", reason.clone(), reason.clone()),
            Some(Failed::Messages(messages)) => {
                let items = messages
                    .iter()
                    .enumerate()
                    .map(|(at, message)| {
                        json!({"key": format!("failed:{}-{at}", self.generation), "label": format!("{} {}", message.id, message.class), "cells": [message.id, message.class, message.failed_at, message.error], "activationEnabled": false,
                               "details": [{"label": "Failure transport", "value": "the default failure_transport"}]})
                    })
                    .collect();
                let limit = if messages.len() == MAX_FAILED { format!(" (first {MAX_FAILED})") } else { String::new() };
                (items, "No failed messages", "Messages that fail all their retries land in the failure transport and show here.".into(), format!("{} failed messages{limit} in the default failure transport. Refresh to update.", messages.len()))
            }
        };
        let data = json!({
            "columns": ["Id", "Class", "Failed at", "Error"],
            "columnStyles": ["code", "code", "muted", "text"],
            "columnWeights": [1, 3, 2, 5],
            "facets": [1],
            "searchPlaceholder": "Filter failed messages…",
            "items": items,
            "emptyTitle": empty_title,
            "emptyText": empty_text,
            "emptyAction": "runtime-refresh",
        });
        View::new(Body::Other { kind: "resources".into(), data: data.to_string() })
            .button(Button::new("runtime-refresh", "Refresh").icon("refresh"))
            .button(Button::new("runtime-show-logs", "Logs").icon("file-log"))
            .status(&status)
    }

    pub(crate) fn render(&mut self, ctx: &Invocation) -> View {
        let Some(project) = ctx.project() else { return View::message("Open a project first.") };
        let Ok(root) = project.root() else { return View::message("Open a project first.") };
        self.scope(&root);
        match self.section {
            Section::Logs => self.log_view(&project),
            Section::Failed => self.failed_view(),
        }
    }

    pub(crate) fn handle(&mut self, event: &ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        let project = ctx.project().ok_or("Open a project first.")?;
        self.scope(&project.root()?);
        match event {
            ViewEvent::Button(id) => match id.as_str() {
                "runtime-refresh" if self.section == Section::Logs => self.reload_logs(&project),
                "runtime-refresh" => self.reload_failed(&project),
                "runtime-log-file" => self.pick_log(&project)?,
                "runtime-vendor-frames" => self.vendor_frames = !self.vendor_frames,
                "runtime-show-logs" => self.section = Section::Logs,
                "runtime-show-failed" => {
                    self.section = Section::Failed;
                    if self.failed.is_none() {
                        self.reload_failed(&project);
                    }
                }
                _ => {}
            },
            ViewEvent::Activate(key) if key.starts_with("error:") => {
                self.open_frame(ctx, Some(key))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn pick_log(&mut self, project: &Project) -> Result<(), Error> {
        let files = log_files(project);
        if files.is_empty() {
            return Err(Error::Invalid("There are no log files in var/log yet. Load a page or run a command, then try again.".into()));
        }
        let items: Vec<PickItem> = files.iter().map(|file| PickItem::new(file, file)).collect();
        let Some(file) = host::pick("Show log file", &items)? else { return Ok(()) };
        if !files.contains(&file) {
            return Err(Error::Stale);
        }
        self.log_file = Some(file);
        self.reload_logs(project);
        Ok(())
    }

    /// `Symfony: Open Error Stack Frame…`: the row's error, or a picked one, then a picked frame.
    pub(crate) fn open_frame(&mut self, ctx: &Invocation, key: Option<&str>) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("Open a project first.")?;
        self.scope(&project.root()?);
        if self.logs.is_none() {
            self.reload_logs(&project);
        }
        let index = match key.filter(|key| key.starts_with("error:")) {
            Some(key) => self.row_index(key, "error").filter(|at| *at < self.entries().len()).ok_or("This error is no longer listed. Refresh the Runtime tab.")?,
            None => {
                let items: Vec<PickItem> = self
                    .entries()
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| !entry.frames.is_empty())
                    .take(100)
                    .map(|(at, entry)| PickItem::new(&at.to_string(), &format!("{} · {} · {}", entry.time, entry.level, first_line(&entry.message))))
                    .collect();
                if items.is_empty() {
                    let file = match &self.logs {
                        Some(Ok(logs)) => logs.file.clone(),
                        Some(Err(reason)) => return Err(Error::Invalid(reason.clone())),
                        None => String::new(),
                    };
                    return Err(Error::Invalid(format!("{file} has no errors with stack frames. Pick another file with Log File… in the Runtime tab.")));
                }
                let Some(picked) = host::pick("Open stack frame of error", &items)? else { return Ok(None) };
                picked.parse().ok().filter(|at| items.iter().any(|item| item.key == picked) && *at < self.entries().len()).ok_or(Error::Stale)?
            }
        };
        let frames = self.frames(&project, &self.entries()[index]);
        let (relative, line) = match frames.as_slice() {
            [] => {
                return Err(Error::Invalid(
                    "None of this error's stack frames is a file in this project. If PHP runs in Docker, choose its container with Symfony: Choose Docker Target….".into(),
                ));
            }
            [(relative, line, _)] => (relative.clone(), *line),
            _ => {
                let items: Vec<PickItem> = frames
                    .iter()
                    .enumerate()
                    .map(|(at, (relative, line, call))| PickItem::new(&at.to_string(), &if call.is_empty() { format!("{relative}:{line}") } else { format!("{relative}:{line} · {call}") }))
                    .collect();
                let Some(picked) = host::pick("Open stack frame", &items)? else { return Ok(None) };
                let (relative, line, _) = picked.parse::<usize>().ok().and_then(|at| frames.get(at)).ok_or(Error::Stale)?;
                (relative.clone(), *line)
            }
        };
        host::open_location(&format!("{}/{relative}", self.root), line - 1, 0)?;
        Ok(None)
    }

    /// `Symfony: Show Failed Messages`: reads them and opens the tab on them.
    pub(crate) fn inspect_failed(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("Open a project first.")?;
        self.scope(&project.root()?);
        self.section = Section::Failed;
        self.reload_failed(&project);
        Ok(None)
    }

    pub(crate) fn retry(&mut self, ctx: &Invocation, key: Option<&str>) -> Result<Option<String>, Error> {
        self.act(ctx, key, Action::Retry)
    }

    pub(crate) fn remove(&mut self, ctx: &Invocation, key: Option<&str>) -> Result<Option<String>, Error> {
        self.act(ctx, key, Action::Remove)
    }

    fn act(&mut self, ctx: &Invocation, key: Option<&str>, action: Action) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("Open a project first.")?;
        self.scope(&project.root()?);
        let (verb, command) = match action {
            Action::Retry => ("Retry", "messenger:failed:retry"),
            Action::Remove => ("Remove", "messenger:failed:remove"),
        };
        let message = match key.filter(|key| key.starts_with("failed:")) {
            Some(key) => self.row_index(key, "failed").and_then(|at| self.messages().get(at)).cloned().ok_or("This failed message is no longer listed. Refresh the Failed Messages list.")?,
            None => {
                if self.failed.is_none() {
                    self.reload_failed(&project);
                }
                match &self.failed {
                    Some(Failed::NoTransport) => return Err(Error::Invalid(format!("No failure transport is configured. {NO_TRANSPORT}"))),
                    Some(Failed::Problem(reason)) => return Err(Error::Invalid(reason.clone())),
                    _ if self.messages().is_empty() => return Err(Error::Invalid("There are no failed messages. Messages that fail all their retries show in the Runtime tab.".into())),
                    _ => {}
                }
                let items: Vec<PickItem> = self.messages().iter().map(|m| PickItem::new(&m.id, &format!("{} · {} · {}", m.id, m.class, m.error))).collect();
                let Some(id) = host::pick(&format!("{verb} failed message"), &items)? else { return Ok(None) };
                self.messages().iter().find(|m| m.id == id).cloned().ok_or(Error::Stale)?
            }
        };
        if !message_id(&message.id) {
            return Err(Error::Stale);
        }
        let shown = {
            let _task = host::begin_task("Reading the failed message", false);
            console(&project, &["messenger:failed:show", &message.id])?
        };
        if shown.status != Some(0) {
            let text = format!("{}\n{}", shown.stderr, shown.stdout);
            if text.contains("was not found") {
                self.reload_failed(&project);
                return Err(Error::Invalid(format!("Failed message {} is no longer in the failure transport. The list is refreshed.", message.id)));
            }
            return Err(Error::Failed(format!("bin/console messenger:failed:show {} failed: {}", message.id, console_error(&text))));
        }
        let details = parse_details(&shown.stdout);
        let detail = |key: &str| details.iter().find(|(known, _)| *known == key).map(|(_, value)| value.clone());
        let class = detail("Class").unwrap_or(message.class.clone());
        let transport = detail("Transport").map_or_else(|| "an unknown transport".to_owned(), |transport| format!("transport {transport}"));
        let error = runtime_logs::display(&detail("Error").unwrap_or(message.error.clone()), 200);
        let effect = match action {
            Action::Retry => "Handles the message again now, in a console process. If it fails again it goes back to the failure transport.\nThe handler's side effects can't be undone.",
            Action::Remove => "Deletes it from the failure transport. This can't be undone.",
        };
        let body = format!("{class} from {transport}.\nError: {error}\n{effect}");
        if !host::confirm(&format!("{verb} failed message {}?", message.id), &body)? {
            return Ok(None);
        }
        let output = {
            let _task = host::begin_task(&format!("bin/console {command} {}", message.id), false);
            console(&project, &[command, &message.id, "--force"])?
        };
        if output.status != Some(0) {
            return Err(Error::Failed(format!("bin/console {command} {} failed: {}", message.id, console_error(&format!("{}\n{}", output.stderr, output.stdout)))));
        }
        self.reload_failed(&project);
        let done = match action {
            Action::Retry => format!("Retried failed message {}. If it failed again, it is back in the list.", message.id),
            Action::Remove => format!("Removed failed message {}.", message.id),
        };
        host::notify(NotificationLevel::Info, crate::SOURCE, &done);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::ProcessOutput;
    use openidea_plugin::mock::MockHost;

    const LOG: &str = "[2026-11-12T09:00:00+00:00] app.INFO: Started {\"token\":\"abc\"} []\n[2026-11-12T09:01:00+00:00] request.CRITICAL: Uncaught PHP Exception RuntimeException: \"Boom\" {\"exception\":\"[object] (RuntimeException(code: 0): Boom at /app/src/Controller/Home.php:3)\\n[previous exception] [object] (LogicException(code: 0): Inner at /app/vendor/acme/lib/Thing.php:2)\"} []\n";

    const TABLE: &str = "There are 2 messages pending in the failure transport.\n ---- -------------------- --------------------- ---------------------\n  Id   Class                Failed at             Error                \n ---- -------------------- --------------------- ---------------------\n  41   App\\Message\\Mail     2026-11-12 09:00:00   SMTP down            \n                                                    retry later          \n  42   App\\Message\\Resize   2026-11-12 09:05:00   Image too large      \n ---- -------------------- --------------------- ---------------------\n";

    const SHOW: &str = "Failed Message Details\n======================\n\n ------------- -------------------\n  Class         App\\Message\\Resize\n  Message Id    42\n  Failed at     2026-11-12 09:05:00\n  Error         Image too large\n  Error Code    0\n  Error Class   RuntimeException\n  Transport     async\n ------------- -------------------\n";

    fn output(stdout: &str) -> Result<ProcessOutput, Error> {
        Ok(ProcessOutput { status: Some(0), stdout: stdout.into(), stderr: String::new() })
    }

    fn items(view: &View) -> Value {
        let Body::Other { data, .. } = &view.body else { panic!("resources") };
        serde_json::from_str(data).unwrap()
    }

    #[test]
    fn logs_come_from_app_env_newest_first_with_masked_context() {
        let host = MockHost::install();
        host.add_file(".env", "APP_ENV=dev\n");
        host.add_file(".env.local", "APP_ENV=\"staging\"\n");
        host.add_file("var/log/staging.log", LOG);
        let mut runtime = Runtime::default();
        let view = runtime.render(&host.invocation());
        let data = items(&view);
        assert_eq!(data["items"][0]["cells"], json!(["CRITICAL", "request", "2026-11-12T09:01:00+00:00", "Uncaught PHP Exception RuntimeException: \"Boom\""]));
        assert!(data["items"][0]["key"].as_str().unwrap().starts_with("error:"));
        assert!(data["items"][1]["key"].as_str().unwrap().starts_with("log:"));
        let details = data["items"][1]["details"].to_string();
        assert!(details.contains("[redacted]") && !details.contains("abc"));
        assert!(view.status.unwrap().starts_with("var/log/staging.log · 2 entries"));
    }

    #[test]
    fn missing_log_explains_how_to_fill_it() {
        let host = MockHost::install();
        let data = items(&Runtime::default().render(&host.invocation()));
        assert_eq!(data["items"], json!([]));
        assert!(data["emptyText"].as_str().unwrap().starts_with("var/log/dev.log doesn't exist yet."));
        assert_eq!(data["emptyAction"], "runtime-refresh");
    }

    #[test]
    fn stack_frames_map_docker_paths_and_skip_vendor_unless_asked() {
        use crate::docker_target::tests::{choose_first, raw};
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:docker"]);
        choose_first(&host, raw());
        host.add_file("var/log/dev.log", LOG);
        host.add_file("src/Controller/Home.php", "<?php\n\nthrow new RuntimeException();\n");
        host.add_file("vendor/acme/lib/Thing.php", "<?php\nthrow 1;\n");
        let ctx = host.invocation();
        let mut runtime = Runtime::default();
        let key = items(&runtime.render(&ctx))["items"][0]["key"].as_str().unwrap().to_owned();
        runtime.open_frame(&ctx, Some(&key)).unwrap();
        assert_eq!(host.opened_locations(), [("/project/src/Controller/Home.php".to_owned(), 2, 0)]);

        runtime.handle(&ViewEvent::Button("runtime-vendor-frames".into()), &ctx).unwrap();
        host.answer_next_prompt(Some(&["1"]));
        runtime.open_frame(&ctx, Some(&key)).unwrap();
        assert_eq!(host.opened_locations()[1], ("/project/vendor/acme/lib/Thing.php".to_owned(), 1, 0));

        runtime.handle(&ViewEvent::Button("runtime-refresh".into()), &ctx).unwrap();
        assert!(runtime.open_frame(&ctx, Some(&key)).is_err(), "keys of an earlier read are refused");
    }

    #[test]
    fn palette_open_frame_picks_an_error_first() {
        let host = MockHost::install();
        host.add_file("var/log/dev.log", &LOG.replace("/app/", "/project/"));
        host.add_file("src/Controller/Home.php", "<?php\n\nthrow 1;\n");
        let mut runtime = Runtime::default();
        host.answer_next_prompt(Some(&["0"]));
        runtime.open_frame(&host.invocation(), None).unwrap();
        assert_eq!(host.opened_locations(), [("/project/src/Controller/Home.php".to_owned(), 2, 0)]);
    }

    #[test]
    fn failed_table_rows_join_wrapped_errors() {
        let messages = parse_failed_table(TABLE);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], FailedMessage { id: "41".into(), class: r"App\Message\Mail".into(), failed_at: "2026-11-12 09:00:00".into(), error: "SMTP down retry later".into() });
        assert_eq!(messages[1].error, "Image too large");
        assert!(parse_failed_table("[OK] No failed messages were found.").is_empty());
        assert_eq!(parse_details(SHOW)[..2], [("Class", r"App\Message\Resize".to_owned()), ("Message Id", "42".to_owned())]);
        assert!(console_error("\nIn DoctrineReceiver.php line 109:\n\n  Connection refused  \n  for password=hunter2  \n\nmessenger:failed:show [--max MAX]").ends_with("Connection refused for password=[redacted]"));
    }

    #[test]
    fn failed_messages_need_a_failure_transport() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        host.add_file("bin/console", "#!/usr/bin/env php");
        host.on_process("php", |_, _| Ok(ProcessOutput { status: Some(1), stdout: String::new(), stderr: "\n  Command \"messenger:failed:show\" is not defined.  \n".into() }));
        let ctx = host.invocation();
        let mut runtime = Runtime::default();
        runtime.inspect_failed(&ctx).unwrap();
        let data = items(&runtime.render(&ctx));
        assert_eq!(data["emptyTitle"], "No failure transport");
        assert!(runtime.retry(&ctx, None).unwrap_err().to_string().contains("failure_transport"));
    }

    #[test]
    fn retry_confirms_with_transport_class_and_effect_then_refreshes() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        host.add_file("bin/console", "#!/usr/bin/env php");
        host.on_process("php", |args, _| match args[1].as_str() {
            "messenger:failed:show" if args[2].starts_with("--max") => output(TABLE),
            "messenger:failed:show" => output(SHOW),
            "messenger:failed:retry" => {
                assert_eq!(args[2..], ["42", "--force", "--no-interaction", "--no-ansi"]);
                output("[OK] All done!")
            }
            other => panic!("unexpected {other}"),
        });
        let ctx = host.invocation();
        let mut runtime = Runtime::default();
        runtime.inspect_failed(&ctx).unwrap();
        let key = items(&runtime.render(&ctx))["items"][1]["key"].as_str().unwrap().to_owned();

        host.answer_next_prompt(None);
        runtime.retry(&ctx, Some(&key)).unwrap();
        assert!(!host.process_runs().iter().any(|run| run.args[1] == "messenger:failed:retry"), "a dismissed confirm runs nothing");

        host.answer_next_prompt(Some(&["ok"]));
        runtime.retry(&ctx, Some(&key)).unwrap();
        assert!(host.process_runs().iter().any(|run| run.args[1] == "messenger:failed:retry"));
        assert!(host.notifications().last().unwrap().body.starts_with("Retried failed message 42."));
    }

    #[test]
    fn remove_from_the_palette_picks_a_message() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php"]);
        host.add_file("bin/console", "#!/usr/bin/env php");
        host.on_process("php", |args, _| match args[1].as_str() {
            "messenger:failed:show" if args[2].starts_with("--max") => output(TABLE),
            "messenger:failed:show" => output(&SHOW.replace("42", "41")),
            "messenger:failed:remove" => output("[OK] Message with id \"41\" removed."),
            other => panic!("unexpected {other}"),
        });
        host.answer_next_prompt(Some(&["41"]));
        host.answer_next_prompt(Some(&["ok"]));
        Runtime::default().remove(&host.invocation(), None).unwrap();
        assert!(host.process_runs().iter().any(|run| run.args[1..3] == ["messenger:failed:remove", "41"]));
    }

    #[test]
    fn denied_php_is_a_plain_permission_state() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("bin/console", "#!/usr/bin/env php");
        let ctx = host.invocation();
        let mut runtime = Runtime::default();
        runtime.inspect_failed(&ctx).unwrap();
        let data = items(&runtime.render(&ctx));
        assert!(data["emptyText"].as_str().unwrap().contains("process:php"));
    }
}
