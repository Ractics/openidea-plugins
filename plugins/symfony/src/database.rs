//! Tool window > Database: tables from the Doctrine entity mappings, compared with the live database,
//! read-only masked rows and the schema diagram (the page is Laravel's database workspace). Browsing
//! reads source only; Compare and Rows boot the app in a PHP process through the console runner.
//! Also the database commands: Run Migrations, Reset Database…, Load Fixtures….
use crate::db_schema::{self, Live, Naming, Schema};
use openidea_plugin::{
    Error, Invocation, NotificationLevel, Project, host,
    views::{Body, Button, View},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

pub(crate) const VIEW: &str = "symfony-database";
const MARKER: &str = "@@symfony-database@@";
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const BUDGET: Duration = Duration::from_secs(60);
const MAX_FILES: usize = 2000;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CELLS: usize = 128 * 100;
const MIGRATIONS_BUNDLE: &str = "doctrine/doctrine-migrations-bundle";
const FIXTURES_BUNDLE: &str = "doctrine/doctrine-fixtures-bundle";
const MIGRATE: &str = "doctrine:migrations:migrate";

#[derive(Default)]
pub(crate) struct State {
    projects: BTreeMap<String, Session>,
}

#[derive(Default)]
struct Session {
    compared: Option<Value>,
    /// Entity and migration files of the last answer: the only files Open accepts.
    paths: BTreeSet<String>,
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
fn static_schema(project: &Project, root: &str) -> Result<(Schema, Vec<(String, String)>), Error> {
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

fn probe_args(mode: &str, options: &Value) -> Vec<String> {
    vec!["-r".into(), include_str!("database_probe.php").trim_start_matches("<?php").into(), json!({"mode": mode, "options": options}).to_string()]
}

/// The helper's reply after its marker; its `error` becomes an error.
pub(crate) fn parse_reply(raw: &str) -> Result<Value, Error> {
    let Some(at) = raw.rfind(MARKER) else {
        return Err(Error::Failed("The app ended the PHP process without an answer. Run bin/console about in a terminal to see why it does not boot.".into()));
    };
    let line = raw[at + MARKER.len()..].lines().next().unwrap_or_default();
    let value: Value = serde_json::from_str(line).map_err(|_| Error::Failed("The database helper returned an unreadable answer.".into()))?;
    if let Some(message) = value.get("error").and_then(Value::as_str) {
        return Err(Error::Failed(message.chars().take(400).collect()));
    }
    Ok(value)
}

/// Runs the helper in `mode` through the console runner (local `php` or the Docker target),
/// cancellable, stopped after 60 seconds or 4 MiB of output; the raw output.
pub(crate) fn run_probe(project: &Project, mode: &str, options: &Value) -> Result<String, Error> {
    let task = host::begin_task("Reading the Symfony database", true);
    let child = crate::console_runner::start(project, &probe_args(mode, options))?;
    crate::console_runner::collect(&child, || task.is_cancelled(), BUDGET, MAX_OUTPUT, "Reading the database")
}

pub(crate) fn probe(project: &Project, mode: &str, options: &Value) -> Result<Value, Error> {
    parse_reply(&run_probe(project, mode, options)?)
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

    /// Symfony: Refresh Database Metadata: compares again; the open tab shows it.
    pub(crate) fn compare(&mut self, ctx: &Invocation) -> Result<Value, Error> {
        self.handle(ctx, r#"{"op":"compare"}"#)
    }

    /// Drops the last comparison after the database changed.
    pub(crate) fn invalidate(&mut self, root: &str) {
        if let Some(session) = self.projects.get_mut(root) {
            session.compared = None;
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
        let session = self.projects.entry(root.clone()).or_default();
        match request {
            Request::State => {
                let (schema, migrations) = static_schema(&project, &root)?;
                session.paths = schema.tables.iter().filter_map(|table| table.open.as_ref().map(|place| place.path.clone())).chain(migrations.into_iter().map(|(_, path)| path)).collect();
                let mut diagram = db_schema::diagram(&root, &schema);
                diagram["unreadable"] = json!(schema.unreadable);
                diagram["live"] = session.compared.clone().unwrap_or(Value::Null);
                diagram["root"] = json!(root);
                Ok(diagram)
            }
            Request::Compare => {
                let (schema, migrations) = static_schema(&project, &root)?;
                let live = live_from(run(&project, "compare", &json!({}))?)?;
                let rows: Vec<Value> = db_schema::compare(&schema, &migrations, &live).iter().map(|row| row.json(&root)).collect();
                let tables: Vec<&str> = live.tables.iter().map(|table| table.name.as_str()).collect();
                let compared = json!({"connection": live.connection, "driver": live.driver, "truncated": live.truncated, "tables": tables, "rows": rows});
                session.paths.extend(migrations.into_iter().map(|(_, path)| path));
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

/// The tab with its toolbar; Run Migrations and Load Fixtures… only when their bundle is installed.
pub(crate) fn view(project: &Project) -> View {
    let mut view = View::new(Body::Html(html())).button(Button::new("database-refresh", "Refresh Database Metadata").icon("refresh"));
    if has_package(project, MIGRATIONS_BUNDLE) {
        view = view.button(Button::new("database-migrate", "Run Migrations").icon("run"));
    }
    view = view
        .button(Button::new("database-add-source", "Add Database to IDE…").icon("db-add"))
        .button(Button::new("database-sources", "Database Connections…").icon("database"))
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

/// The database a command would change, from `.env` files: `(name, "app on 127.0.0.1:3306")`,
/// credentials left out; `None` when DATABASE_URL is not set or not a URL.
fn database_target(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    if scheme.starts_with("sqlite") {
        let path = rest.trim_start_matches('/').replace("%kernel.project_dir%/", "");
        let name = path.rsplit('/').next().unwrap_or_default().to_owned();
        return (!name.is_empty()).then(|| (name, format!("{path} (SQLite)")));
    }
    let (authority, database) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let database = database.trim_matches('/');
    if host.is_empty() || database.is_empty() || host.contains(['$', '%']) || database.contains(['$', '%']) {
        return None;
    }
    Some((database.to_owned(), format!("{database} on {host}")))
}

#[derive(Clone, Copy, PartialEq)]
enum Change {
    Reset,
    Fixtures,
}

/// The confirmation every database-changing command and button shares: the project, the database and
/// host from DATABASE_URL, APP_ENV and the effect. In prod, or when the database is unknown, a typed
/// word is needed as well. `Ok(false)` when cancelled.
fn confirm_change(project: &Project, change: Change, migrations: bool) -> Result<bool, Error> {
    let root = project.root()?;
    let environment = dotenv(project, &[".env.local".into(), ".env".into()], "APP_ENV").map_or_else(|| "dev".to_owned(), |(value, _)| value);
    let files = [format!(".env.{environment}.local"), ".env.local".into(), format!(".env.{environment}"), ".env".into()];
    let target = dotenv(project, &files, "DATABASE_URL").and_then(|(url, file)| database_target(&url).map(|(name, place)| (name, place, file)));
    let (verb, word, effect) = match change {
        Change::Reset if migrations => ("Reset database", "reset", "Drops every table, then runs all migrations. This cannot be undone."),
        Change::Reset => ("Reset database", "reset", "Drops every table, then creates the schema from the entity mappings. This cannot be undone."),
        Change::Fixtures => ("Load fixtures into database", "load", "Purges the database, then loads the fixtures. This cannot be undone."),
    };
    let (title, database) = match &target {
        Some((name, place, file)) => (format!("{verb} {name}?"), format!("Database: {place} (DATABASE_URL in {file}). Environment: {environment}.")),
        None => (format!("{verb} of this project?"), format!("Couldn't read DATABASE_URL from the .env files, so the database is unknown. Environment: {environment}.")),
    };
    if !host::confirm(&title, &format!("Project: {root}\n{database}\n{effect}"))? {
        return Ok(false);
    }
    if environment != "prod" && target.is_some() {
        return Ok(true);
    }
    let why = if environment == "prod" { "This is the prod environment" } else { "The database is unknown" };
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

impl State {
    /// Symfony: Run Migrations, Reset Database… and Load Fixtures…; `id` is the command id.
    pub(crate) fn change(&mut self, ctx: &Invocation, id: &str) -> Result<(), Error> {
        let (project, root) = project_of(ctx)?;
        let migrations = has_package(&project, MIGRATIONS_BUNDLE);
        let done = match id {
            "database-migrate" => {
                needs_bundle(&project, MIGRATIONS_BUNDLE, "DoctrineMigrationsBundle")?;
                let _task = host::begin_task("Running Doctrine migrations", false);
                console(&project, &[MIGRATE])?;
                "Ran the migrations. Compare with database in the Database tab to check the schema."
            }
            "database-reset" => {
                if !confirm_change(&project, Change::Reset, migrations)? {
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
                if !confirm_change(&project, Change::Fixtures, migrations)? {
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
    fn reply_parsing_reads_the_marker_and_reports_errors() {
        assert_eq!(parse_reply("noise\n@@symfony-database@@{\"tables\":[]}\n").unwrap(), json!({"tables": []}));
        assert!(matches!(parse_reply("@@symfony-database@@{\"error\":\"Reading the database failed (PDOException: refused).\"}"), Err(Error::Failed(m)) if m.contains("refused")));
        assert!(parse_reply("Fatal error").unwrap_err().to_string().contains("without an answer"));
        assert!(parse_reply("@@symfony-database@@{oops").is_err());
    }

    #[test]
    fn the_probe_runs_through_the_console_runner_locally_and_in_docker() {
        use crate::docker_target::tests::{choose_first, raw};
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:php", "process:docker"]);
        let project = host.invocation().project().unwrap();
        let local = run_probe(&project, "compare", &json!({})).unwrap();
        assert!(local.starts_with("started: php\n"), "{local}");
        assert!(local.contains("@@symfony-database@@") && local.contains(r#"\"mode\":\"compare\""#), "the script and its input go over stdin");
        choose_first(&host, raw());
        host.on_process("docker", |args, _| {
            assert_eq!(args[2], "inspect");
            Ok(ProcessOutput { status: Some(0), stdout: raw().to_string(), stderr: String::new() })
        });
        assert!(run_probe(&project, "rows", &json!({})).unwrap().starts_with("started: docker\n"));
        assert!(host.tasks().iter().any(|task| task.title == "Reading the Symfony database"));
        let denied = MockHost::install().with_capabilities(&["project.read"]);
        assert!(run_probe(&denied.invocation().project().unwrap(), "compare", &json!({})).unwrap_err().to_string().contains("process:php"));
    }

    #[test]
    fn database_target_strips_credentials() {
        assert_eq!(database_target("mysql://app:s3cret@127.0.0.1:3306/shop?serverVersion=8"), Some(("shop".into(), "shop on 127.0.0.1:3306".into())));
        assert_eq!(database_target("postgresql://app:!ChangeMe!@database:5432/app?charset=utf8"), Some(("app".into(), "app on database:5432".into())));
        assert_eq!(database_target("sqlite:///%kernel.project_dir%/var/data.db"), Some(("data.db".into(), "var/data.db (SQLite)".into())));
        assert_eq!(database_target("${DB_URL}"), None);
        assert_eq!(scrub("could not reach mysql://root:pw@db:3306/x"), "could not reach mysql://•••@db:3306/x");
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

    #[test]
    fn a_cancelled_confirm_runs_nothing() {
        let host = setup();
        with_env(&host, "dev");
        let log = console_log(&host);
        let mut state = State::default();
        state.change(&host.invocation(), "database-reset").unwrap();
        state.change(&host.invocation(), "database-fixtures").unwrap();
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn reset_drops_then_migrates_after_the_confirm() {
        let host = setup();
        with_env(&host, "dev");
        let log = console_log(&host);
        host.answer_next_prompt(Some(&["ok"]));
        State::default().change(&host.invocation(), "database-reset").unwrap();
        assert_eq!(
            *log.borrow(),
            ["bin/console doctrine:schema:drop --force --full-database --no-interaction", "bin/console doctrine:migrations:migrate --no-interaction"]
        );
        host.answer_next_prompt(Some(&["ok"]));
        State::default().change(&host.invocation(), "database-fixtures").unwrap();
        assert_eq!(log.borrow().last().unwrap(), "bin/console doctrine:fixtures:load --no-interaction");
        assert!(!host.notifications().iter().any(|n| n.body.contains("hunter2")));
    }

    #[test]
    fn prod_needs_the_typed_word() {
        let host = setup();
        with_env(&host, "prod");
        let log = console_log(&host);
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(Some(&["yes"]));
        assert!(State::default().change(&host.invocation(), "database-reset").unwrap_err().to_string().contains("Nothing was changed"));
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(None);
        State::default().change(&host.invocation(), "database-reset").unwrap();
        assert!(log.borrow().is_empty());
        host.answer_next_prompt(Some(&["ok"]));
        host.answer_next_prompt(Some(&["reset"]));
        State::default().change(&host.invocation(), "database-reset").unwrap();
        assert_eq!(log.borrow().len(), 2);
    }

    #[test]
    fn migrate_needs_the_bundle_and_goes_through_docker_with_a_target() {
        use crate::docker_target::tests::{choose_first, raw};
        let host = setup();
        let log = console_log(&host);
        let error = State::default().change(&host.invocation(), "database-migrate").unwrap_err().to_string();
        assert!(error.contains("composer require doctrine/doctrine-migrations-bundle"), "{error}");
        with_env(&host, "dev");
        State::default().change(&host.invocation(), "database-migrate").unwrap();
        assert_eq!(*log.borrow(), ["bin/console doctrine:migrations:migrate --no-interaction"]);

        choose_first(&host, raw());
        let docker = Rc::new(RefCell::new(Vec::new()));
        let seen = docker.clone();
        host.on_process("docker", move |args, _| {
            seen.borrow_mut().push(args.join(" "));
            let stdout = if args[2] == "inspect" { raw().to_string() } else { String::new() };
            Ok(ProcessOutput { status: Some(0), stdout, stderr: String::new() })
        });
        State::default().change(&host.invocation(), "database-migrate").unwrap();
        assert!(docker.borrow().last().unwrap().ends_with("php bin/console doctrine:migrations:migrate --no-interaction"), "{:?}", docker.borrow());
    }

    #[test]
    fn a_failed_command_names_it_without_credentials() {
        let host = setup();
        with_env(&host, "dev");
        host.on_process("php", |_, _| Ok(ProcessOutput { status: Some(1), stdout: String::new(), stderr: "In Driver: connect to mysql://app:hunter2@db/shop refused".into() }));
        let error = State::default().change(&host.invocation(), "database-migrate").unwrap_err().to_string();
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
        assert!(html().contains("data-dbw-text='{") && !html().contains("Laravel"));
    }

    /// Runs the real helper with `php` against stub classes; `None` when php is missing.
    fn run_php(mode: &str, options: Value) -> Option<String> {
        let dir = std::env::temp_dir().join(format!("symfony-database-{}-{mode}", std::process::id()));
        std::fs::create_dir_all(dir.join("vendor")).ok()?;
        std::fs::write(dir.join("vendor/autoload.php"), STUBS).ok()?;
        let output = std::process::Command::new("php").args(probe_args(mode, &options)).current_dir(&dir).env("APP_ENV", "test").output().ok()?;
        let _ = std::fs::remove_dir_all(&dir);
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    const STUBS: &str = r#"<?php
namespace {
    class Column { function __construct(public $n) {} function getName() { return $this->n; } }
    class Idx { function __construct(public $c, public $p) {} function getColumns() { return $this->c; } function isPrimary() { return $this->p; } }
    class Platform { function quoteSingleIdentifier($n) { return '"'.$n.'"'; } function quoteIdentifier($n) { return '"'.$n.'"'; } function modifyLimitQuery($q, $l, $o) { return $q.' LIMIT '.$l.' OFFSET '.$o; } }
    class SchemaManager {
        function listTableNames() { return ['user', 'doctrine_migration_versions']; }
        function listTableColumns($t) { return $t === 'user' ? [new Column('id'), new Column('email'), new Column('api_key'), new Column('name')] : [new Column('version')]; }
        function listTableIndexes($t) { return [new Idx(['id'], true)]; }
    }
    class Conn {
        public static $sql = [];
        function createSchemaManager() { return new SchemaManager; }
        function getDatabasePlatform() { return new Platform; }
        function getParams() { return ['driver' => 'pdo_mysql', 'host' => 'db', 'port' => 3306, 'dbname' => 'shop', 'user' => 'app', 'password' => 'hunter2', 'url' => 'mysql://app:hunter2@db/shop', 'driverOptions' => [1 => 2]]; }
        function fetchFirstColumn($q) { self::$sql[] = $q; return ['DoctrineMigrations\Version1']; }
        function fetchOne($q) { self::$sql[] = $q; return 2; }
        function fetchAllNumeric($q) { return [[1, 'ada@example.com', 'k', 'Ada'], [2, null, null, str_repeat('x', 300)]]; }
    }
    class Registry {
        function getConnection($name = null) { return new Conn; }
        function getConnectionNames() { return ['default' => 'doctrine.dbal.default_connection']; }
        function getDefaultConnectionName() { return 'default'; }
    }
    class Container { function has($id) { return $id === 'doctrine'; } function get($id) { return new Registry; } }
}
namespace App { class Kernel { function __construct($e, $d) {} function boot() {} function getContainer() { return new \Container; } } }
"#;

    #[test]
    fn php_probe_compares_masks_rows_and_never_exports_the_password() {
        let Some(output) = run_php("compare", json!({})) else {
            eprintln!("php is not on PATH; skipping the database helper check");
            return;
        };
        let live = live_from(parse_reply(&output).unwrap()).unwrap();
        assert_eq!(live.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["user", "doctrine_migration_versions"]);
        assert_eq!(live.executed, Some(vec!["DoctrineMigrations\\Version1".to_owned()]));
        assert_eq!(live.driver, "pdo_mysql");

        let rows = rows_reply(&parse_reply(&run_php("rows", json!({"table": "user", "page": 0, "size": 10})).unwrap()).unwrap()).unwrap();
        assert_eq!(rows["masked"], json!(["email", "api_key"]));
        assert_eq!(rows["rows"][0], json!([1, "•••", "•••", "Ada"]));
        assert_eq!(rows["rows"][1][1], Value::Null, "null stays null");
        assert!(rows["rows"][1][3].as_str().unwrap().ends_with('…'));
        assert!(parse_reply(&run_php("rows", json!({"table": "secrets; drop"})).unwrap()).is_err());

        let sources = run_php("sources", json!({})).unwrap();
        assert!(!sources.contains("hunter2") && !sources.contains("driverOptions"), "{sources}");
        let connection = &parse_reply(&sources).unwrap()["connections"][0];
        assert_eq!(connection, &json!({"name":"default","default":true,"driver":"pdo_mysql","host":"db","port":"3306","dbname":"shop","user":"app"}));
    }
}
