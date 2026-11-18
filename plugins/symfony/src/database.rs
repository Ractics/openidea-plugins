//! Tool window > Database: tables from the Doctrine entity mappings, compared with the live database,
//! read-only masked rows and the schema diagram (the page is Laravel's database workspace). Browsing
//! reads source only; Compare and Rows boot the app in a PHP process through the console runner.
//! Also the database commands: Run Migrations…, Reset Database…, Load Fixtures….
use crate::db_schema::{self, Live, Naming, Schema};
use openidea_plugin::{
    Error, Invocation, NotificationLevel, Project, host,
    views::{Body, Button, View},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

pub(crate) const VIEW: &str = "symfony-database";
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const BUDGET: Duration = Duration::from_secs(60);
const MAX_FILES: usize = 2000;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CELLS: usize = 128 * 100;
const MIGRATIONS_BUNDLE: &str = "doctrine/doctrine-migrations-bundle";
const FIXTURES_BUNDLE: &str = "doctrine/doctrine-fixtures-bundle";
const MIGRATE: &str = "doctrine:migrations:migrate";

type Migrations = Vec<(String, String)>;

#[derive(Default)]
pub(crate) struct State {
    projects: BTreeMap<String, Session>,
    /// Whether file events arrive, so a read schema can be kept until a PHP or Doctrine file changes.
    cache_schemas: bool,
}

#[derive(Default)]
struct Session {
    compared: Option<Value>,
    /// Entity and migration files of the last answer: the only files Open accepts.
    paths: BTreeSet<String>,
    schema: Option<(Schema, Migrations)>,
}

/// A fresh random token per helper run: the reply line must carry it, so nothing the app prints
/// (a response body, a log line) can pass for the reply.
pub(crate) fn nonce() -> String {
    let half = || std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", half(), half())
}

/// The text after `@@symfony-<kind>-<nonce>@@` on the last line that starts with it.
pub(crate) fn reply_line<'a>(raw: &'a str, kind: &str, nonce: &str) -> Option<&'a str> {
    let marker = format!("@@symfony-{kind}-{nonce}@@");
    raw.lines().rev().find_map(|line| line.strip_prefix(marker.as_str()))
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    State,
    Compare,
    Rows {
        table: String,
        #[serde(default)]
        page: u32,
        #[serde(default = "default_size")]
        size: u32,
    },
    Open {
        path: String,
        line: u32,
    },
}

fn default_size() -> u32 {
    25
}

fn project_of(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Symfony project first.")?;
    let root = project.root()?;
    Ok((project, root))
}

fn owned(path: &str, root: &str) -> bool {
    host::project_of(path).and_then(|project| project.root().ok()).as_deref() == Some(root)
}

/// The project's files matching `globs`, at most [`MAX_FILES`].
fn files(project: &Project, root: &str, globs: &[String]) -> Result<Vec<String>, Error> {
    let mut paths = BTreeSet::new();
    for glob in globs {
        paths.extend(project.find_files(glob, MAX_FILES as u32 + 1)?.into_iter().filter(|path| owned(path, root)));
    }
    if paths.len() > MAX_FILES {
        return Err(Error::Invalid(format!("More than {MAX_FILES} entity or migration files; the schema cannot be drawn completely.")));
    }
    Ok(paths.into_iter().collect())
}

/// The entity-mapping schema and the migration classes, from source.
fn static_schema(project: &Project, root: &str) -> Result<(Schema, Migrations), Error> {
    let _task = host::begin_task("Reading Doctrine mappings", false);
    let doctrine = project.read_file("config/packages/doctrine.yaml").ok();
    let mut sources = Vec::new();
    let mut bytes = 0;
    for path in files(project, root, &crate::index::entity_globs(doctrine.as_deref().unwrap_or_default()))? {
        let text = project.read_file(&path).ok();
        bytes += text.as_ref().map_or(0, String::len);
        if bytes > MAX_SOURCE_BYTES {
            return Err(Error::Invalid("Entity sources exceed 32 MiB.".into()));
        }
        if text.as_ref().is_none_or(|text| text.contains("Entity")) {
            sources.push((path, text));
        }
    }
    let schema = db_schema::build(&sources, Naming::from_config(doctrine.as_deref()));
    let migrations = db_schema::migration_classes(&files(project, root, &["migrations/**/Version*.php".to_owned()])?);
    Ok((schema, migrations))
}

/// The read schema of `slot`, read from source when there is none yet or caching is off.
fn cached<'a>(slot: &'a mut Option<(Schema, Migrations)>, cache: bool, project: &Project, root: &str) -> Result<&'a (Schema, Migrations), Error> {
    if slot.is_none() || !cache {
        *slot = Some(static_schema(project, root)?);
    }
    slot.as_ref().ok_or(Error::Stale)
}

fn probe_args(mode: &str, options: &Value, nonce: &str) -> Vec<String> {
    vec!["-r".into(), include_str!("database_probe.php").trim_start_matches("<?php").into(), json!({"mode": mode, "options": options, "nonce": nonce}).to_string()]
}

/// The helper's reply on its marker line; its `error` becomes an error.
pub(crate) fn parse_reply(raw: &str, nonce: &str) -> Result<Value, Error> {
    let Some(line) = reply_line(raw, "database", nonce) else {
        return Err(Error::Failed("The app ended the PHP process without an answer. Run bin/console about in a terminal to see why it does not boot.".into()));
    };
    let value: Value = serde_json::from_str(line).map_err(|_| Error::Failed("The database helper returned an unreadable answer.".into()))?;
    if let Some(message) = value.get("error").and_then(Value::as_str) {
        return Err(Error::Failed(message.chars().take(400).collect()));
    }
    Ok(value)
}

/// Runs the helper with `args` through the console runner (local `php` or the Docker target),
/// cancellable, stopped after 60 seconds or 4 MiB of output; the raw output.
// ponytail: blocks the plugin instance while it runs (up to 60 s); the shared database page awaits
// one reply per message, so polling would need a protocol change in database-workspace.js.
fn run_probe(project: &Project, args: &[String]) -> Result<String, Error> {
    let task = host::begin_task("Reading the Symfony database", true);
    let child = crate::console_runner::start(project, args)?;
    crate::console_runner::collect(&child, || task.is_cancelled(), BUDGET, MAX_OUTPUT, "Reading the database")
}

pub(crate) fn probe(project: &Project, mode: &str, options: &Value) -> Result<Value, Error> {
    let nonce = nonce();
    parse_reply(&run_probe(project, &probe_args(mode, options, &nonce))?, &nonce)
}

fn live_from(value: Value) -> Result<Live, Error> {
    let live: Live = serde_json::from_value(value).map_err(|error| Error::Failed(format!("The database helper returned an unexpected answer ({error}).")))?;
    let bounded = live.tables.len() <= db_schema::MAX_LIVE_TABLES
        && live.executed.as_ref().is_none_or(|executed| executed.len() <= 5000)
        && live.tables.iter().all(|table| table.columns.len() <= 512 && table.indexes.len() <= 64);
    if !bounded {
        return Err(Error::Failed("The database answer is larger than expected (over 512 tables, 512 columns or 64 indexes in a table).".into()));
    }
    Ok(live)
}

fn rows_reply(value: &Value) -> Result<Value, Error> {
    let unexpected = || Error::Failed("The database helper returned unexpected rows.".into());
    let columns = value["columns"].as_array().filter(|columns| columns.len() <= 128 && columns.iter().all(Value::is_string));
    let rows = value["rows"].as_array().filter(|rows| rows.len() <= 100);
    let (Some(columns), Some(rows)) = (columns, rows) else { return Err(unexpected()) };
    let shaped = rows
        .iter()
        .all(|row| row.as_array().is_some_and(|cells| cells.len() == columns.len() && cells.iter().all(|cell| !cell.is_array() && !cell.is_object())));
    if !shaped || rows.len() * columns.len() > MAX_CELLS {
        return Err(unexpected());
    }
    let masked: Vec<&Value> = value["masked"].as_array().into_iter().flatten().filter(|name| name.is_string()).collect();
    Ok(json!({
        "table": value["table"].as_str().unwrap_or_default(),
        "columns": columns,
        "rows": rows,
        "total": value["total"].as_u64().unwrap_or(0),
        "page": value["page"].as_u64().unwrap_or(0),
        "size": value["size"].as_u64().unwrap_or(25),
        "masked": masked,
    }))
}

type Probe<'a> = &'a mut dyn FnMut(&Project, &str, &Value) -> Result<Value, Error>;

impl State {
    /// A page message (`state`, `compare`, `rows`, `open`) and its answer.
    pub(crate) fn handle(&mut self, ctx: &Invocation, message: &str) -> Result<Value, Error> {
        self.handle_with(ctx, message, &mut probe)
    }

    /// Symfony: Compare with Database: compares again; the open tab shows it.
    pub(crate) fn compare(&mut self, ctx: &Invocation) -> Result<Value, Error> {
        self.handle(ctx, r#"{"op":"compare"}"#)
    }

    /// Drops the last comparison after the database changed.
    pub(crate) fn invalidate(&mut self, root: &str) {
        if let Some(session) = self.projects.get_mut(root) {
            session.compared = None;
        }
    }

    /// Keeps read schemas between refreshes; only when file events arrive to drop them.
    pub(crate) fn cache_schemas(&mut self, on: bool) {
        self.cache_schemas = on;
    }

    /// Drops every read schema (too many file changes to list).
    pub(crate) fn forget_schemas(&mut self) {
        self.projects.values_mut().for_each(|session| session.schema = None);
    }

    /// Drops the read schema of each project with a changed PHP file or Doctrine config.
    pub(crate) fn files_changed<'a>(&mut self, paths: impl IntoIterator<Item = &'a str>) {
        for path in paths.into_iter().filter(|path| path.ends_with(".php") || path.ends_with("doctrine.yaml") || path.ends_with("doctrine.yml")) {
            for (root, session) in &mut self.projects {
                if crate::under(path, root) {
                    session.schema = None;
                }
            }
        }
    }

    fn handle_with(&mut self, ctx: &Invocation, message: &str, run: Probe) -> Result<Value, Error> {
        if message.len() > 4096 {
            return Err(Error::Invalid("The request is too large.".into()));
        }
        let request: Request = serde_json::from_str(message).map_err(|_| Error::Invalid("The Database tab sent a request this version does not know. Reload the Symfony plugin.".into()))?;
        let (project, root) = project_of(ctx)?;
        if !self.projects.contains_key(&root) && self.projects.len() >= 8 {
            self.projects.clear();
        }
        let cache = self.cache_schemas;
        let session = self.projects.entry(root.clone()).or_default();
        match request {
            Request::State => {
                let (schema, migrations) = cached(&mut session.schema, cache, &project, &root)?;
                session.paths = schema.tables.iter().filter_map(|table| table.open.as_ref().map(|place| place.path.clone())).chain(migrations.iter().map(|(_, path)| path.clone())).collect();
                let mut diagram = db_schema::diagram(&root, schema);
                diagram["unreadable"] = json!(schema.unreadable);
                diagram["live"] = session.compared.clone().unwrap_or(Value::Null);
                diagram["root"] = json!(root);
                Ok(diagram)
            }
            Request::Compare => {
                let (schema, migrations) = cached(&mut session.schema, cache, &project, &root)?;
                let live = live_from(run(&project, "compare", &json!({}))?)?;
                let rows: Vec<Value> = db_schema::compare(schema, migrations, &live).iter().map(|row| row.json(&root)).collect();
                let tables: Vec<&str> = live.tables.iter().map(|table| table.name.as_str()).collect();
                let compared = json!({"connection": live.connection, "driver": live.driver, "truncated": live.truncated, "tables": tables, "rows": rows});
                session.paths.extend(migrations.iter().map(|(_, path)| path.clone()));
                session.compared = Some(compared.clone());
                Ok(json!({"live": compared}))
            }
            Request::Rows { table, page, size } => {
                if table.is_empty() || table.len() > 512 || table.chars().any(char::is_control) {
                    return Err(Error::Invalid("Choose a table.".into()));
                }
                let options = json!({"table": table, "page": page.min(100_000), "size": size.clamp(10, 100)});
                Ok(json!({"rows": rows_reply(&run(&project, "rows", &options)?)?}))
            }
            Request::Open { path, line } => {
                if !session.paths.contains(&path) || !owned(&path, &root) {
                    return Err(Error::Invalid("This file is no longer part of the schema. Refresh the Database tab.".into()));
                }
                host::open_location(&path, line.saturating_sub(1), 0)?;
                Ok(json!({"opened": true}))
            }
        }
    }
}

/// Wording of the shared page that differs from Laravel's.
fn texts() -> Value {
    json!({
        "region": "Symfony database",
        "noTables": "No entity mapping defines a table yet. Add #[ORM\\Entity] classes under src/Entity, then refresh.",
        "unreadable": "{count} entity files could not be read and are left out (unreadable text, or annotation, XML or YAML mappings): {files}",
        "openMigration": "Open the mapping",
        "goToMigration": "Go to Mapping",
        "statusOnlyMigrations": "Only in the mappings",
        "inSync": "The database matches the entity mappings and every migration has run.",
        "filterPlaceholder": "Filter by table, entity or column",
        "legendDashed": "Dashed: entity association",
    })
}

fn attribute_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('\'', "&#39;").replace('<', "&lt;")
}

pub(crate) fn html() -> String {
    let panels = r#"<div class="ui-tabs dbw-tabs" role="tablist" aria-label="Database sections" data-i18n-aria-label="database.sections"><button type="button" role="tab" class="ui-tab" data-dbw-tab="tables" aria-selected="true" data-i18n="database.tabTables">Tables</button><button type="button" role="tab" class="ui-tab" data-dbw-tab="compare" aria-selected="false" data-i18n="database.compareTab">Compare with database</button><button type="button" role="tab" class="ui-tab" data-dbw-tab="schema" aria-selected="false" data-i18n="database.tabSchema">Schema</button><button type="button" role="tab" class="ui-tab" data-dbw-tab="rows" aria-selected="false" data-i18n="database.tabRows">Rows</button></div><div data-dbw-panel="tables"><div class="dbw-toolbar ui-toolbar"><button type="button" id="dbw-table-refresh" class="ui-button"><span data-ui-icon="refresh" aria-hidden="true"></span><span data-i18n="database.refresh">Refresh</span></button><button type="button" id="dbw-expand-all" class="ui-button ui-icon-button" aria-label="Expand all" title="Expand all" data-i18n-aria-label="database.expandAllButton" data-i18n-title="database.expandAllButton"><span data-ui-icon="expand-all" aria-hidden="true"></span></button><button type="button" id="dbw-collapse-all" class="ui-button ui-icon-button" aria-label="Collapse all" title="Collapse all" data-i18n-aria-label="database.collapseAllButton" data-i18n-title="database.collapseAllButton"><span data-ui-icon="collapse-all" aria-hidden="true"></span></button><span class="dbw-grow"></span><span id="dbw-table-count" class="dbw-muted"></span><label class="dbw-search"><span class="ui-visually-hidden" data-i18n="database.filterTables">Filter tables</span><input id="dbw-table-filter" class="ui-input" type="search" placeholder="Filter by table, entity or column"></label></div><div id="dbw-tables" class="dbw-scroll"></div><p id="dbw-unreadable" class="dbw-muted" role="status" hidden></p></div><div data-dbw-panel="compare" hidden><div class="dbw-toolbar ui-toolbar"><button type="button" id="dbw-compare" class="ui-button"><span data-ui-icon="run" aria-hidden="true"></span><span data-i18n="database.compareTab">Compare with database</span></button><label class="dbw-inline"><span data-i18n="database.show">Show</span><select id="dbw-compare-status" class="ui-input"><option value="" data-i18n="database.allDifferences">All differences</option><option value="pending" data-i18n="database.pendingMigrations">Pending migrations</option><option value="only-migrations">Only in the mappings</option><option value="only-database" data-i18n="database.statusOnlyDatabase">Only in the database</option><option value="differs" data-i18n="database.missingColumns">Missing columns or indexes</option></select></label><span class="dbw-grow"></span><span id="dbw-compare-summary" class="dbw-muted"></span></div><p class="dbw-muted">Boots the app kernel and runs read-only queries on the default Doctrine connection. Nothing is changed.</p><div id="dbw-compare-result" class="dbw-scroll"></div></div><div data-dbw-panel="schema" hidden><div class="dbw-toolbar ui-toolbar" role="toolbar" aria-label="Diagram" data-i18n-aria-label="database.diagramToolbar"><button type="button" id="dbw-zoom-out" class="ui-button ui-icon-button" aria-label="Zoom out" title="Zoom out" data-i18n-aria-label="database.zoomOut" data-i18n-title="database.zoomOut"><span data-ui-icon="remove" aria-hidden="true"></span></button><button type="button" id="dbw-zoom-in" class="ui-button ui-icon-button" aria-label="Zoom in" title="Zoom in" data-i18n-aria-label="database.zoomIn" data-i18n-title="database.zoomIn"><span data-ui-icon="add" aria-hidden="true"></span></button><button type="button" id="dbw-fit" class="ui-button"><span data-ui-icon="expand-all" aria-hidden="true"></span><span data-i18n="database.fit">Fit</span></button><button type="button" id="dbw-refresh" class="ui-button"><span data-ui-icon="refresh" aria-hidden="true"></span><span data-i18n="database.refresh">Refresh</span></button><span class="dbw-grow"></span><span id="dbw-count" class="dbw-muted"></span><label class="dbw-search"><span class="ui-visually-hidden">Filter by table, entity or column</span><input id="dbw-diagram-filter" class="ui-input" type="search" placeholder="Filter by table, entity or column"></label></div><div id="dbw-diagram" class="dbw-diagram" tabindex="0" aria-label="Schema diagram. Drag to pan, scroll to zoom." data-i18n-aria-label="database.diagramHelp"></div><p class="dbw-legend dbw-muted"><span data-i18n="database.legendPrimary">PK primary</span><span data-i18n="database.legendForeign">FK foreign key</span><span data-i18n="database.legendUnique">UK unique</span><span data-i18n="database.legendSolid">Solid: foreign key</span><span>Dashed: entity association</span></p></div><div data-dbw-panel="rows" hidden><div class="dbw-toolbar ui-toolbar"><label class="dbw-inline"><span data-i18n="database.table">Table</span><span class="dbw-combo"><input id="dbw-rows-table" class="ui-input" role="combobox" aria-expanded="false" aria-controls="dbw-rows-tables" aria-autocomplete="list" autocomplete="off" spellcheck="false"><ul id="dbw-rows-tables" class="dbw-suggest" role="listbox" hidden></ul></span></label><label class="dbw-inline"><span data-i18n="database.rowsPerPage">Rows per page</span><select id="dbw-rows-size" class="ui-input"><option>10</option><option selected>25</option><option>50</option><option>100</option></select></label><button type="button" id="dbw-rows-load" class="ui-button"><span data-ui-icon="run" aria-hidden="true"></span><span data-i18n="database.showRows">Show rows</span></button><button type="button" id="dbw-rows-prev" class="ui-button ui-icon-button" aria-label="Previous page" title="Previous page" data-i18n-aria-label="database.previousPage" data-i18n-title="database.previousPage" disabled><span data-ui-icon="chevron-left" aria-hidden="true"></span></button><button type="button" id="dbw-rows-next" class="ui-button ui-icon-button" aria-label="Next page" title="Next page" data-i18n-aria-label="database.nextPage" data-i18n-title="database.nextPage" disabled><span data-ui-icon="chevron-right" aria-hidden="true"></span></button><span class="dbw-grow"></span><span id="dbw-rows-summary" class="dbw-muted"></span></div><p class="dbw-muted" data-i18n="database.rowsNote">Read only. Columns named like secrets or personal data (password, token, email, phone, address, ...) are masked.</p><div id="dbw-rows-result" class="dbw-scroll"></div></div>"#;
    format!(
        "<style>{}</style><section data-database-workspace=\"database\" aria-label=\"Symfony database\" data-dbw-text='{}'>{panels}<p id=\"dbw-status\" class=\"dbw-muted\" role=\"status\"></p><p id=\"dbw-error\" class=\"dbw-error\" role=\"alert\"></p></section>",
        include_str!("../web/resources/database-workspace.css"),
        attribute_text(&texts().to_string())
    )
}

/// Whether composer.json or symfony.lock names `package`.
fn has_package(project: &Project, package: &str) -> bool {
    let quoted = format!("\"{package}\"");
    ["composer.json", "symfony.lock"].iter().any(|file| project.read_file(file).is_ok_and(|text| text.contains(&quoted)))
}

/// The tab with its toolbar; Run Migrations… and Load Fixtures… only when their bundle is installed.
pub(crate) fn view(project: &Project) -> View {
    let mut view = View::new(Body::Html(html())).button(Button::new("database-refresh", "Compare with Database").icon("refresh"));
    if has_package(project, MIGRATIONS_BUNDLE) {
        view = view.button(Button::new("database-migrate", "Run Migrations…").icon("run"));
    }
    view = view
        .button(Button::new("database-add-source", "Add Database to IDE…").icon("db-add"))
        .button(Button::new("database-sources", "Manage Database Connections…").icon("database"))
        .button(Button::new("database-reset", "Reset Database…").icon("trash"));
    if has_package(project, FIXTURES_BUNDLE) {
        view = view.button(Button::new("database-fixtures", "Load Fixtures…").icon("import"));
    }
    view
}

/// `KEY=value` from the first of `files` that sets it, unquoted.
fn dotenv(project: &Project, files: &[String], key: &str) -> Option<(String, String)> {
    files.iter().find_map(|file| {
        let text = project.read_file(file).ok()?;
        let value = text.lines().rev().find_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            line.strip_prefix(key)?.strip_prefix('=')
        })?;
        let value = value.trim().trim_matches(['"', '\'']).to_owned();
        (!value.is_empty()).then(|| (value, file.clone()))
    })
}

/// One database a command would change, credentials left out: its name, its host without the port
/// (empty for SQLite) and how to show it ("shop on 127.0.0.1:3306").
#[derive(Clone, Debug, PartialEq)]
struct Database {
    name: String,
    host: String,
    place: String,
}

fn sqlite(path: &str) -> Option<Database> {
    let name = path.rsplit('/').next().unwrap_or_default().to_owned();
    (!name.is_empty()).then(|| Database { name, host: String::new(), place: format!("{path} (SQLite)") })
}

fn without_port(authority: &str) -> &str {
    authority.rsplit_once(':').filter(|(_, port)| !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())).map_or(authority, |(host, _)| host)
}

/// The database of DATABASE_URL; `None` when it is not a URL or uses variables.
fn database_target(url: &str) -> Option<Database> {
    let (scheme, rest) = url.split_once("://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    if scheme.starts_with("sqlite") {
        return sqlite(&rest.trim_start_matches('/').replace("%kernel.project_dir%/", ""));
    }
    let (authority, database) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let database = database.trim_matches('/');
    if host.is_empty() || database.is_empty() || host.contains(['$', '%']) || database.contains(['$', '%']) {
        return None;
    }
    Some(Database { name: database.to_owned(), host: without_port(host).to_owned(), place: format!("{database} on {host}") })
}

/// The default connection of the helper's `sources` answer, as the running app sees it.
fn runtime_database(answer: &Value) -> Option<Database> {
    let connections = answer["connections"].as_array()?;
    let connection = connections.iter().find(|connection| connection["default"] == true).or(connections.first())?;
    let text = |key: &str| connection[key].as_str().filter(|text| !text.is_empty() && text.len() <= 512 && !text.chars().any(char::is_control));
    if connection.get("error").is_some() {
        return None;
    }
    if let Some(path) = text("path") {
        return sqlite(path);
    }
    let (name, host) = (text("dbname")?, text("host")?);
    let port = text("port").map(|port| format!(":{port}")).unwrap_or_default();
    Some(Database { name: name.to_owned(), host: host.to_owned(), place: format!("{name} on {host}{port}") })
}

#[derive(Clone, Copy, PartialEq)]
enum Change {
    Migrate,
    Reset,
    Fixtures,
}

/// What the confirmation shows and why a typed word is needed as well (`None`: it isn't).
#[derive(Debug, PartialEq)]
struct Prompt {
    title: String,
    body: String,
    word: &'static str,
    why_type: Option<&'static str>,
}

/// What `.env` files and the running app (the helper's `sources` answer) say about the target.
struct Facts<'a> {
    root: &'a str,
    dotenv_environment: &'a str,
    dotenv_database: Option<(Database, String)>,
    runtime: &'a Result<Value, Error>,
    docker: Option<String>,
}

/// The confirmation text: the project, the database and APP_ENV as the running app sees them
/// (environment variables win over `.env` files), the Docker container and the effect. A typed word is
/// needed in prod, when the app's values differ from the `.env` files, or when they can't be read.
fn change_prompt(change: Change, migrations: bool, facts: &Facts) -> Prompt {
    let (word, effect) = match change {
        Change::Migrate => ("migrate", "Applies every pending migration. Undo needs doctrine:migrations:migrate prev."),
        Change::Reset if migrations => ("reset", "Drops every table, then runs all migrations. This cannot be undone."),
        Change::Reset => ("reset", "Drops every table, then creates the schema from the entity mappings. This cannot be undone."),
        Change::Fixtures => ("load", "Purges the database, then loads the fixtures. This cannot be undone."),
    };
    let runtime = facts.runtime.as_ref().ok().and_then(|answer| answer["environment"].as_str().filter(|environment| !environment.is_empty()).map(|environment| (environment, runtime_database(answer))));
    let dotenv = facts.dotenv_database.as_ref();
    let mut lines = vec![format!("Project: {}", facts.root)];
    lines.extend(facts.docker.as_ref().map(|label| format!("Runs in Docker container {label}.")));
    let (database, why_type) = match &runtime {
        Some((environment, database)) => {
            let same_database = database.as_ref().map(|database| (&database.name, &database.host)) == dotenv.map(|(database, _)| (&database.name, &database.host));
            let differs = *environment != facts.dotenv_environment || !same_database;
            match database {
                Some(database) => lines.push(format!("Database: {} (default connection, as the app sees it). Environment: {environment}.", database.place)),
                None => lines.push(format!("Couldn't read the default connection's database from the app. Environment: {environment}.")),
            }
            if differs {
                let file = dotenv.map_or_else(|| "no readable DATABASE_URL".to_owned(), |(database, file)| format!("{} in {file}", database.place));
                lines.push(format!("The .env files say {file}, APP_ENV {}: environment variables override them.", facts.dotenv_environment));
            }
            let why = if *environment == "prod" {
                Some("This is the prod environment")
            } else if database.is_none() {
                Some("The database is unknown")
            } else if differs {
                Some("The app's settings differ from the .env files")
            } else {
                None
            };
            (database.clone(), why)
        }
        None => {
            let reason = match facts.runtime {
                Err(error) => error.to_string().chars().take(200).collect::<String>(),
                Ok(_) => "no environment in its answer".to_owned(),
            };
            match dotenv {
                Some((database, file)) => lines.push(format!("Database: {} (DATABASE_URL in {file}). Environment: {}.", database.place, facts.dotenv_environment)),
                None => lines.push(format!("Couldn't read DATABASE_URL from the .env files. Environment: {}.", facts.dotenv_environment)),
            }
            lines.push(format!("Couldn't read the app's own settings ({reason}), so environment variables may point elsewhere."));
            let why = if facts.dotenv_environment == "prod" { "This is the prod environment" } else { "The app's settings couldn't be read" };
            (dotenv.map(|(database, _)| database.clone()), Some(why))
        }
    };
    lines.push(effect.to_owned());
    let title = match (change, &database) {
        (Change::Migrate, Some(database)) => format!("Run migrations on {}?", database.name),
        (Change::Migrate, None) => "Run migrations on this project's database?".to_owned(),
        (Change::Reset, Some(database)) => format!("Reset database {}?", database.name),
        (Change::Reset, None) => "Reset the database of this project?".to_owned(),
        (Change::Fixtures, Some(database)) => format!("Load fixtures into database {}?", database.name),
        (Change::Fixtures, None) => "Load fixtures into the database of this project?".to_owned(),
    };
    Prompt { title, body: lines.join("\n"), word, why_type }
}

/// The confirmation every database-changing command and button shares (see [`change_prompt`]);
/// `probe` reads the running app's settings. `Ok(false)` when cancelled.
fn confirm_change(project: &Project, change: Change, migrations: bool, probe: SourcesProbe) -> Result<bool, Error> {
    let root = project.root()?;
    let environment = dotenv(project, &[".env.local".into(), ".env".into()], "APP_ENV").map_or_else(|| "dev".to_owned(), |(value, _)| value);
    let files = [format!(".env.{environment}.local"), ".env.local".into(), format!(".env.{environment}"), ".env".into()];
    let dotenv_database = dotenv(project, &files, "DATABASE_URL").and_then(|(url, file)| database_target(&url).map(|database| (database, file)));
    let runtime = probe(project);
    if matches!(runtime, Err(Error::Cancelled)) {
        return Ok(false);
    }
    let docker = crate::docker_target::current(project).ok().flatten().map(|target| target.label());
    let facts = Facts { root: &root, dotenv_environment: &environment, dotenv_database, runtime: &runtime, docker };
    let Prompt { title, body, word, why_type } = change_prompt(change, migrations, &facts);
    if !host::confirm(&title, &body)? {
        return Ok(false);
    }
    let Some(why) = why_type else {
        return Ok(true);
    };
    let Some(typed) = host::input(&format!("{why}. Type {word} to continue."), None)? else {
        return Ok(false);
    };
    if typed.trim() != word {
        return Err(Error::Invalid(format!("Nothing was changed: you typed {} instead of {word}. Run the command again to retry.", typed.trim())));
    }
    Ok(true)
}

/// `//user:password@` in tool output becomes `//•••@`.
fn scrub(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        out.push_str(&rest[..at + 3]);
        rest = &rest[at + 3..];
        let end = rest.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(rest.len());
        if let Some(credentials) = rest[..end].rfind('@') {
            out.push_str("•••");
            rest = &rest[credentials..];
        }
    }
    out.push_str(rest);
    out
}

/// Runs `bin/console <args…> --no-interaction` through the console runner; a failure names the
/// command and its last output lines.
fn console(project: &Project, args: &[&str]) -> Result<(), Error> {
    let mut argv = vec!["bin/console".to_owned()];
    argv.extend(args.iter().map(|arg| (*arg).to_owned()));
    argv.push("--no-interaction".into());
    let output = crate::console_runner::run(project, &argv)?;
    if output.status == Some(0) {
        return Ok(());
    }
    let error = if output.stderr.trim().is_empty() { &output.stdout } else { &output.stderr };
    let tail: Vec<&str> = error.trim().lines().rev().take(5).collect();
    let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
    Err(Error::Failed(format!("bin/console {} failed:\n{}", args.join(" "), scrub(&tail))))
}

fn needs_bundle(project: &Project, package: &str, bundle: &str) -> Result<(), Error> {
    if has_package(project, package) {
        return Ok(());
    }
    Err(Error::Invalid(format!("{bundle} is not installed in this project. Run composer require {package}, then try again.")))
}

type SourcesProbe<'a> = &'a mut dyn FnMut(&Project) -> Result<Value, Error>;

impl State {
    /// Symfony: Run Migrations…, Reset Database… and Load Fixtures…; `id` is the command id.
    pub(crate) fn change(&mut self, ctx: &Invocation, id: &str) -> Result<(), Error> {
        self.change_with(ctx, id, &mut |project| probe(project, "sources", &json!({})))
    }

    fn change_with(&mut self, ctx: &Invocation, id: &str, probe: SourcesProbe) -> Result<(), Error> {
        let (project, root) = project_of(ctx)?;
        let migrations = has_package(&project, MIGRATIONS_BUNDLE);
        let done = match id {
            "database-migrate" => {
                needs_bundle(&project, MIGRATIONS_BUNDLE, "DoctrineMigrationsBundle")?;
                if !confirm_change(&project, Change::Migrate, migrations, probe)? {
                    return Ok(());
                }
                let _task = host::begin_task("Running Doctrine migrations", false);
                console(&project, &[MIGRATE])?;
                "Ran the migrations. Compare with database in the Database tab to check the schema."
            }
            "database-reset" => {
                if !confirm_change(&project, Change::Reset, migrations, probe)? {
                    return Ok(());
                }
                let _task = host::begin_task("Resetting the database", false);
                console(&project, &["doctrine:schema:drop", "--force", "--full-database"])?;
                self.invalidate(&root);
                console(&project, &[if migrations { MIGRATE } else { "doctrine:schema:create" }])?;
                "Reset the database. Load fixtures or compare with database next."
            }
            "database-fixtures" => {
                needs_bundle(&project, FIXTURES_BUNDLE, "DoctrineFixturesBundle")?;
                if !confirm_change(&project, Change::Fixtures, migrations, probe)? {
                    return Ok(());
                }
                let _task = host::begin_task("Loading fixtures", false);
                console(&project, &["doctrine:fixtures:load"])?;
                "Loaded the fixtures. Open Rows in the Database tab to see them."
            }
            _ => return Err(Error::NotFound(format!("command {id}"))),
        };
        self.invalidate(&root);
        host::notify(NotificationLevel::Info, "Symfony", done);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{ProcessOutput, mock::MockHost};
    use std::{cell::RefCell, rc::Rc};

    fn setup() -> MockHost {
        let host = MockHost::install().with_capabilities(&["project.read", "process:php", "process:docker", "storage"]);
        host.add_file("bin/console", "<?php");
        for (path, text) in crate::db_schema::tests::files() {
            if let Some(text) = text {
                host.add_file(&path.replace("/p/", ""), &text);
            }
        }
        host.add_file("migrations/Version1.php", "<?php\nnamespace DoctrineMigrations;\nfinal class Version1 {}\n");
        host
    }

    fn send(state: &mut State, host: &MockHost, message: Value, reply: Value, calls: &mut Vec<(String, Value)>) -> Result<Value, Error> {
        state.handle_with(&host.invocation(), &message.to_string(), &mut |_, mode, options| {
            calls.push((mode.into(), options.clone()));
            Ok(reply.clone())
        })
    }

    #[test]
    fn state_draws_the_mapping_schema_without_running_php() {
        let host = setup();
        let mut state = State::default();
        let mut calls = vec![];
        let reply = send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert!(calls.is_empty());
        let names: Vec<&str> = reply["tables"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["blog_post_tag", "blog_post", "comment", "tag", "user"]);
        assert_eq!(reply["unreadable"], json!(["/project/src/Entity/Legacy.php"]));
        assert_eq!(reply["live"], Value::Null);
        assert!(host.tasks().iter().any(|task| task.title == "Reading Doctrine mappings"));
    }

    #[test]
    fn compare_runs_the_probe_once_and_is_kept_for_the_next_state() {
        let host = setup();
        let mut state = State::default();
        let mut calls = vec![];
        let live = json!({"connection":"default","driver":"pdo_sqlite","tables":[{"name":"user","columns":["id","profile_id"],"indexes":[{"columns":["profile_id"]}]}],"executed":[]});
        let reply = send(&mut state, &host, json!({"op":"compare"}), live, &mut calls).unwrap();
        assert_eq!(calls, [("compare".to_owned(), json!({}))]);
        let statuses: Vec<&str> = reply["live"]["rows"].as_array().unwrap().iter().map(|row| row["status"].as_str().unwrap()).collect();
        assert_eq!(statuses, ["pending", "only-migrations", "only-migrations", "only-migrations", "only-migrations"]);
        let again = send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert_eq!(again["live"]["driver"], "pdo_sqlite");
        assert_eq!(calls.len(), 1);
        send(&mut state, &host, json!({"op":"open","path":"/project/migrations/Version1.php","line":1}), Value::Null, &mut calls).unwrap();
        state.invalidate("/project");
        assert_eq!(send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap()["live"], Value::Null);
    }

    #[test]
    fn oversized_live_answers_are_refused() {
        let host = setup();
        let mut state = State::default();
        let mut calls = vec![];
        let tables: Vec<Value> = (0..600).map(|n| json!({"name": format!("t{n}")})).collect();
        assert!(send(&mut state, &host, json!({"op":"compare"}), json!({"tables": tables}), &mut calls).is_err());
        let wide = json!({"tables":[{"name":"t","columns": vec!["c"; 600]}]});
        assert!(send(&mut state, &host, json!({"op":"compare"}), wide, &mut calls).is_err());
    }

    #[test]
    fn rows_are_bounded_validated_and_shape_checked() {
        let host = setup();
        let mut state = State::default();
        let mut calls = vec![];
        let page = json!({"table":"user","columns":["id","email"],"rows":[[1,"•••"]],"total":41,"page":1,"size":10,"masked":["email"]});
        let reply = send(&mut state, &host, json!({"op":"rows","table":"user","page":1,"size":5000}), page, &mut calls).unwrap();
        assert_eq!(calls[0].1, json!({"table":"user","page":1,"size":100}));
        assert_eq!(reply["rows"]["masked"], json!(["email"]));
        assert!(send(&mut state, &host, json!({"op":"rows","table":"user"}), json!({"columns":["id","email"],"rows":[[1]]}), &mut calls).is_err());
        assert!(send(&mut state, &host, json!({"op":"rows","table":"user"}), json!({"columns":["id"],"rows":[[{"a":1}]]}), &mut calls).is_err());
        assert!(send(&mut state, &host, json!({"op":"rows","table":""}), Value::Null, &mut calls).is_err());
        assert!(send(&mut state, &host, json!({"op":"rows","table":"user","sql":"drop"}), Value::Null, &mut calls).is_err());
        assert_eq!(calls.len(), 3);
    }

    #[test]
    fn open_only_accepts_files_of_the_last_state() {
        let host = setup();
        let mut state = State::default();
        let mut calls = vec![];
        let path = "/project/src/Entity/BlogPost.php";
        assert!(send(&mut state, &host, json!({"op":"open","path":path,"line":5}), Value::Null, &mut calls).is_err());
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert!(send(&mut state, &host, json!({"op":"open","path":"/project/bin/console","line":1}), Value::Null, &mut calls).is_err());
        send(&mut state, &host, json!({"op":"open","path":path,"line":5}), Value::Null, &mut calls).unwrap();
    }

    #[test]
    fn the_read_schema_is_kept_until_a_php_file_changes_when_events_arrive() {
        let host = setup();
        let mut state = State::default();
        let mut calls = vec![];
        let reads = |host: &MockHost| host.tasks().iter().filter(|task| task.title == "Reading Doctrine mappings").count();
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert_eq!(reads(&host), 2, "without file events every refresh reads the source");
        state.cache_schemas(true);
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert_eq!(reads(&host), 2, "the last read is kept");
        state.files_changed(["/project/templates/base.html.twig", "/other/src/Entity/X.php"]);
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert_eq!(reads(&host), 2);
        state.files_changed(["/project/src/Entity/BlogPost.php"]);
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert_eq!(reads(&host), 3);
        state.forget_schemas();
        send(&mut state, &host, json!({"op":"state"}), Value::Null, &mut calls).unwrap();
        assert_eq!(reads(&host), 4);
    }

    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn reply_parsing_reads_the_marker_and_reports_errors() {
        let reply = |json: &str| format!("noise\n@@symfony-database-{NONCE}@@{json}\n");
        assert_eq!(parse_reply(&reply("{\"tables\":[]}"), NONCE).unwrap(), json!({"tables": []}));
        assert!(matches!(parse_reply(&reply("{\"error\":\"Reading the database failed (PDOException: refused).\"}"), NONCE), Err(Error::Failed(m)) if m.contains("refused")));
        assert!(parse_reply("Fatal error", NONCE).unwrap_err().to_string().contains("without an answer"));
        assert!(parse_reply(&reply("{oops"), NONCE).is_err());
    }

    #[test]
    fn only_a_line_starting_with_this_runs_nonce_is_the_reply() {
        let real = format!("@@symfony-database-{NONCE}@@{{\"tables\":[]}}");
        let spoofed = format!("{real}\n@@symfony-database-{}@@{{\"tables\":[\"evil\"]}}\nlog: x@@symfony-database-{NONCE}@@{{\"tables\":[\"inline\"]}}\n", "f".repeat(32));
        assert_eq!(parse_reply(&spoofed, NONCE).unwrap(), json!({"tables": []}));
        assert!(parse_reply(&format!("@@symfony-database@@{{\"tables\":[]}}"), NONCE).is_err(), "the old marker without a nonce is not accepted");
        assert_ne!(nonce(), nonce());
        assert!(nonce().len() == 32 && nonce().bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn the_probe_runs_through_the_console_runner_locally_and_in_docker() {
        use crate::docker_target::tests::{choose_first, raw};
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:php", "process:docker"]);
        let project = host.invocation().project().unwrap();
        let local = run_probe(&project, &probe_args("compare", &json!({}), NONCE)).unwrap();
        assert!(local.starts_with("started: php\n"), "{local}");
        assert!(local.contains("@@symfony-database-") && local.contains(r#"\"mode\":\"compare\""#) && local.contains(NONCE), "the script and its input go over stdin");
        choose_first(&host, raw());
        host.on_process("docker", |args, _| {
            assert_eq!(args[2], "inspect");
            Ok(ProcessOutput { status: Some(0), stdout: raw().to_string(), stderr: String::new() })
        });
        assert!(run_probe(&project, &probe_args("rows", &json!({}), NONCE)).unwrap().starts_with("started: docker\n"));
        assert!(host.tasks().iter().any(|task| task.title == "Reading the Symfony database"));
        let denied = MockHost::install().with_capabilities(&["project.read"]);
        assert!(probe(&denied.invocation().project().unwrap(), "compare", &json!({})).unwrap_err().to_string().contains("process:php"));
    }

    #[test]
    fn database_target_strips_credentials() {
        let place = |url: &str| database_target(url).map(|database| (database.name, database.host, database.place));
        assert_eq!(place("mysql://app:s3cret@127.0.0.1:3306/shop?serverVersion=8"), Some(("shop".into(), "127.0.0.1".into(), "shop on 127.0.0.1:3306".into())));
        assert_eq!(place("postgresql://app:!ChangeMe!@database:5432/app?charset=utf8"), Some(("app".into(), "database".into(), "app on database:5432".into())));
        assert_eq!(place("sqlite:///%kernel.project_dir%/var/data.db"), Some(("data.db".into(), String::new(), "var/data.db (SQLite)".into())));
        assert_eq!(place("${DB_URL}"), None);
        assert_eq!(scrub("could not reach mysql://root:pw@db:3306/x"), "could not reach mysql://•••@db:3306/x");
        assert_eq!(scrub("could not reach mysql://u:p@ss@db/x"), "could not reach mysql://•••@db/x");
    }

    fn console_log(host: &MockHost) -> Rc<RefCell<Vec<String>>> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let seen = log.clone();
        host.on_process("php", move |args, _| {
            seen.borrow_mut().push(args.join(" "));
            Ok(ProcessOutput { status: Some(0), stdout: String::new(), stderr: String::new() })
        });
        log
    }

    fn with_env(host: &MockHost, env: &str) {
        host.add_file(".env", &format!("APP_ENV={env}\nDATABASE_URL=\"mysql://app:hunter2@127.0.0.1:3306/shop\"\n"));
        host.add_file("composer.json", r#"{"require":{"doctrine/doctrine-migrations-bundle":"^3","doctrine/doctrine-fixtures-bundle":"^3"}}"#);
    }

    /// The helper's `sources` answer for one default MySQL connection.
    fn sources(environment: &str, host: &str, dbname: &str) -> Value {
        json!({"environment": environment, "connections": [{"name": "default", "default": true, "driver": "pdo_mysql", "host": host, "port": "3306", "dbname": dbname, "user": "app"}]})
    }

    /// Runs the database command `id` with the running app answering `runtime` (`None`: it doesn't boot).
    fn change(host: &MockHost, id: &str, runtime: Option<Value>) -> Result<(), Error> {
        State::default().change_with(&host.invocation(), id, &mut |_| runtime.clone().ok_or_else(|| Error::Failed("The app did not boot (RuntimeException: x).".into())))
    }

    fn matching() -> Option<Value> {
        Some(sources("dev", "127.0.0.1", "shop"))
    }

    #[test]
    fn a_cancelled_confirm_runs_nothing() {
        let host = setup();
        with_env(&host, "dev");
        let log = console_log(&host);
        change(&host, "database-reset", matching()).unwrap();
        change(&host, "database-fixtures", matching()).unwrap();
        change(&host, "database-migrate", matching()).unwrap();
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn reset_drops_then_migrates_after_the_confirm() {
        let host = setup();
        with_env(&host, "dev");
        let log = console_log(&host);
        host.answer_next_prompt(Some(&["ok"]));
        change(&host, "database-reset", matching()).unwrap();
        assert_eq!(
            *log.borrow(),
            ["bin/console doctrine:schema:drop --force --full-database --no-interaction", "bin/console doctrine:migrations:migrate --no-interaction"]
        );
        host.answer_next_prompt(Some(&["ok"]));
        change(&host, "database-fixtures", matching()).unwrap();
        assert_eq!(log.borrow().last().unwrap(), "bin/console doctrine:fixtures:load --no-interaction");
        assert!(!host.notifications().iter().any(|n| n.body.contains("hunter2")));
    }

    #[test]
    fn prod_needs_the_typed_word() {
        let host = setup();
        with_env(&host, "prod");
        let log = console_log(&host);
        let prod = || Some(sources("prod", "127.0.0.1", "shop"));
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(Some(&["yes"]));
        assert!(change(&host, "database-reset", prod()).unwrap_err().to_string().contains("Nothing was changed"));
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(None);
        change(&host, "database-reset", prod()).unwrap();
        assert!(log.borrow().is_empty());
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(Some(&["reset"]));
        change(&host, "database-reset", prod()).unwrap();
        assert_eq!(log.borrow().len(), 2);
    }

    #[test]
    fn the_running_apps_values_decide_the_typed_word() {
        let host = setup();
        with_env(&host, "dev");
        let log = console_log(&host);
        let refused = |runtime: Option<Value>| {
            host.answer_next_prompt(Some(&["ok"]));
            host.answer_next_prompt(Some(&["yes"]));
            change(&host, "database-fixtures", runtime).unwrap_err().to_string()
        };
        assert!(refused(Some(sources("prod", "127.0.0.1", "shop"))).contains("instead of load"), "prod from a real environment variable");
        assert!(refused(Some(sources("dev", "db", "shop"))).contains("instead of load"), "another database host than .env");
        assert!(refused(Some(sources("dev", "127.0.0.1", "shop_test"))).contains("instead of load"), "another database than .env");
        assert!(refused(None).contains("instead of load"), "the app's values can't be read");
        assert!(log.borrow().is_empty());
        host.answer_next_prompt(Some(&["ok"]));
        change(&host, "database-fixtures", matching()).unwrap();
        assert_eq!(log.borrow().len(), 1, "matching values need the confirm only");
    }

    fn facts<'a>(runtime: &'a Result<Value, Error>, docker: Option<&str>) -> Facts<'a> {
        let dotenv = database_target("mysql://app:hunter2@127.0.0.1:3306/shop").map(|database| (database, ".env".to_owned()));
        Facts { root: "/project", dotenv_environment: "dev", dotenv_database: dotenv, runtime, docker: docker.map(str::to_owned) }
    }

    #[test]
    fn the_confirm_shows_the_runtime_database_the_container_and_the_env_files_when_they_differ() {
        let runtime = Ok(sources("prod", "db", "app"));
        let prompt = change_prompt(Change::Reset, true, &facts(&runtime, Some("shop-php-1 (abc…, /app)")));
        assert_eq!(prompt.title, "Reset database app?");
        assert_eq!(
            prompt.body,
            "Project: /project\nRuns in Docker container shop-php-1 (abc…, /app).\nDatabase: app on db:3306 (default connection, as the app sees it). Environment: prod.\nThe .env files say shop on 127.0.0.1:3306 in .env, APP_ENV dev: environment variables override them.\nDrops every table, then runs all migrations. This cannot be undone."
        );
        assert_eq!(prompt.why_type, Some("This is the prod environment"));

        let same = Ok(sources("dev", "127.0.0.1", "shop"));
        let prompt = change_prompt(Change::Fixtures, true, &facts(&same, None));
        assert_eq!(prompt.body.lines().count(), 3, "{}", prompt.body);
        assert_eq!(prompt.why_type, None);

        let failed = Err(Error::Failed("The app did not boot (PDOException: refused).".into()));
        let prompt = change_prompt(Change::Fixtures, true, &facts(&failed, None));
        assert_eq!(prompt.title, "Load fixtures into database shop?");
        assert!(prompt.body.contains("DATABASE_URL in .env") && prompt.body.contains("Couldn't read the app's own settings (The app did not boot"), "{}", prompt.body);
        assert_eq!(prompt.why_type, Some("The app's settings couldn't be read"));
        assert!(!prompt.body.contains("hunter2"));
    }

    #[test]
    fn migrations_ask_first_and_name_the_undo() {
        let runtime = Ok(sources("dev", "127.0.0.1", "shop"));
        let prompt = change_prompt(Change::Migrate, true, &facts(&runtime, None));
        assert_eq!((prompt.title.as_str(), prompt.word), ("Run migrations on shop?", "migrate"));
        assert!(prompt.body.ends_with("Applies every pending migration. Undo needs doctrine:migrations:migrate prev."), "{}", prompt.body);

        let host = setup();
        with_env(&host, "dev");
        let log = console_log(&host);
        change(&host, "database-migrate", matching()).unwrap();
        assert!(log.borrow().is_empty(), "a cancelled confirm runs nothing");
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(Some(&["reset"]));
        assert!(change(&host, "database-migrate", Some(sources("dev", "db", "shop"))).unwrap_err().to_string().contains("instead of migrate"));
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(Some(&["migrate"]));
        change(&host, "database-migrate", Some(sources("dev", "db", "shop"))).unwrap();
        assert_eq!(*log.borrow(), ["bin/console doctrine:migrations:migrate --no-interaction"]);
    }

    #[test]
    fn migrate_needs_the_bundle_and_goes_through_docker_with_a_target() {
        use crate::docker_target::tests::{choose_first, raw};
        let host = setup();
        let log = console_log(&host);
        let error = change(&host, "database-migrate", matching()).unwrap_err().to_string();
        assert!(error.contains("composer require doctrine/doctrine-migrations-bundle"), "{error}");
        with_env(&host, "dev");
        host.answer_next_prompt(Some(&["ok"]));
        change(&host, "database-migrate", matching()).unwrap();
        assert_eq!(*log.borrow(), ["bin/console doctrine:migrations:migrate --no-interaction"]);

        choose_first(&host, raw());
        let docker = Rc::new(RefCell::new(Vec::new()));
        let seen = docker.clone();
        host.on_process("docker", move |args, _| {
            seen.borrow_mut().push(args.join(" "));
            let stdout = if args[2] == "inspect" { raw().to_string() } else { String::new() };
            Ok(ProcessOutput { status: Some(0), stdout, stderr: String::new() })
        });
        host.answer_next_prompt(Some(&["ok"]));
        change(&host, "database-migrate", matching()).unwrap();
        assert!(docker.borrow().last().unwrap().ends_with("php bin/console doctrine:migrations:migrate --no-interaction"), "{:?}", docker.borrow());
    }

    #[test]
    fn a_failed_command_names_it_without_credentials() {
        let host = setup();
        with_env(&host, "dev");
        host.on_process("php", |_, _| Ok(ProcessOutput { status: Some(1), stdout: String::new(), stderr: "In Driver: connect to mysql://app:hunter2@db/shop refused".into() }));
        host.answer_next_prompt(Some(&["ok"]));
        let error = change(&host, "database-migrate", matching()).unwrap_err().to_string();
        assert!(error.starts_with("bin/console doctrine:migrations:migrate failed:") && !error.contains("hunter2"), "{error}");
    }

    #[test]
    fn the_toolbar_follows_the_installed_bundles() {
        let host = setup();
        let project = host.invocation().project().unwrap();
        let ids = |view: View| view.toolbar.iter().map(|button| button.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(view(&project)), ["database-refresh", "database-add-source", "database-sources", "database-reset"]);
        with_env(&host, "dev");
        assert_eq!(ids(view(&project)).len(), 6);
        assert!(view(&project).toolbar.iter().any(|button| button.label == "Run Migrations…"));
        assert!(html().contains("data-dbw-text='{") && !html().contains("Laravel"));
    }

    /// Runs the real helper with `php` against stub classes; `None` when php is missing.
    fn run_php(mode: &str, options: Value) -> Option<String> {
        static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("symfony-database-{}-{mode}-{run}", std::process::id()));
        std::fs::create_dir_all(dir.join("vendor")).ok()?;
        std::fs::write(dir.join("vendor/autoload.php"), STUBS).ok()?;
        let output = std::process::Command::new("php").args(probe_args(mode, &options, NONCE)).current_dir(&dir).env("APP_ENV", "test").output().ok()?;
        let _ = std::fs::remove_dir_all(&dir);
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    const STUBS: &str = r#"<?php
namespace {
    class Column { function __construct(public $n) {} function getName() { return $this->n; } }
    class Idx { function __construct(public $c, public $p) {} function getColumns() { return $this->c; } function isPrimary() { return $this->p; } }
    class Platform { function quoteSingleIdentifier($n) { return '"'.$n.'"'; } function quoteIdentifier($n) { return '"'.$n.'"'; } function modifyLimitQuery($q, $l, $o) { return $q.' LIMIT '.$l.' OFFSET '.$o; } }
    class SchemaManager {
        const MFA = ['id', 'backupCodes', 'recovery_codes', 'mfa_code', 'pin', 'totp', 'jwt', 'session_id', 'pinned', 'category'];
        function listTableNames() { return ['user', 'doctrine_migration_versions', 'mfa']; }
        function listTableColumns($t) { return array_map(fn ($n) => new Column($n), match ($t) { 'user' => ['id', 'email', 'api_key', 'name'], 'mfa' => self::MFA, default => ['version'] }); }
        function listTableIndexes($t) { return [new Idx(['id'], true)]; }
    }
    class Conn {
        public static $sql = [];
        function createSchemaManager() { return new SchemaManager; }
        function getDatabasePlatform() { return new Platform; }
        function getParams() { return ['driver' => 'pdo_mysql', 'host' => 'db', 'port' => 3306, 'dbname' => 'shop', 'user' => 'app', 'password' => 'hunter2', 'url' => 'mysql://app:hunter2@db/shop', 'driverOptions' => [1 => 2]]; }
        function fetchFirstColumn($q) { self::$sql[] = $q; return ['DoctrineMigrations\Version1']; }
        function fetchOne($q) { self::$sql[] = $q; return 2; }
        function fetchAllNumeric($q) { return str_contains($q, '"mfa"') ? [array_fill(0, count(SchemaManager::MFA), 'x')] : [[1, 'ada@example.com', 'k', 'Ada'], [2, null, null, str_repeat('x', 300)]]; }
    }
    class Registry {
        function getConnection($name = null) { if ($name === 'audit') { throw new RuntimeException('SQLSTATE[HY000]: connect to mysql://u:p@ss@audit-db/log refused'); } return new Conn; }
        function getConnectionNames() { return ['default' => 'doctrine.dbal.default_connection', 'audit' => 'doctrine.dbal.audit_connection']; }
        function getDefaultConnectionName() { return 'default'; }
    }
    class Container { function has($id) { return $id === 'doctrine'; } function get($id) { return new Registry; } }
}
namespace App { class Kernel { function __construct(public $e, $d) {} function boot() {} function getContainer() { return new \Container; } function getEnvironment() { return $this->e; } } }
"#;

    #[test]
    fn php_probe_compares_masks_rows_and_never_exports_the_password() {
        let Some(output) = run_php("compare", json!({})) else {
            eprintln!("php is not on PATH; skipping the database helper check");
            return;
        };
        let live = live_from(parse_reply(&output, NONCE).unwrap()).unwrap();
        assert_eq!(live.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["user", "doctrine_migration_versions", "mfa"]);
        assert_eq!(live.executed, Some(vec!["DoctrineMigrations\\Version1".to_owned()]));
        assert_eq!(live.driver, "pdo_mysql");

        let rows = rows_reply(&parse_reply(&run_php("rows", json!({"table": "user", "page": 0, "size": 10})).unwrap(), NONCE).unwrap()).unwrap();
        assert_eq!(rows["masked"], json!(["email", "api_key"]));
        assert_eq!(rows["rows"][0], json!([1, "•••", "•••", "Ada"]));
        assert_eq!(rows["rows"][1][1], Value::Null, "null stays null");
        assert!(rows["rows"][1][3].as_str().unwrap().ends_with('…'));
        assert!(parse_reply(&run_php("rows", json!({"table": "secrets; drop"})).unwrap(), NONCE).is_err());

        let sources = run_php("sources", json!({})).unwrap();
        assert!(!sources.contains("hunter2") && !sources.contains("driverOptions"), "{sources}");
        let answer = parse_reply(&sources, NONCE).unwrap();
        assert_eq!(answer["environment"], "test", "the kernel's environment, from the real APP_ENV variable");
        assert_eq!(&answer["connections"][0], &json!({"name":"default","default":true,"driver":"pdo_mysql","host":"db","port":"3306","dbname":"shop","user":"app"}));
        assert_eq!(runtime_database(&answer).map(|database| database.place), Some("shop on db:3306".to_owned()));
    }

    #[test]
    fn php_probe_masks_mfa_and_session_columns_and_scrubs_passwords_with_an_at_sign() {
        let Some(output) = run_php("rows", json!({"table": "mfa"})) else { return };
        let rows = rows_reply(&parse_reply(&output, NONCE).unwrap()).unwrap();
        assert_eq!(rows["masked"], json!(["backupCodes", "recovery_codes", "mfa_code", "pin", "totp", "jwt", "session_id"]));

        let sources = run_php("sources", json!({})).unwrap();
        let audit = &parse_reply(&sources, NONCE).unwrap()["connections"][1];
        let error = audit["error"].as_str().unwrap();
        assert!(error.contains("mysql://•••@audit-db/log") && !sources.contains("p@ss") && !sources.contains("u:p"), "{error}");
    }
}
