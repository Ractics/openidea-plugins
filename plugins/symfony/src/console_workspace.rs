//! Console and Generators tabs and the console MCP tools. Commands come from
//! `bin/console list --format=json`, read only when asked (Refresh) through `console_runner`
//! and kept per project and runner. A form per command builds the exact argv; Run captures one
//! process per project with bounded output and history, Run in Terminal opens a terminal tab.
use crate::console_job::{Job, created_files};
use crate::docker_target::Target;
use crate::{Main, SOURCE};
use openidea_plugin::views::{Body, Button, View, ViewEvent};
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Plugin, Project, host};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

pub(crate) const VIEW: &str = "symfony-console";
pub(crate) const GENERATORS_VIEW: &str = "symfony-generators";
const LIST_ARGS: [&str; 5] = ["bin/console", "list", "--format=json", "--no-interaction", "--no-ansi"];
const MAKER_MISSING: &str = "MakerBundle is not installed. Run composer require --dev symfony/maker-bundle, then click Reindex.";
const MAX_LIST_OUTPUT: usize = 4 * 1024 * 1024;
const LIST_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);
const MAX_COMMANDS: usize = 1024;
const MAX_FIELDS: usize = 128;
const MAX_PROJECTS: usize = 8;
const MAX_CATALOGS: usize = 16;
const HISTORY: usize = 20;
const HISTORY_BYTES: usize = 128 * 1024;
const MAX_REQUEST: usize = 128 * 1024;
const MAX_TOOL_RESULT: usize = 4 * 1024 * 1024;
const STALE: &str = "The command list changed. Refresh the commands and try again.";
const UNKNOWN_REQUEST: &str = "The Console tab sent a request this version does not know. Reload the Symfony plugin.";
/// Commands that print secret values: an agent never sees their output.
const SECRET_COMMANDS: [&str; 3] = ["secrets:reveal", "secrets:decrypt-to-local", "debug:dotenv"];
/// Commands that run until stopped unless given `--time-limit` or `--limit`.
const LONG_RUNNING: [&str; 5] = ["messenger:consume", "server:run", "server:start", "server:log", "serve"];

fn hash(text: &str) -> String {
    format!("{:016x}", text.bytes().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)))
}

/// The shared command workspace page with Symfony wording; the Laravel strings are catalogued
/// under the same keys, so their translation markers are dropped here.
pub(crate) fn html(view: &str) -> String {
    let (title, help, load) = match view {
        GENERATORS_VIEW => ("Load MakerBundle makers", "Load this project's makers, then select one to configure and run.", "Load makers"),
        _ => ("Load console commands", "Load this project's commands, then select one to configure and run.", "Load commands"),
    };
    let css = hash(include_str!("../web/resources/command-workspace.css"));
    let components = hash(include_str!("../web/resources/components.css"));
    let page = include_str!("../web/resources/command-workspace.html")
        .replace("./command-workspace.css", &format!("./command-workspace.css?v={css}"))
        .replace(r#"aria-label="Artisan command workspace" data-i18n-aria-label="command.workspace""#, r#"aria-label="Console command workspace""#)
        .replace(r#"<span data-i18n="command.loadTitle">Load Artisan commands</span>"#, &format!("<span>{title}</span>"))
        .replace(r#"<span data-i18n="command.loadHelp">Load this project's commands, then select one to configure and run.</span>"#, &format!("<span>{help}</span>"))
        .replace(r#"<span data-i18n="command.load">Load commands</span>"#, &format!("<span>{load}</span>"));
    format!("<link rel=\"stylesheet\" data-ui-components href=\"./components.css?v={components}\">{page}")
}

#[derive(Clone)]
struct Catalog {
    key: String,
    captured_at: u64,
    commands: Value,
    token: String,
}

impl Catalog {
    fn commands(&self, view: &str) -> Vec<&Value> {
        self.commands.as_array().into_iter().flatten().filter(|command| view != GENERATORS_VIEW || is_maker(command)).collect()
    }

    fn find(&self, view: &str, name: &str) -> Option<&Value> {
        self.commands(view).into_iter().find(|command| command["name"] == name)
    }
}

fn is_maker(command: &Value) -> bool {
    command["name"].as_str().is_some_and(|name| name.starts_with("make:"))
}

#[derive(Default)]
struct Session {
    job: Option<Job>,
    history: Vec<Value>,
    /// Counts changes to `history`, and which count each page was last sent.
    history_revision: u64,
    history_sent: BTreeMap<String, u64>,
    seen: VecDeque<String>,
    /// Command a palette pick asked a tab to select, by view.
    select: BTreeMap<String, String>,
}

#[derive(Default)]
pub(crate) struct State {
    catalogs: BTreeMap<String, Rc<Catalog>>,
    sessions: BTreeMap<String, Session>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    State,
    Refresh,
    DockerTarget,
    Runner {
        runner: String,
    },
    Preview {
        command: String,
        token: String,
        values: BTreeMap<String, Value>,
        revision: u32,
    },
    CopyOutput {
        id: String,
    },
    Run {
        id: String,
        command: String,
        token: String,
        values: BTreeMap<String, Value>,
        #[serde(default)]
        terminal: bool,
        /// Set by the MCP tool, never by the page: the run is confirmed first.
        #[serde(skip)]
        agent: bool,
    },
    Poll {
        id: String,
    },
    Cancel {
        id: String,
    },
}

fn owner(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Symfony project first.")?;
    let root = project.root()?;
    if root.is_empty() || !host::projects()?.iter().any(|candidate| candidate.root().ok().as_deref() == Some(root.as_str())) {
        return Err(STALE.into());
    }
    Ok((project, root))
}

/// `("php", None)` or `("docker", Some(label))` for this project's console runner; never runs Docker.
fn runner(target: Option<&Target>) -> (&'static str, Option<String>) {
    match target {
        Some(target) => ("docker", Some(target.label())),
        None => ("php", None),
    }
}

fn catalog_key(root: &str, target: Option<&Target>) -> String {
    let (runner, label) = runner(target);
    format!("{root}\n{runner}\n{}", label.unwrap_or_default())
}

fn storage_key(key: &str) -> String {
    format!("symfony-console-v1:{}", hash(key))
}

fn clip(value: &Value, max: usize) -> String {
    value.as_str().unwrap_or_default().chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(max).collect()
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.starts_with('-') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_:-.".contains(&b))
}

fn scalar(value: &Value) -> bool {
    value.is_null() || value.is_boolean() || value.is_number() || value.as_str().is_some_and(|text| text.len() <= 4096)
}

/// Definition entries in their declared order: Symfony writes them as a JSON object keyed by
/// name, and `serde_json::Value` would sort those keys, swapping positional arguments.
#[derive(Default)]
struct Ordered(Vec<Value>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> serde::de::Visitor<'de> for Entries {
            type Value = Ordered;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an object or array of definitions")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some((_, value)) = map.next_entry::<serde::de::IgnoredAny, Value>()? {
                    entries.push(value);
                }
                Ok(Ordered(entries))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some(value) = seq.next_element::<Value>()? {
                    entries.push(value);
                }
                Ok(Ordered(entries))
            }
            fn visit_unit<E>(self) -> Result<Ordered, E> {
                Ok(Ordered::default())
            }
        }
        deserializer.deserialize_any(Entries)
    }
}

#[derive(Deserialize, Default)]
struct Definition {
    #[serde(default)]
    arguments: Ordered,
    #[serde(default)]
    options: Ordered,
}

#[derive(Deserialize)]
struct Listed {
    #[serde(default)]
    name: Value,
    #[serde(default)]
    description: Value,
    #[serde(default)]
    hidden: Value,
    #[serde(default)]
    definition: Definition,
}

#[derive(Deserialize)]
struct List {
    #[serde(default)]
    commands: Vec<Listed>,
}

/// Visible commands of `bin/console list --format=json`, each with its arguments and options as arrays.
fn parse(raw: &str) -> Result<Value, Error> {
    // PHP notices printed before the JSON document are skipped.
    let start = if raw.trim_start().starts_with('{') { 0 } else { raw.find("\n{").map_or(0, |at| at + 1) };
    let list: List = serde_json::from_str(&raw[start..]).map_err(|_| "bin/console list did not return JSON. Run bin/console list --format=json in a terminal to see why.")?;
    let mut seen = BTreeSet::new();
    let mut commands = Vec::new();
    for command in &list.commands {
        let Some(name) = command.name.as_str().filter(|name| valid_name(name)) else { continue };
        if command.hidden == true || !seen.insert(name) || commands.len() == MAX_COMMANDS {
            continue;
        }
        let mut definition = serde_json::Map::new();
        for (kind, items) in [("arguments", &command.definition.arguments), ("options", &command.definition.options)] {
            let mut entries = Vec::new();
            for item in items.0.iter().take(MAX_FIELDS) {
                let Some(id) = item["name"].as_str().filter(|id| valid_name(id.trim_start_matches('-'))) else { continue };
                let mut entry = json!({"name": id, "description": clip(&item["description"], 2048)});
                for flag in ["is_required", "is_array", "accept_value", "is_value_required", "is_multiple"] {
                    if let Some(set) = item[flag].as_bool() {
                        entry[flag] = set.into();
                    }
                }
                if let Some(default) = item.get("default").filter(|default| scalar(default) || default.as_array().is_some_and(|values| values.len() <= 128 && values.iter().all(scalar))) {
                    entry["default"] = default.clone();
                }
                entries.push(entry);
            }
            definition.insert(kind.into(), Value::Array(entries));
        }
        commands.push(json!({"name": name, "description": clip(&command.description, 4096), "definition": definition}));
    }
    if commands.is_empty() {
        return Err("bin/console list returned no commands. Run it in a terminal to see why.".into());
    }
    Ok(Value::Array(commands))
}

fn texts(key: &str, value: Option<&Value>) -> Result<Vec<String>, Error> {
    let invalid = || Error::from(format!("{key} takes text: one value, or one per line where repeated."));
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(text)) if text.len() <= 8192 && !text.contains('\0') => Ok(if text.is_empty() { Vec::new() } else { vec![text.clone()] }),
        Some(Value::Array(values)) if values.len() <= 128 => values.iter().map(|value| value.as_str().filter(|text| text.len() <= 8192 && !text.contains('\0')).map(str::to_owned).ok_or_else(invalid)).collect(),
        _ => Err(invalid()),
    }
}

/// `bin/console <name> [--no-interaction --no-ansi] [options…] [-- arguments…]`, from the form's
/// values keyed by the definition's names. A captured run needs every required argument; a
/// terminal run leaves missing ones to the command, which may ask for them.
pub(crate) fn arguments(command: &Value, values: &BTreeMap<String, Value>, captured: bool) -> Result<Vec<String>, Error> {
    let name = command["name"].as_str().ok_or(STALE)?;
    let args = command["definition"]["arguments"].as_array().ok_or(STALE)?;
    let options = command["definition"]["options"].as_array().ok_or(STALE)?;
    if let Some(unknown) = values.keys().find(|key| !args.iter().chain(options).any(|item| item["name"] == key.as_str())) {
        return Err(format!("{name} has no argument or option {unknown}.").into());
    }
    let flag = |item: &Value, key: &str| item[key].as_bool().unwrap_or(false);
    let mut argv = vec!["bin/console".to_owned(), name.to_owned()];
    if captured {
        argv.extend(["--no-interaction".to_owned(), "--no-ansi".to_owned()]);
    }
    for item in options {
        let key = item["name"].as_str().ok_or(STALE)?;
        let option = format!("--{}", key.trim_start_matches('-'));
        if ["--no-interaction", "--ansi", "--no-ansi"].contains(&option.as_str()) {
            continue;
        }
        let value = values.get(key);
        if !flag(item, "accept_value") {
            match value {
                Some(Value::Bool(true)) => argv.push(option),
                None | Some(Value::Bool(false)) | Some(Value::Null) => {}
                _ => return Err(format!("{option} is a checkbox.").into()),
            }
            continue;
        }
        if value == Some(&Value::Bool(true)) && !flag(item, "is_value_required") {
            argv.push(option);
            continue;
        }
        let entries = texts(&option, value)?;
        if entries.len() > 1 && !flag(item, "is_multiple") {
            return Err(format!("{option} takes one value.").into());
        }
        argv.extend(entries.into_iter().map(|entry| format!("{option}={entry}")));
    }
    // `--` ends the options, so a value starting with `-` stays an argument.
    let mut positional = Vec::new();
    let mut gap = None;
    for item in args {
        let key = item["name"].as_str().ok_or(STALE)?;
        if key == "command" {
            continue;
        }
        let entries = texts(key, values.get(key))?;
        if entries.is_empty() {
            if captured && flag(item, "is_required") {
                return Err(format!("{key} is required.").into());
            }
            gap.get_or_insert(key);
            continue;
        }
        if let Some(missing) = gap {
            return Err(format!("Fill in {missing} before {key}.").into());
        }
        if entries.len() > 1 && !flag(item, "is_array") {
            return Err(format!("{key} takes one value.").into());
        }
        positional.extend(entries);
    }
    if !positional.is_empty() {
        argv.push("--".into());
        argv.extend(positional);
    }
    if argv.len() > 512 || argv.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err("The command line is too long (over 512 arguments or 64 KiB).".into());
    }
    Ok(argv)
}

/// What a captured run executes, shown before it runs: `docker exec …` for a Docker target.
fn executed(target: Option<&Target>, argv: &[String]) -> Vec<String> {
    match target {
        Some(target) => target.exec_args(argv, false),
        None => argv.to_vec(),
    }
}

/// Title and body of the confirm an agent's run needs: the exact command line, the project,
/// where PHP runs and the environment when one is given.
fn agent_confirm(root: &str, target: Option<&Target>, argv: &[String], values: &BTreeMap<String, Value>) -> (String, String) {
    let (program, place) = match target {
        Some(target) => ("docker", format!("Runs in Docker container {}.", target.label())),
        None => ("php", "Runs with local PHP.".to_owned()),
    };
    let line = shell_words::join(std::iter::once(program.to_owned()).chain(executed(target, argv)));
    let mut body = format!("{}\nProject: {root}\n{place}", ui_kit::prompts::readable_source(&line));
    if let Some(env) = values.get("--env").and_then(Value::as_str).filter(|env| !env.is_empty()) {
        body.push_str(&format!("\nEnvironment: {}", ui_kit::prompts::readable_source(env)));
    }
    (format!("Run bin/console {}?", argv.get(1).map_or("", String::as_str)), body)
}

/// Why an agent may not run `command` with `values`: it prints secrets or never stops.
fn agent_refusal(command: &str, values: &Value) -> Option<String> {
    let given = |key: &str| values.get(key).is_some_and(|value| !(value.is_null() || *value == false || *value == "" || value.as_array().is_some_and(Vec::is_empty)));
    let secret = SECRET_COMMANDS.contains(&command) || (command == "debug:container" && (given("--env-vars") || given("--env-var"))) || (command == "secrets:list" && given("--reveal"));
    if secret {
        return Some(format!("bin/console {command} prints secret values, so agents can't run it. Run it yourself in the Console tab."));
    }
    if LONG_RUNNING.contains(&command) && !given("--time-limit") && !given("--limit") {
        return Some(format!("bin/console {command} keeps running until it is stopped. Pass --time-limit or --limit, or run it yourself in the Console tab."));
    }
    None
}

fn definition<'a>(catalog: Option<&'a Catalog>, view: &str, command: &str, token: &str) -> Result<&'a Value, Error> {
    catalog.filter(|catalog| catalog.token == token).and_then(|catalog| catalog.find(view, command)).ok_or_else(|| STALE.into())
}

fn request(message: &str) -> Result<Request, Error> {
    if message.len() > MAX_REQUEST {
        return Err("The console request is too large (over 128 KiB).".into());
    }
    serde_json::from_str(message).map_err(|_| Error::Failed(UNKNOWN_REQUEST.into()))
}

fn has_maker_bundle(project: &Project) -> bool {
    ["composer.json", "composer.lock"].iter().any(|file| project.read_file(file).is_ok_and(|text| text.contains("\"symfony/maker-bundle\"")))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

impl State {
    /// The commands kept under `key` (project and runner), from memory or storage; never runs PHP.
    fn catalog(&mut self, key: &str) -> Option<Rc<Catalog>> {
        if let Some(catalog) = self.catalogs.get(key) {
            return Some(catalog.clone());
        }
        let raw = host::storage::get(&storage_key(key)).ok().flatten()?;
        let mut saved = serde_json::from_str::<Value>(&raw).ok()?;
        if saved["schema"] != 1 || saved["key"] != key || !saved["commands"].is_array() {
            return None;
        }
        let catalog = Catalog { key: key.to_owned(), captured_at: saved["capturedAt"].as_u64().unwrap_or(0), token: saved["token"].as_str().unwrap_or_default().to_owned(), commands: saved["commands"].take() };
        Some(self.remember(catalog))
    }

    /// This project's commands for its current runner; never runs PHP.
    fn project_catalog(&mut self, project: &Project) -> Result<Option<Rc<Catalog>>, Error> {
        let target = crate::docker_target::current(project)?;
        Ok(self.catalog(&catalog_key(&project.root()?, target.as_ref())))
    }

    fn remember(&mut self, catalog: Catalog) -> Rc<Catalog> {
        if !self.catalogs.contains_key(&catalog.key) && self.catalogs.len() >= MAX_CATALOGS {
            self.catalogs.pop_first();
        }
        let catalog = Rc::new(catalog);
        self.catalogs.insert(catalog.key.clone(), catalog.clone());
        catalog
    }

    /// Runs `bin/console list --format=json` and keeps the result under `key`.
    fn refresh(&mut self, project: &Project, key: String) -> Result<Rc<Catalog>, Error> {
        let output = {
            let _task = host::begin_task("Listing console commands", false);
            let run = crate::console_runner::spawn(project, &LIST_ARGS.map(str::to_owned), LIST_BUDGET, MAX_LIST_OUTPUT, "bin/console list")?;
            crate::console_runner::collect_full(run, || false)?
        };
        if output.status != Some(0) {
            let error = if output.stderr.trim().is_empty() { &output.stdout } else { &output.stderr };
            let tail: Vec<&str> = error.trim().lines().rev().take(5).collect();
            return Err(format!("bin/console list failed:\n{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n")).into());
        }
        let commands = parse(&output.stdout)?;
        let captured_at = now();
        let token = hash(&format!("{key}\n{captured_at}\n{commands}"));
        // Storage is optional: without it the list lasts until the plugin is reloaded.
        let saved = json!({"schema": 1, "key": key, "capturedAt": captured_at, "token": token, "commands": commands}).to_string();
        let _ = host::storage::set(&storage_key(&key), &saved);
        Ok(self.remember(Catalog { key, captured_at, commands, token }))
    }

    fn session(&mut self, root: &str) -> Result<&mut Session, Error> {
        if !self.sessions.contains_key(root) && self.sessions.len() >= MAX_PROJECTS {
            return Err("Too many projects use the Console tab. Reload the Symfony plugin to clear them.".into());
        }
        Ok(self.sessions.entry(root.to_owned()).or_default())
    }

    /// Stops every run past its time limit, so one is never left running without polls.
    pub(crate) fn reap(&mut self) {
        for job in self.sessions.values_mut().filter_map(|session| session.job.as_mut()).filter(|job| job.overdue()) {
            // poll() kills an overdue run; a failure is kept in the job's stderr.
            let _ = job.poll(false);
        }
    }

    /// Palette entry for a tab: picks from the loaded commands and selects the pick in `view`.
    /// `false` when no commands are loaded, so the caller keeps its own picker.
    pub(crate) fn pick_into(&mut self, ctx: &Invocation, view: &str, title: &str) -> Result<bool, Error> {
        let (project, root) = owner(ctx)?;
        let Some(catalog) = self.project_catalog(&project)? else { return Ok(false) };
        let items: Vec<PickItem> = catalog
            .commands(view)
            .iter()
            .filter_map(|command| Some(PickItem::new(command["name"].as_str()?, command["name"].as_str()?).description(command["description"].as_str().unwrap_or_default())))
            .collect();
        if items.is_empty() {
            return Ok(false);
        }
        let Some(name) = host::pick(title, &items)? else { return Ok(true) };
        if catalog.find(view, &name).is_none() {
            return Err(STALE.into());
        }
        self.session(&root)?.select.insert(view.to_owned(), name);
        ui_kit::web::reveal_view(view, include_str!("ui_config.json"))?;
        Ok(true)
    }

    pub(crate) fn render(&mut self, view: &str, ctx: &Invocation) -> View {
        self.reap();
        let Some(project) = ctx.project() else { return View::message("Open a project first.") };
        let makers = view != GENERATORS_VIEW || has_maker_bundle(&project) || self.project_catalog(&project).ok().flatten().is_some_and(|catalog| !catalog.commands(view).is_empty());
        match makers {
            true => View::new(Body::Html(html(view))),
            false => View::message(MAKER_MISSING).button(Button::new("reindex", "Reindex").icon("refresh")),
        }
    }

    #[cfg(test)]
    fn handle(&mut self, ctx: &Invocation, view: &str, message: &str) -> Result<Value, Error> {
        self.respond(ctx, view, request(message)?)
    }

    fn respond(&mut self, ctx: &Invocation, view: &str, request: Request) -> Result<Value, Error> {
        let (project, root) = owner(ctx)?;
        self.reap();
        let busy = self.session(&root)?.job.as_ref().is_some_and(Job::running);
        if matches!(request, Request::DockerTarget | Request::Runner { .. }) && busy {
            return Err("Wait for the current run to finish, or stop it, before changing where commands run.".into());
        }
        match &request {
            Request::DockerTarget => {
                crate::docker_target::choose(ctx)?;
            }
            Request::Runner { runner: wanted } => match (wanted.as_str(), crate::docker_target::current(&project)?) {
                ("docker", None) => {
                    crate::docker_target::choose(ctx)?;
                }
                ("php", Some(_)) => {
                    crate::docker_target::clear(ctx)?;
                }
                _ => {}
            },
            _ => {}
        }
        let mut response = json!({"root": root, "sessionHistory": true});
        // Polls, cancels and copies need neither the runner nor the command list.
        let mut target = None;
        let mut catalog = None;
        if !matches!(request, Request::Poll { .. } | Request::Cancel { .. } | Request::CopyOutput { .. }) {
            target = crate::docker_target::current(&project)?;
            let (runner, label) = runner(target.as_ref());
            response["runner"] = runner.into();
            response["targetLabel"] = json!(label);
            let key = catalog_key(&root, target.as_ref());
            catalog = match request {
                Request::Refresh => Some(self.refresh(&project, key)?),
                _ => self.catalog(&key),
            };
        }
        if matches!(request, Request::State | Request::Refresh | Request::DockerTarget | Request::Runner { .. }) {
            response["commands"] = json!(catalog.as_ref().map(|catalog| catalog.commands(view)).unwrap_or_default());
            response["token"] = json!(catalog.as_ref().map(|catalog| &catalog.token));
            response["capturedAt"] = json!(catalog.as_ref().map(|catalog| catalog.captured_at));
            let session = self.session(&root)?;
            if let Some(name) = session.select.remove(view).filter(|name| catalog.as_ref().is_some_and(|catalog| catalog.find(view, name).is_some())) {
                response["selectCommand"] = name.into();
            }
        }
        // The page keeps its job and history when a reply has no `job`.
        let sends_job = !matches!(request, Request::Preview { .. } | Request::CopyOutput { .. });
        let catalog = catalog.as_deref();
        let session = self.session(&root)?;
        match request {
            Request::Preview { command, token, values, revision } => {
                let argv = arguments(definition(catalog, view, &command, &token)?, &values, true)?;
                response["argv"] = executed(target.as_ref(), &argv).into();
                response["revision"] = revision.into();
            }
            Request::CopyOutput { id } => {
                let job = session.job.as_ref().filter(|job| job.id == id).ok_or(STALE)?;
                host::copy_to_clipboard(&format!("{}{}", job.stdout(), job.stderr())).map_err(|error| ui_kit::denied::map(error, "Copying the output", "clipboard", "Symfony"))?;
                response["copied"] = true.into();
            }
            Request::Run { id, command, token, values, terminal, agent } => {
                if id.len() < 16 || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return Err("The run request has an invalid ID (16 to 128 letters, digits or dashes).".into());
                }
                if session.seen.contains(&id) || busy {
                    return Err("A console command is already running in this project, or this run was already sent.".into());
                }
                let argv = arguments(definition(catalog, view, &command, &token)?, &values, !terminal)?;
                if agent {
                    let (title, body) = agent_confirm(&root, target.as_ref(), &argv, &values);
                    if !ui_kit::prompts::confirm_exact(&title, &body)? {
                        return Err(Error::Cancelled);
                    }
                }
                session.seen.push_back(id.clone());
                if session.seen.len() > 64 {
                    session.seen.pop_front();
                }
                if terminal {
                    crate::console_runner::open_terminal(&project, &format!("console {command}"), &argv)?;
                    response["terminal"] = true.into();
                } else {
                    let job = Job::start(&project, id, command, json!(values), argv)?;
                    if let Some(previous) = session.job.replace(job) {
                        let mut record = previous.json();
                        if let Some(record) = record.as_object_mut() {
                            record.remove("stdout");
                            record.remove("stderr");
                        }
                        session.history.insert(0, record);
                        session.history_revision += 1;
                        session.history.truncate(HISTORY);
                        while serde_json::to_string(&session.history).map_or(0, |raw| raw.len()) > HISTORY_BYTES {
                            session.history.pop();
                        }
                    }
                }
            }
            Request::Poll { id } => poll(session, &id, false, &mut response)?,
            Request::Cancel { id } => poll(session, &id, true, &mut response)?,
            _ => {}
        }
        if sends_job {
            response["job"] = session.job.as_ref().map_or(Value::Null, Job::json);
            response["history"] = json!(session.history);
        }
        Ok(response)
    }

    /// Leaves `history` out of a page's poll reply when that page already has it.
    fn trim_history(&mut self, view: &str, poll: bool, reply: &mut Value) {
        let Some(session) = reply["root"].as_str().and_then(|root| self.sessions.get_mut(root)) else { return };
        let Some(fields) = reply.as_object_mut().filter(|fields| fields.contains_key("history")) else { return };
        if poll && session.history_sent.get(view) == Some(&session.history_revision) {
            fields.remove("history");
        }
        session.history_sent.insert(view.to_owned(), session.history_revision);
    }

    /// The MCP console tools; `None` for other tool names. Job replies carry `completed`/`created`.
    fn tool(&mut self, tool: &str, arguments: &str, ctx: &Invocation) -> Result<Option<Value>, Error> {
        if !tool.starts_with("console_") {
            return Ok(None);
        }
        if arguments.len() > MAX_REQUEST {
            return Err("Tool arguments exceed 128 KiB.".into());
        }
        let input: Value = serde_json::from_str(if arguments.trim().is_empty() { "{}" } else { arguments }).map_err(|error| Error::Invalid(error.to_string()))?;
        let (project, _) = owner(ctx)?;
        let check_runner = |input: &Value| -> Result<(), Error> {
            let (current, _) = runner(crate::docker_target::current(&project)?.as_ref());
            let wanted = input.get("runner").map_or(Some("php"), Value::as_str);
            match wanted {
                Some(wanted) if wanted == current => Ok(()),
                Some("php" | "docker") => Err(format!("This project runs console commands with {current}. Pass runner \"{current}\", or change it with Symfony: Choose Docker Target… or Symfony: Clear Docker Target.").into()),
                _ => Err(Error::Invalid("runner must be \"php\" or \"docker\".".into())),
            }
        };
        let only = |input: &Value, allowed: &[&str]| -> Result<(), Error> {
            match input.as_object() {
                Some(fields) if fields.keys().all(|key| allowed.contains(&key.as_str())) => Ok(()),
                _ => Err(Error::Invalid(format!("Expected an object with only {}.", allowed.join(", ")))),
            }
        };
        let reply = match tool {
            "console_commands" => {
                only(&input, &["runner"])?;
                check_runner(&input)?;
                let catalog = self.project_catalog(&project)?.ok_or("No console commands are loaded. Click Load commands in the Console tab of the Symfony tool window first.")?;
                // Defaults can hold app-specific values; agents get the parameter schema only.
                let mut commands = catalog.commands.clone();
                for command in commands.as_array_mut().into_iter().flatten() {
                    for kind in ["arguments", "options"] {
                        for field in command["definition"][kind].as_array_mut().into_iter().flatten() {
                            if let Some(field) = field.as_object_mut() {
                                field.remove("default");
                            }
                        }
                    }
                }
                json!({"runner": input.get("runner").and_then(Value::as_str).unwrap_or("php"), "token": catalog.token, "capturedAt": catalog.captured_at, "commands": commands})
            }
            "console_execute" => {
                only(&input, &["runner", "command", "token", "requestId", "values"])?;
                if ["runner", "command", "token", "requestId", "values"].iter().any(|key| input.get(*key).is_none()) {
                    return Err(Error::Invalid("runner, command, token, requestId and values are required.".into()));
                }
                check_runner(&input)?;
                if let Some(refusal) = agent_refusal(input["command"].as_str().unwrap_or_default(), &input["values"]) {
                    return Err(Error::Invalid(refusal));
                }
                let request = json!({"op": "run", "id": input["requestId"], "command": input["command"], "token": input["token"], "values": input["values"]});
                let Ok(Request::Run { id, command, token, values, terminal, .. }) = serde_json::from_value(request) else {
                    return Err(Error::Invalid("requestId, command and token must be text, and values an object.".into()));
                };
                self.respond(ctx, VIEW, Request::Run { id, command, token, values, terminal, agent: true })?
            }
            "console_job" | "console_cancel" => {
                only(&input, &["id"])?;
                let id = input["id"].as_str().filter(|id| (16..=128).contains(&id.len())).ok_or_else(|| Error::Invalid("id must be 16 to 128 characters.".into()))?.to_owned();
                self.respond(ctx, VIEW, if tool == "console_job" { Request::Poll { id } } else { Request::Cancel { id } })?
            }
            _ => return Err(Error::NotFound(format!("tool {tool}"))),
        };
        Ok(Some(redacted(reply)))
    }
}

/// An agent's copy of a reply: the job output with credentials and secret values removed.
fn redacted(mut reply: Value) -> Value {
    for stream in ["stdout", "stderr"] {
        if let Some(text) = reply["job"][stream].as_str().map(crate::console_runner::redact) {
            reply["job"][stream] = text.into();
        }
    }
    reply
}

fn poll(session: &mut Session, id: &str, cancel: bool, response: &mut Value) -> Result<(), Error> {
    let job = session.job.as_mut().filter(|job| job.id == id).ok_or(STALE)?;
    let was_running = job.running();
    job.poll(cancel)?;
    if !was_running || job.running() {
        return Ok(());
    }
    if job.succeeded() {
        response["completed"] = true.into();
        response["created"] = json!(created_files(job.stdout()));
    } else if job.json()["cancelled"] == false {
        let status = job.status().map_or_else(|| "no status".to_owned(), |status| status.to_string());
        host::notify(NotificationLevel::Error, SOURCE, &format!("bin/console {} exited with status {status}. See its output in the Console tab.", job.command));
    }
    Ok(())
}

impl Main {
    pub(crate) fn render_console(&mut self, view: &str, ctx: &Invocation) -> View {
        self.console.render(view, ctx)
    }

    /// A web page message of the Console or Generators tab; the reply goes back to the page.
    pub(crate) fn handle_console(&mut self, view: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        let message = match event {
            ViewEvent::Message(message) => message,
            ViewEvent::Button(id) if id == "reindex" => {
                let result = self.run_command(&id, ctx, None);
                ui_kit::web::refresh_view(view, include_str!("ui_config.json"));
                return result.map(|_| ());
            }
            _ => return Ok(()),
        };
        let request = request(&message);
        let preview = matches!(request, Ok(Request::Preview { .. }));
        let poll = matches!(request, Ok(Request::Poll { .. }));
        let reply = match request.and_then(|request| self.console.respond(ctx, view, request)) {
            Ok(mut reply) => {
                self.after_console_run(ctx, &reply);
                self.console.trim_history(view, poll, &mut reply);
                reply
            }
            Err(error) => {
                if !preview && !matches!(error, Error::Cancelled) {
                    host::notify(NotificationLevel::Error, SOURCE, &error.to_string());
                }
                json!({"error": error.to_string()})
            }
        };
        ui_kit::web::post_view_message(view, &reply.to_string(), include_str!("ui_config.json"))
    }

    /// The console MCP tools; `None` for other tools.
    pub(crate) fn console_tool(&mut self, tool: &str, arguments: &str, ctx: &Invocation) -> Result<Option<String>, Error> {
        let Some(reply) = self.console.tool(tool, arguments, ctx)? else { return Ok(None) };
        self.after_console_run(ctx, &reply);
        let raw = reply.to_string();
        if raw.len() > MAX_TOOL_RESULT {
            return Err("Tool result exceeds 4 MiB.".into());
        }
        Ok(Some(raw))
    }

    /// After a successful captured run: open the files a maker reports as created, and reindex
    /// after a maker or a cache command; file events cover what other commands change.
    fn after_console_run(&mut self, ctx: &Invocation, reply: &Value) {
        if reply["completed"] != true {
            return;
        }
        let Some(project) = ctx.project() else { return };
        let command = reply["job"]["command"].as_str().unwrap_or_default();
        let created = reply["created"].as_array().is_some_and(|created| !created.is_empty());
        if created || command.starts_with("make:") || matches!(command, "cache:warmup" | "cache:clear") {
            self.index_project(&project);
        }
        let Ok(root) = project.root() else { return };
        for path in reply["created"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            // The output already lists every file; one that can't be opened is skipped.
            let _ = host::open_location(&format!("{}/{path}", root.trim_end_matches('/')), 0, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::{McpProvider, Plugin};

    const LIST: &str = r#"{"application":{"name":"Symfony"},"commands":[
        {"name":"about","description":"Display information","hidden":false,"definition":{"arguments":{},"options":{"help":{"name":"--help","accept_value":false,"is_value_required":false,"is_multiple":false,"description":"Display help","default":false}}}},
        {"name":"_complete","hidden":true,"definition":{"arguments":{},"options":{}}},
        {"name":"app:probe","description":"Probe","definition":{"arguments":{"name":{"name":"name","is_required":true,"is_array":false,"description":"","default":null},"tags":{"name":"tags","is_required":false,"is_array":true,"description":"","default":[]}},
            "options":{"force":{"name":"--force","accept_value":false,"is_value_required":false,"is_multiple":false,"description":"","default":false},"label":{"name":"--label","accept_value":true,"is_value_required":true,"is_multiple":false,"description":"","default":"x"},"notify":{"name":"--notify","accept_value":true,"is_value_required":false,"is_multiple":false,"description":"","default":null},"item":{"name":"--item","accept_value":true,"is_value_required":true,"is_multiple":true,"description":"","default":[]}}}},
        {"name":"make:controller","description":"Create a controller","definition":{"arguments":{"controller-class":{"name":"controller-class","is_required":true,"is_array":false,"description":"","default":null}},"options":{}}}
    ]}"#;

    fn host_with_list() -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php", "storage", "clipboard"]);
        host.add_file("bin/console", "<?php");
        list_answers(|| openidea_plugin::ProcessOutput { status: Some(0), stdout: format!("Deprecated: notice\n{LIST}"), stderr: String::new() });
        host
    }

    /// `bin/console list` prints `answer()`; the count of its runs.
    fn list_answers(answer: impl Fn() -> openidea_plugin::ProcessOutput + 'static) -> Rc<std::cell::Cell<usize>> {
        let runs = Rc::new(std::cell::Cell::new(0));
        let counted = runs.clone();
        crate::console_runner::script(move |args| {
            assert_eq!(args, LIST_ARGS);
            counted.set(counted.get() + 1);
            answer()
        });
        runs
    }

    #[test]
    fn the_list_is_read_with_its_output_capped_and_failures_name_stderr_and_status() {
        let host = host_with_list();
        let mut state = State::default();
        list_answers(|| openidea_plugin::ProcessOutput { status: Some(255), stdout: "partial".into(), stderr: "PHP Fatal error: boom\nin Kernel.php".into() });
        let error = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap_err().to_string();
        assert_eq!(error, "bin/console list failed:\nPHP Fatal error: boom\nin Kernel.php");
        list_answers(|| openidea_plugin::ProcessOutput { status: Some(0), stdout: "x".repeat(MAX_LIST_OUTPUT + 1), stderr: String::new() });
        let error = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap_err().to_string();
        assert_eq!(error, "bin/console list printed more than 4 MiB and was stopped.");
        assert!(state.catalogs.is_empty());
    }

    fn values(value: Value) -> BTreeMap<String, Value> {
        serde_json::from_value(value).unwrap()
    }

    fn probe() -> Value {
        parse(LIST).unwrap().as_array().unwrap().iter().find(|command| command["name"] == "app:probe").unwrap().clone()
    }

    fn send(state: &mut State, host: &MockHost, view: &str, request: Value) -> Result<Value, Error> {
        state.handle(&host.invocation(), view, &request.to_string())
    }

    #[test]
    fn page_uses_console_wording_and_versioned_styles() {
        for view in [VIEW, GENERATORS_VIEW] {
            let page = html(view);
            assert!(page.contains("data-command-workspace") && page.contains("command-workspace.css?v="), "{page}");
            assert!(!page.contains("Artisan"), "{page}");
        }
        assert!(html(GENERATORS_VIEW).contains("Load MakerBundle makers"));
        assert!(html(VIEW).contains("Load console commands"));
    }

    #[test]
    fn parses_visible_commands_with_definitions_as_arrays() {
        let commands = parse(&format!("Warning: x\n{LIST}")).unwrap();
        let names: Vec<_> = commands.as_array().unwrap().iter().map(|command| command["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["about", "app:probe", "make:controller"]);
        assert_eq!(probe()["definition"]["options"][1]["default"], "x");
        let ordered = parse(r#"{"commands":[{"name":"make:entity","definition":{"arguments":{"zeta":{"name":"zeta"},"alpha":{"name":"alpha"}},"options":[]}}]}"#).unwrap();
        assert_eq!(ordered[0]["definition"]["arguments"], json!([{"name": "zeta", "description": ""}, {"name": "alpha", "description": ""}]));
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"commands":[]}"#).is_err());
    }

    #[test]
    fn builds_ordered_argv_without_shell_splitting() {
        let argv = arguments(&probe(), &values(json!({"name": "-a b;$(touch nope)", "tags": ["one two", "three"], "--force": true, "--label": "x=y z", "--notify": true, "--item": ["a", "b"]})), true).unwrap();
        assert_eq!(argv, ["bin/console", "app:probe", "--no-interaction", "--no-ansi", "--force", "--label=x=y z", "--notify", "--item=a", "--item=b", "--", "-a b;$(touch nope)", "one two", "three"]);
    }

    #[test]
    fn captured_runs_need_required_arguments_and_terminal_runs_do_not() {
        for value in [json!({}), json!({"name": "x", "unknown": "y"}), json!({"name": ["a", "b"]}), json!({"name": "x", "--force": "true"}), json!({"name": "x", "--label": true}), json!({"name": "x\u{0000}"})] {
            assert!(arguments(&probe(), &values(value), true).is_err());
        }
        assert_eq!(arguments(&probe(), &values(json!({})), false).unwrap(), ["bin/console", "app:probe"]);
        let error = arguments(&probe(), &values(json!({"tags": ["a"]})), false).unwrap_err().to_string();
        assert_eq!(error, "Fill in name before tags.");
    }

    #[test]
    fn state_never_runs_php_and_refresh_loads_commands_once() {
        let host = host_with_list();
        let runs = list_answers(|| openidea_plugin::ProcessOutput { status: Some(0), stdout: LIST.into(), stderr: String::new() });
        let mut state = State::default();
        let empty = send(&mut state, &host, VIEW, json!({"op": "state"})).unwrap();
        assert_eq!((empty["commands"].clone(), empty["token"].clone()), (json!([]), Value::Null));
        assert_eq!(runs.get(), 0);

        let loaded = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap();
        assert_eq!(loaded["commands"].as_array().unwrap().len(), 3);
        assert_eq!(loaded["runner"], "php");
        assert!(!host.tasks().is_empty());
        let makers = send(&mut state, &host, GENERATORS_VIEW, json!({"op": "state"})).unwrap();
        assert_eq!(makers["commands"].as_array().unwrap().len(), 1);
        assert_eq!(makers["token"], loaded["token"]);

        // A reloaded plugin reads the saved list from storage.
        let mut fresh = State::default();
        assert_eq!(send(&mut fresh, &host, VIEW, json!({"op": "state"})).unwrap()["token"], loaded["token"]);
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn preview_is_exact_and_a_stale_token_never_runs() {
        let host = host_with_list();
        let runs = list_answers(|| openidea_plugin::ProcessOutput { status: Some(0), stdout: LIST.into(), stderr: String::new() });
        let mut state = State::default();
        let token = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap()["token"].clone();
        let preview = send(&mut state, &host, VIEW, json!({"op": "preview", "command": "app:probe", "token": token, "values": {"name": "two words"}, "revision": 4})).unwrap();
        assert_eq!(preview["argv"], json!(["bin/console", "app:probe", "--no-interaction", "--no-ansi", "--", "two words"]));
        assert_eq!(preview["revision"], 4);
        assert!(send(&mut state, &host, VIEW, json!({"op": "preview", "command": "app:probe", "token": "old", "values": {}, "revision": 5})).is_err());
        assert!(send(&mut state, &host, GENERATORS_VIEW, json!({"op": "preview", "command": "app:probe", "token": token, "values": {"name": "x"}, "revision": 6})).is_err());
        let run = send(&mut state, &host, VIEW, json!({"op": "run", "id": "execution-request-0001", "command": "app:probe", "token": "old", "values": {"name": "x"}}));
        assert!(run.is_err());
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn run_cancel_history_and_replay_are_bounded_per_project() {
        let host = host_with_list();
        let mut state = State::default();
        let token = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap()["token"].clone();
        let run = |id: &str| json!({"op": "run", "id": id, "command": "app:probe", "token": token, "values": {"name": "hello world"}});
        let started = send(&mut state, &host, VIEW, run("execution-request-0001")).unwrap();
        assert_eq!(started["job"]["running"], true);
        assert_eq!(started["job"]["argv"], json!(["bin/console", "app:probe", "--no-interaction", "--no-ansi", "--", "hello world"]));
        assert!(send(&mut state, &host, VIEW, run("execution-request-0002")).is_err(), "one run per project");
        let cancelled = send(&mut state, &host, VIEW, json!({"op": "cancel", "id": "execution-request-0001"})).unwrap();
        assert_eq!((cancelled["job"]["cancelled"].clone(), cancelled["job"]["status"].clone()), (json!(true), json!(-1)));
        assert!(send(&mut state, &host, VIEW, run("execution-request-0001")).is_err(), "replayed request");
        let second = send(&mut state, &host, VIEW, run("execution-request-0003")).unwrap();
        assert_eq!(second["history"].as_array().unwrap().len(), 1);
        assert!(second["history"][0].get("stdout").is_none());
        assert!(send(&mut state, &host, VIEW, json!({"op": "poll", "id": "unknown-request-0000"})).is_err());
        assert_eq!(send(&mut state, &host, VIEW, json!({"op": "poll", "id": "execution-request-0003"})).unwrap()["job"]["stdout"], "started: php\n");
        let copied = send(&mut state, &host, VIEW, json!({"op": "copy-output", "id": "execution-request-0003"})).unwrap();
        assert_eq!(copied["copied"], true);
        assert_eq!(host.clipboard().as_deref(), Some("started: php\n"));
    }

    #[test]
    fn page_polls_carry_the_history_only_when_that_page_lacks_it() {
        let host = host_with_list();
        let mut plugin = Main::default();
        let ctx = host.invocation();
        let mut page = |view: &str, request: Value| -> Value {
            plugin.handle_console(view, ViewEvent::Message(request.to_string()), &ctx).unwrap();
            let envelope: Value = serde_json::from_str(&host.view_messages().last().unwrap().1).unwrap();
            serde_json::from_str(envelope["message"].as_str().unwrap()).unwrap()
        };
        let token = page(VIEW, json!({"op": "refresh"}))["token"].clone();
        let run = |id: &str| json!({"op": "run", "id": id, "command": "app:probe", "token": token, "values": {"name": "x"}});
        assert_eq!(page(VIEW, run("execution-request-0001"))["history"], json!([]));
        let poll = |id: &str| json!({"op": "poll", "id": id});
        let polled = page(VIEW, poll("execution-request-0001"));
        assert!(polled.get("history").is_none() && polled["job"]["running"] == true, "{polled}");
        page(VIEW, json!({"op": "cancel", "id": "execution-request-0001"}));
        assert_eq!(page(VIEW, run("execution-request-0002"))["history"].as_array().unwrap().len(), 1);
        assert!(page(VIEW, poll("execution-request-0002")).get("history").is_none());
        assert_eq!(page(GENERATORS_VIEW, poll("execution-request-0002"))["history"].as_array().unwrap().len(), 1, "the other tab has not seen it");
        assert!(page(GENERATORS_VIEW, poll("execution-request-0002")).get("history").is_none());
    }

    #[test]
    fn terminal_runs_are_interactive_and_named() {
        let host = host_with_list();
        let mut state = State::default();
        let token = send(&mut state, &host, GENERATORS_VIEW, json!({"op": "refresh"})).unwrap()["token"].clone();
        let reply = send(&mut state, &host, GENERATORS_VIEW, json!({"op": "run", "id": "terminal-request-0001", "command": "make:controller", "token": token, "values": {}, "terminal": true})).unwrap();
        assert_eq!(reply["terminal"], true);
        let terminal = &host.terminals()[0];
        assert_eq!((terminal.title.as_str(), terminal.program.as_str()), ("console make:controller", "php"));
        assert_eq!(terminal.args, ["bin/console", "make:controller"]);
    }

    #[test]
    fn generators_tab_explains_a_missing_maker_bundle() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let mut state = State::default();
        let View { body, toolbar, .. } = state.render(GENERATORS_VIEW, &host.invocation());
        assert!(matches!(body, Body::Message(ref text) if text == MAKER_MISSING), "{body:?}");
        assert_eq!(toolbar.iter().map(|button| button.id.as_str()).collect::<Vec<_>>(), ["reindex"]);
        host.add_file("composer.json", r#"{"require-dev":{"symfony/maker-bundle":"^1.60"}}"#);
        assert!(matches!(state.render(GENERATORS_VIEW, &host.invocation()).body, Body::Html(_)));
        assert!(matches!(state.render(VIEW, &host.invocation()).body, Body::Html(_)));
    }

    #[test]
    fn palette_pick_selects_the_command_in_its_tab_once() {
        let host = host_with_list();
        let mut state = State::default();
        assert!(!state.pick_into(&host.invocation(), VIEW, "Run console command").unwrap());
        send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap();
        host.answer_next_prompt(Some(&["make:controller"]));
        assert!(state.pick_into(&host.invocation(), GENERATORS_VIEW, "Symfony: Make").unwrap());
        assert!(send(&mut state, &host, VIEW, json!({"op": "state"})).unwrap().get("selectCommand").is_none());
        assert_eq!(send(&mut state, &host, GENERATORS_VIEW, json!({"op": "state"})).unwrap()["selectCommand"], "make:controller");
        assert!(send(&mut state, &host, GENERATORS_VIEW, json!({"op": "state"})).unwrap().get("selectCommand").is_none());
    }

    #[test]
    fn a_successful_run_reports_created_files() {
        let host = host_with_list();
        let mut session = Session::default();
        let project = host.invocation().project().unwrap();
        let mut job = Job::start(&project, "maker-request-00001".into(), "make:controller".into(), json!({}), vec!["bin/console".into()]).unwrap();
        job.finish(" created: src/Controller/BlogController.php\n");
        session.job = Some(job);
        let mut response = json!({});
        // finish() leaves the job running until its final poll drains the pipes.
        poll(&mut session, "maker-request-00001", false, &mut response).unwrap();
        assert_eq!(response["completed"], true);
        assert_eq!(response["created"], json!(["src/Controller/BlogController.php"]));
    }

    #[test]
    fn mcp_tools_list_execute_poll_and_cancel_through_the_console_session() {
        let host = host_with_list();
        host.add_file("composer.json", r#"{"require": {"symfony/framework-bundle": "^7.0"}}"#);
        let mut plugin = Main::default();
        plugin.activate(&host.context("aritti.symfony")).unwrap();
        let ctx = host.invocation();
        assert!(plugin.call("console_commands", "{}", &ctx).unwrap_err().to_string().contains("Load commands"));
        plugin.console.handle(&ctx, VIEW, r#"{"op":"refresh"}"#).unwrap();
        let catalog: Value = serde_json::from_str(&plugin.call("console_commands", "{}", &ctx).unwrap()).unwrap();
        assert_eq!(catalog["runner"], "php");
        assert!(catalog["commands"][1]["definition"]["options"][1].get("default").is_none(), "defaults are not shared");
        assert!(plugin.call("console_commands", r#"{"runner":"docker"}"#, &ctx).unwrap_err().to_string().contains("runs console commands with php"));

        let execute = |id: &str, values: Value| json!({"runner": "php", "command": "app:probe", "token": catalog["token"], "requestId": id, "values": values}).to_string();
        assert!(plugin.call("console_execute", &execute("agent-execution-0001", json!({})), &ctx).is_err(), "required argument");
        host.answer_next_prompt(Some(&["ok"]));
        let started: Value = serde_json::from_str(&plugin.call("console_execute", &execute("agent-execution-0002", json!({"name": "x"})), &ctx).unwrap()).unwrap();
        assert_eq!(started["job"]["id"], "agent-execution-0002");
        assert!(plugin.call("console_execute", r#"{"runner":"php"}"#, &ctx).is_err());
        let polled: Value = serde_json::from_str(&plugin.call("console_job", r#"{"id":"agent-execution-0002"}"#, &ctx).unwrap()).unwrap();
        assert_eq!(polled["job"]["running"], true);
        let cancelled: Value = serde_json::from_str(&plugin.call("console_cancel", r#"{"id":"agent-execution-0002"}"#, &ctx).unwrap()).unwrap();
        assert_eq!(cancelled["job"]["cancelled"], true);
        assert!(plugin.call("console_job", r#"{"id":"short"}"#, &ctx).is_err());
    }

    #[test]
    fn an_agent_run_starts_only_after_the_exact_command_is_confirmed() {
        let host = host_with_list();
        let mut plugin = Main::default();
        plugin.activate(&host.context("aritti.symfony")).unwrap();
        let ctx = host.invocation();
        let token = plugin.console.handle(&ctx, VIEW, r#"{"op":"refresh"}"#).unwrap()["token"].clone();
        let execute = |id: &str| json!({"runner": "php", "command": "app:probe", "token": token, "requestId": id, "values": {"name": "x"}}).to_string();

        assert_eq!(plugin.call("console_execute", &execute("agent-execution-0001"), &ctx), Err(Error::Cancelled));
        assert!(plugin.call("console_job", r#"{"id":"agent-execution-0001"}"#, &ctx).is_err(), "nothing started");
        host.answer_next_prompt(Some(&["ok"]));
        let started: Value = serde_json::from_str(&plugin.call("console_execute", &execute("agent-execution-0001"), &ctx).unwrap()).unwrap();
        assert_eq!(started["job"]["running"], true);

        // The page's own runs never ask.
        plugin.console.handle(&ctx, VIEW, r#"{"op":"cancel","id":"agent-execution-0001"}"#).unwrap();
        let page = json!({"op": "run", "id": "page-execution-0001", "command": "app:probe", "token": token, "values": {"name": "x"}});
        assert_eq!(plugin.console.handle(&ctx, VIEW, &page.to_string()).unwrap()["job"]["running"], true);
    }

    #[test]
    fn the_agent_confirm_names_the_command_line_project_runner_and_environment() {
        let argv: Vec<String> = ["bin/console", "app:probe", "--no-interaction", "--env=prod", "--", "two words"].map(str::to_owned).to_vec();
        let (title, body) = agent_confirm("/project", None, &argv, &values(json!({"--env": "prod"})));
        assert_eq!(title, "Run bin/console app:probe?");
        assert_eq!(body, "php bin/console app:probe --no-interaction '--env=prod' -- 'two words'\nProject: /project\nRuns with local PHP.\nEnvironment: prod");
    }

    #[test]
    fn agents_cannot_run_secret_printing_or_endless_commands() {
        for (command, values) in [
            ("secrets:reveal", json!({})),
            ("secrets:decrypt-to-local", json!({})),
            ("debug:dotenv", json!({})),
            ("debug:container", json!({"--env-vars": true})),
            ("debug:container", json!({"--env-var": "APP_SECRET"})),
            ("secrets:list", json!({"--reveal": true})),
        ] {
            assert!(agent_refusal(command, &values).is_some_and(|refusal| refusal.contains("prints secret values")), "{command}");
        }
        for command in LONG_RUNNING {
            assert!(agent_refusal(command, &json!({})).is_some_and(|refusal| refusal.contains("keeps running")), "{command}");
        }
        assert!(agent_refusal("messenger:consume", &json!({"--time-limit": "60"})).is_none());
        assert!(agent_refusal("messenger:consume", &json!({"--limit": "10"})).is_none());
        assert!(agent_refusal("debug:container", &json!({"--env-vars": false})).is_none());

        let host = host_with_list();
        let mut plugin = Main::default();
        plugin.activate(&host.context("aritti.symfony")).unwrap();
        let call = json!({"runner": "php", "command": "messenger:consume", "token": "t", "requestId": "agent-execution-0001", "values": {}}).to_string();
        assert!(plugin.call("console_execute", &call, &host.invocation()).unwrap_err().to_string().contains("--time-limit"));
    }

    #[test]
    fn agents_get_job_output_without_credentials_or_secret_values() {
        let host = host_with_list();
        let project = host.invocation().project().unwrap();
        let mut job = Job::start(&project, "agent-execution-0001".into(), "debug:config".into(), json!({}), vec!["bin/console".into()]).unwrap();
        job.finish("url: mysql://app:hunter2@db:3306/app\nAPP_SECRET=abc123\n\"api_key\": \"zz top\"\nkept: visible\n");
        let reply = redacted(json!({"job": job.json()}));
        let stdout = reply["job"]["stdout"].as_str().unwrap();
        for secret in ["hunter2", "abc123", "zz top"] {
            assert!(!stdout.contains(secret), "{stdout}");
        }
        assert!(stdout.contains("kept: visible"), "{stdout}");
    }

    #[test]
    fn an_overdue_run_is_stopped_on_the_next_request() {
        let host = host_with_list();
        let mut state = State::default();
        let token = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap()["token"].clone();
        send(&mut state, &host, VIEW, json!({"op": "run", "id": "execution-request-0001", "command": "app:probe", "token": token, "values": {"name": "x"}})).unwrap();
        state.sessions.get_mut("/project").unwrap().job.as_mut().unwrap().expire();
        let reply = send(&mut state, &host, VIEW, json!({"op": "state"})).unwrap();
        assert_eq!((reply["job"]["running"].clone(), reply["job"]["cancelled"].clone()), (json!(false), json!(true)));
    }

    #[test]
    fn only_makers_and_cache_commands_reindex_after_a_run() {
        let host = host_with_list();
        host.add_file("composer.json", r#"{"require": {"symfony/framework-bundle": "^7.0"}}"#);
        let mut plugin = Main::default();
        let ctx = host.invocation();
        plugin.after_console_run(&ctx, &json!({"completed": true, "created": [], "job": {"command": "about"}}));
        assert!(plugin.indexes.is_empty(), "a plain command does not reindex");
        plugin.after_console_run(&ctx, &json!({"completed": true, "created": [], "job": {"command": "cache:clear"}}));
        assert!(!plugin.indexes.is_empty());
    }
}
