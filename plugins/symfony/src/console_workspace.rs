//! Console and Generators tabs and the console MCP tools. Commands come from
//! `bin/console list --format=json`, read only when asked (Refresh) through `console_runner`
//! and kept per project and runner. A form per command builds the exact argv; Run captures one
//! process per project with bounded output and history, Run in Terminal opens a terminal tab.
use crate::console_job::{Job, created_files};
use crate::{Main, SOURCE};
use openidea_plugin::views::{Body, View, ViewEvent};
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Project, host};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) const VIEW: &str = "symfony-console";
pub(crate) const GENERATORS_VIEW: &str = "symfony-generators";
const LIST_ARGS: [&str; 5] = ["bin/console", "list", "--format=json", "--no-interaction", "--no-ansi"];
const MAKER_MISSING: &str = "MakerBundle is not installed. Run composer require --dev symfony/maker-bundle.";
const MAX_LIST_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_COMMANDS: usize = 1024;
const MAX_FIELDS: usize = 128;
const MAX_PROJECTS: usize = 8;
const MAX_CATALOGS: usize = 16;
const HISTORY: usize = 20;
const HISTORY_BYTES: usize = 128 * 1024;
const MAX_REQUEST: usize = 128 * 1024;
const MAX_TOOL_RESULT: usize = 4 * 1024 * 1024;
const STALE: &str = "The command list changed. Refresh the commands and try again.";

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
    seen: VecDeque<String>,
    /// Command a palette pick asked a tab to select, by view.
    select: BTreeMap<String, String>,
}

#[derive(Default)]
pub(crate) struct State {
    catalogs: BTreeMap<String, Catalog>,
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
fn runner(project: &Project) -> Result<(&'static str, Option<String>), Error> {
    Ok(match crate::docker_target::current(project)? {
        Some(target) => ("docker", Some(target.label())),
        None => ("php", None),
    })
}

fn catalog_key(root: &str, project: &Project) -> Result<String, Error> {
    let (runner, label) = runner(project)?;
    Ok(format!("{root}\n{runner}\n{}", label.unwrap_or_default()))
}

fn storage_key(key: &str) -> String {
    format!("symfony-console-v1:{}", hash(key))
}

fn clip(value: &Value, max: usize) -> String {
    value.as_str().unwrap_or_default().chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(max).collect()
}

fn valid_name(name: &str) -> bool {
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
    if raw.len() > MAX_LIST_OUTPUT {
        return Err("bin/console list returned more than 4 MiB. Run it in a terminal to see why.".into());
    }
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
fn executed(project: &Project, argv: &[String]) -> Result<Vec<String>, Error> {
    Ok(match crate::docker_target::current(project)? {
        Some(target) => target.exec_args(argv, false),
        None => argv.to_vec(),
    })
}

fn has_maker_bundle(project: &Project) -> bool {
    ["composer.json", "composer.lock"].iter().any(|file| project.read_file(file).is_ok_and(|text| text.contains("\"symfony/maker-bundle\"")))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

impl State {
    /// This project's commands for its current runner, from memory or storage; never runs PHP.
    fn catalog(&mut self, project: &Project) -> Result<Option<Catalog>, Error> {
        let key = catalog_key(&project.root()?, project)?;
        if let Some(catalog) = self.catalogs.get(&key) {
            return Ok(Some(catalog.clone()));
        }
        let Some(raw) = host::storage::get(&storage_key(&key)).ok().flatten() else { return Ok(None) };
        let Ok(saved) = serde_json::from_str::<Value>(&raw) else { return Ok(None) };
        if saved["schema"] != 1 || saved["key"] != key.as_str() || !saved["commands"].is_array() {
            return Ok(None);
        }
        let catalog = Catalog { key: key.clone(), captured_at: saved["capturedAt"].as_u64().unwrap_or(0), token: saved["token"].as_str().unwrap_or_default().to_owned(), commands: saved["commands"].clone() };
        self.remember(catalog.clone());
        Ok(Some(catalog))
    }

    fn remember(&mut self, catalog: Catalog) {
        if !self.catalogs.contains_key(&catalog.key) && self.catalogs.len() >= MAX_CATALOGS {
            self.catalogs.pop_first();
        }
        self.catalogs.insert(catalog.key.clone(), catalog);
    }

    /// Runs `bin/console list --format=json` and keeps the result for this project and runner.
    fn refresh(&mut self, project: &Project) -> Result<Catalog, Error> {
        let key = catalog_key(&project.root()?, project)?;
        let output = {
            let _task = host::begin_task("Listing console commands", false);
            crate::console_runner::run(project, &LIST_ARGS.map(str::to_owned))?
        };
        if output.status != Some(0) {
            let error = if output.stderr.trim().is_empty() { &output.stdout } else { &output.stderr };
            let tail: Vec<&str> = error.trim().lines().rev().take(5).collect();
            return Err(format!("bin/console list failed:\n{}", tail.into_iter().rev().collect::<Vec<_>>().join("\n")).into());
        }
        let commands = parse(&output.stdout)?;
        let captured_at = now();
        let token = hash(&format!("{key}\n{captured_at}\n{commands}"));
        let catalog = Catalog { key: key.clone(), captured_at, commands, token };
        // Storage is optional: without it the list lasts until the plugin is reloaded.
        let saved = json!({"schema": 1, "key": key, "capturedAt": captured_at, "token": catalog.token, "commands": catalog.commands}).to_string();
        let _ = host::storage::set(&storage_key(&key), &saved);
        self.remember(catalog.clone());
        Ok(catalog)
    }

    fn session(&mut self, root: &str) -> Result<&mut Session, Error> {
        if !self.sessions.contains_key(root) && self.sessions.len() >= MAX_PROJECTS {
            return Err("Too many projects use the Console tab. Reload the Symfony plugin to clear them.".into());
        }
        Ok(self.sessions.entry(root.to_owned()).or_default())
    }

    /// Palette entry for a tab: picks from the loaded commands and selects the pick in `view`.
    /// `false` when no commands are loaded, so the caller keeps its own picker.
    pub(crate) fn pick_into(&mut self, ctx: &Invocation, view: &str, title: &str) -> Result<bool, Error> {
        let (project, root) = owner(ctx)?;
        let Some(catalog) = self.catalog(&project)? else { return Ok(false) };
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
        let Some(project) = ctx.project() else { return View::message("Open a project first.") };
        let makers = view != GENERATORS_VIEW || has_maker_bundle(&project) || self.catalog(&project).ok().flatten().is_some_and(|catalog| !catalog.commands(view).is_empty());
        match makers {
            true => View::new(Body::Html(html(view))),
            false => View::message(MAKER_MISSING),
        }
    }

    pub(crate) fn handle(&mut self, ctx: &Invocation, view: &str, message: &str) -> Result<Value, Error> {
        if message.len() > MAX_REQUEST {
            return Err("The console request is too large (over 128 KiB).".into());
        }
        let request: Request = serde_json::from_str(message).map_err(|error| Error::Failed(format!("Unreadable console request: {error}")))?;
        let (project, root) = owner(ctx)?;
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
        let (runner, label) = runner(&project)?;
        let mut response = json!({"root": root, "runner": runner, "targetLabel": label, "sessionHistory": true});
        let catalog = match request {
            Request::Refresh => Some(self.refresh(&project)?),
            _ => self.catalog(&project)?,
        };
        if matches!(request, Request::State | Request::Refresh | Request::DockerTarget | Request::Runner { .. }) {
            response["commands"] = json!(catalog.as_ref().map(|catalog| catalog.commands(view)).unwrap_or_default());
            response["token"] = json!(catalog.as_ref().map(|catalog| &catalog.token));
            response["capturedAt"] = json!(catalog.as_ref().map(|catalog| catalog.captured_at));
            let session = self.session(&root)?;
            if let Some(name) = session.select.remove(view).filter(|name| catalog.as_ref().is_some_and(|catalog| catalog.find(view, name).is_some())) {
                response["selectCommand"] = name.into();
            }
        }
        let definition = |command: &str, token: &str| -> Result<Value, Error> {
            let catalog = catalog.as_ref().filter(|catalog| catalog.token == token).ok_or(STALE)?;
            catalog.find(view, command).cloned().ok_or_else(|| STALE.into())
        };
        let session = self.session(&root)?;
        match request {
            Request::Preview { command, token, values, revision } => {
                let argv = arguments(&definition(&command, &token)?, &values, true)?;
                response["argv"] = executed(&project, &argv)?.into();
                response["revision"] = revision.into();
            }
            Request::CopyOutput { id } => {
                let job = session.job.as_ref().filter(|job| job.id == id).ok_or(STALE)?;
                host::copy_to_clipboard(&format!("{}{}", job.stdout(), job.stderr())).map_err(|error| ui_kit::denied::map(error, "Copying the output", "clipboard", "Symfony"))?;
                response["copied"] = true.into();
            }
            Request::Run { id, command, token, values, terminal } => {
                if id.len() < 16 || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return Err("The run request has an invalid ID (16 to 128 letters, digits or dashes).".into());
                }
                if session.seen.contains(&id) || busy {
                    return Err("A console command is already running in this project, or this run was already sent.".into());
                }
                let argv = arguments(&definition(&command, &token)?, &values, !terminal)?;
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
        response["job"] = session.job.as_ref().map_or(Value::Null, Job::json);
        response["history"] = json!(session.history);
        Ok(response)
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
            let (current, _) = runner(&project)?;
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
                let catalog = self.catalog(&project)?.ok_or("No console commands are loaded. Click Load commands in the Console tab of the Symfony tool window first.")?;
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
                json!({"runner": runner(&project)?.0, "token": catalog.token, "capturedAt": catalog.captured_at, "commands": commands})
            }
            "console_execute" => {
                only(&input, &["runner", "command", "token", "requestId", "values"])?;
                if ["runner", "command", "token", "requestId", "values"].iter().any(|key| input.get(*key).is_none()) {
                    return Err(Error::Invalid("runner, command, token, requestId and values are required.".into()));
                }
                check_runner(&input)?;
                let request = json!({"op": "run", "id": input["requestId"], "command": input["command"], "token": input["token"], "values": input["values"]});
                self.handle(ctx, VIEW, &request.to_string())?
            }
            "console_job" | "console_cancel" => {
                only(&input, &["id"])?;
                let id = input["id"].as_str().filter(|id| (16..=128).contains(&id.len())).ok_or_else(|| Error::Invalid("id must be 16 to 128 characters.".into()))?;
                let op = if tool == "console_job" { "poll" } else { "cancel" };
                self.handle(ctx, VIEW, &json!({"op": op, "id": id}).to_string())?
            }
            _ => return Err(Error::NotFound(format!("tool {tool}"))),
        };
        Ok(Some(reply))
    }
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
        let ViewEvent::Message(message) = event else { return Ok(()) };
        let preview = serde_json::from_str::<Value>(&message).ok().is_some_and(|request| request["op"] == "preview");
        let reply = match self.console.handle(ctx, view, &message) {
            Ok(reply) => {
                self.after_console_run(ctx, &reply);
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

    /// After a successful captured run: reindex and open the files a maker reports as created.
    fn after_console_run(&mut self, ctx: &Invocation, reply: &Value) {
        if reply["completed"] != true {
            return;
        }
        let Some(project) = ctx.project() else { return };
        self.index_project(&project);
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
        host.on_process("php", |args, _| {
            assert_eq!(args, LIST_ARGS);
            Ok(openidea_plugin::ProcessOutput { status: Some(0), stdout: format!("Deprecated: notice\n{LIST}"), stderr: String::new() })
        });
        host
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
        let mut state = State::default();
        let empty = send(&mut state, &host, VIEW, json!({"op": "state"})).unwrap();
        assert_eq!((empty["commands"].clone(), empty["token"].clone()), (json!([]), Value::Null));
        assert!(host.process_runs().is_empty());

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
        assert_eq!(host.process_runs().len(), 1);
    }

    #[test]
    fn preview_is_exact_and_a_stale_token_never_runs() {
        let host = host_with_list();
        let mut state = State::default();
        let token = send(&mut state, &host, VIEW, json!({"op": "refresh"})).unwrap()["token"].clone();
        let preview = send(&mut state, &host, VIEW, json!({"op": "preview", "command": "app:probe", "token": token, "values": {"name": "two words"}, "revision": 4})).unwrap();
        assert_eq!(preview["argv"], json!(["bin/console", "app:probe", "--no-interaction", "--no-ansi", "--", "two words"]));
        assert_eq!(preview["revision"], 4);
        assert!(send(&mut state, &host, VIEW, json!({"op": "preview", "command": "app:probe", "token": "old", "values": {}, "revision": 5})).is_err());
        assert!(send(&mut state, &host, GENERATORS_VIEW, json!({"op": "preview", "command": "app:probe", "token": token, "values": {"name": "x"}, "revision": 6})).is_err());
        let run = send(&mut state, &host, VIEW, json!({"op": "run", "id": "execution-request-0001", "command": "app:probe", "token": "old", "values": {"name": "x"}}));
        assert!(run.is_err());
        assert_eq!(host.process_runs().len(), 1);
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
        assert!(toolbar.is_empty());
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
        let started: Value = serde_json::from_str(&plugin.call("console_execute", &execute("agent-execution-0002", json!({"name": "x"})), &ctx).unwrap()).unwrap();
        assert_eq!(started["job"]["id"], "agent-execution-0002");
        assert!(plugin.call("console_execute", r#"{"runner":"php"}"#, &ctx).is_err());
        let polled: Value = serde_json::from_str(&plugin.call("console_job", r#"{"id":"agent-execution-0002"}"#, &ctx).unwrap()).unwrap();
        assert_eq!(polled["job"]["running"], true);
        let cancelled: Value = serde_json::from_str(&plugin.call("console_cancel", r#"{"id":"agent-execution-0002"}"#, &ctx).unwrap()).unwrap();
        assert_eq!(cancelled["job"]["cancelled"], true);
        assert!(plugin.call("console_job", r#"{"id":"short"}"#, &ctx).is_err());
    }
}
