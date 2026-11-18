//! Tool window > Request: runs one route through `App\Kernel` inside a PHP process (local or in the
//! Docker target), without a server. Only an explicit Send runs project PHP. With rollback on, the
//! request is not sent unless a transaction really started on every Doctrine connection (checked in
//! PHP). A write method always asks first.
use openidea_plugin::{
    Error, Project, host,
    views::{Body, Button, Field, FieldValue, View},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use ui_kit::routes::Route;

pub(crate) const VIEW: &str = "symfony-request";
/// The Request tab's button that always asks which route to open.
pub(crate) const CHOOSE_ROUTE: &str = "request-choose-route";
pub(crate) const CANCEL: &str = "request-cancel";
/// The background poll message the page sends while a request runs.
pub(crate) const TICK: &str = "request-tick";
const POLL_MS: u32 = 300;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_BODY: usize = 16 * 1024;
const MAX_QUERY: usize = 4096;
const MAX_USER: usize = 254;
const MAX_DRAFTS: usize = 8;
const SEND_BUDGET: Duration = Duration::from_secs(60);
const WRITE_METHODS: [&str; 4] = ["POST", "PUT", "PATCH", "DELETE"];
const NO_BODY_METHODS: [&str; 2] = ["GET", "HEAD"];
const ANY_METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Request {
    method: String,
    uri: String,
    query: String,
    body: Option<Value>,
    user: Option<String>,
    json: bool,
    rollback: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Response {
    status: u64,
    reason: String,
    headers: Vec<(String, String)>,
    body: String,
    size: u64,
    ms: f64,
    redirect: Option<String>,
    rolled_back: bool,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    NotSent(String),
    Error(String),
    Response(Box<Response>),
}

/// One `{name}` placeholder: `{name?default}`, `{name<requirement>}` and `{!name}` included.
#[derive(Clone, Debug, PartialEq)]
struct Placeholder {
    /// Byte range of `{…}` in the path.
    span: std::ops::Range<usize>,
    name: String,
    requirement: Option<String>,
    /// `Some` when optional (`?`), with its inline default (may be empty).
    default: Option<String>,
}

/// A Send in progress: its PHP process and what the reply must carry. Dropping it stops the process.
struct Sending {
    run: crate::console_runner::Running,
    nonce: String,
    rollback: bool,
}

struct Draft {
    label: String,
    path: String,
    methods: Vec<String>,
    method: String,
    params: Vec<(String, String)>,
    query: String,
    body: String,
    user: String,
    json: bool,
    rollback: bool,
    outcome: Option<Outcome>,
    sending: Option<Sending>,
}

#[derive(Default)]
pub(crate) struct State {
    drafts: BTreeMap<String, Draft>,
}

fn is_write(method: &str) -> bool {
    WRITE_METHODS.contains(&method)
}

fn placeholders(path: &str) -> Vec<Placeholder> {
    let mut found = Vec::new();
    let bytes = path.as_bytes();
    let mut at = 0;
    while let Some(open) = path[at..].find('{').map(|i| at + i) {
        let mut i = open + 1;
        if bytes.get(i) == Some(&b'!') {
            i += 1;
        }
        let name_start = i;
        while bytes.get(i).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
            i += 1;
        }
        let name = &path[name_start..i];
        let mut requirement = None;
        if bytes.get(i) == Some(&b'<') {
            // A requirement may hold `{n}` quantifiers: it ends at the `>` before `?` or `}`.
            let Some(end) = (i + 1..path.len()).find(|&j| bytes[j] == b'>' && matches!(bytes.get(j + 1), Some(b'?' | b'}'))) else { break };
            requirement = Some(path[i + 1..end].to_owned());
            i = end + 1;
        }
        let mut default = None;
        if bytes.get(i) == Some(&b'?') {
            let Some(end) = path[i..].find('}').map(|j| i + j) else { break };
            default = Some(path[i + 1..end].to_owned());
            i = end;
        }
        if name.is_empty() || bytes.get(i) != Some(&b'}') {
            at = open + 1;
            continue;
        }
        found.push(Placeholder { span: open..i + 1, name: name.to_owned(), requirement, default });
        at = i + 1;
    }
    found
}

fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(byte).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// The path with each placeholder replaced by its encoded value. An empty optional placeholder is
/// left out with the `/` before it, so Symfony uses its default.
fn fill_path(path: &str, params: &[(String, String)]) -> Result<String, String> {
    let mut uri = String::new();
    let mut from = 0;
    for placeholder in placeholders(path) {
        uri.push_str(&path[from..placeholder.span.start]);
        from = placeholder.span.end;
        let value = params.iter().find(|(name, _)| *name == placeholder.name).map_or("", |(_, value)| value.trim());
        if !value.is_empty() {
            uri.push_str(&encode_segment(value));
            continue;
        }
        if placeholder.default.is_none() {
            return Err(format!("Fill in the {} parameter.", placeholder.name));
        }
        if uri.ends_with('/') {
            uri.pop();
        }
    }
    uri.push_str(&path[from..]);
    Ok(if uri.starts_with('/') { uri } else { format!("/{uri}") })
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn form_pairs(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => map.iter().for_each(|(key, item)| form_pairs(&if prefix.is_empty() { key.clone() } else { format!("{prefix}[{key}]") }, item, out)),
        Value::Array(items) => items.iter().enumerate().for_each(|(at, item)| form_pairs(&format!("{prefix}[{at}]"), item, out)),
        Value::String(text) => out.push(format!("{prefix}={text}")),
        Value::Null => out.push(format!("{prefix}=")),
        other => out.push(format!("{prefix}={other}")),
    }
}

impl Request {
    /// The same request as a curl command line against `base`.
    pub(crate) fn curl(&self, base: &str) -> String {
        let target = if self.query.is_empty() { self.uri.clone() } else { format!("{}?{}", self.uri, self.query) };
        let accept = if self.json { "application/json" } else { "text/html,application/xhtml+xml" };
        let mut parts = vec![ui_kit::routes::curl(&self.method, base, &target), "-H".into(), quote(&format!("Accept: {accept}"))];
        let Some(body) = self.body.as_ref().filter(|_| !NO_BODY_METHODS.contains(&self.method.as_str())) else {
            return parts.join(" ");
        };
        if self.json {
            parts.extend(["-H".into(), quote("Content-Type: application/json"), "--data".into(), quote(&body.to_string())]);
            return parts.join(" ");
        }
        let mut pairs = Vec::new();
        form_pairs("", body, &mut pairs);
        for pair in pairs {
            parts.extend(["--data-urlencode".into(), quote(&pair)]);
        }
        parts.join(" ")
    }

    /// The helper's arguments; its reply line must carry `nonce`.
    pub(crate) fn php_args(&self, nonce: &str) -> Vec<String> {
        let input = json!({
            "nonce": nonce,
            "method": self.method,
            "uri": self.uri,
            "query": self.query,
            "body": self.body,
            "user": self.user,
            "json": self.json,
            "rollback": self.rollback,
        });
        vec!["-r".into(), include_str!("route_request.php").trim_start_matches("<?php").into(), input.to_string()]
    }
}

const ROLLBACK_MISMATCH: &str = "Roll back was on, but its transaction was not open when the request finished (the app ended it itself), so its database changes may have been kept.";

pub(crate) fn parse_output(raw: &str, nonce: &str, rollback: bool) -> Outcome {
    let Some(line) = crate::database::reply_line(raw, "request", nonce) else {
        return Outcome::Error(
            if rollback {
                "The app ended the PHP process without a response. Roll back was on, so the database discards the unfinished transaction when its connection closes."
            } else {
                "The app ended the PHP process without a response."
            }
            .into(),
        );
    };
    let unreadable = || Outcome::Error("Send Request got no readable result from PHP. Check the PHP output in the IDE log, then try again.".into());
    let Ok(reply) = serde_json::from_str::<Value>(line) else {
        return unreadable();
    };
    let rolled_back = reply["rolledBack"] == json!(true);
    if reply["notSent"] == json!(true) {
        return Outcome::NotSent(reply["error"].as_str().unwrap_or("Nothing was sent.").chars().take(600).collect());
    }
    if let Some(error) = reply["error"].as_str() {
        let note = if rollback && !rolled_back { format!(" {ROLLBACK_MISMATCH}") } else { String::new() };
        return Outcome::Error(format!("{}{note}", error.chars().take(600).collect::<String>()));
    }
    let Some(status) = reply["status"].as_u64() else {
        return unreadable();
    };
    let headers = reply["headers"]
        .as_object()
        .map(|map| map.iter().take(64).map(|(name, value)| (name.clone(), value.as_str().unwrap_or_default().chars().take(2000).collect())).collect())
        .unwrap_or_default();
    let mut warnings: Vec<String> =
        reply["warnings"].as_array().map(|list| list.iter().filter_map(Value::as_str).take(4).map(|text| text.chars().take(600).collect()).collect()).unwrap_or_default();
    if rollback && !rolled_back {
        warnings.push(ROLLBACK_MISMATCH.into());
    }
    Outcome::Response(Box::new(Response {
        status,
        reason: reply["reason"].as_str().unwrap_or_default().to_owned(),
        headers,
        body: reply["body"].as_str().unwrap_or_default().chars().take(200_000).collect(),
        size: reply["size"].as_u64().unwrap_or_default(),
        ms: reply["ms"].as_f64().unwrap_or_default(),
        redirect: reply["redirect"].as_str().map(str::to_owned),
        rolled_back,
        warnings,
    }))
}

/// Starts the helper through the console runner (local `php` or the Docker target) without waiting;
/// it is stopped after 60 seconds or 1 MiB of output.
pub(crate) fn start(project: &Project, args: &[String]) -> Result<crate::console_runner::Running, Error> {
    crate::console_runner::spawn(project, args, SEND_BUDGET, MAX_OUTPUT, "The request")
}

/// While a request runs, its form shows what was sent, read-only.
fn frozen(field: Field) -> Field {
    let text = match &field.value {
        FieldValue::Text(text) | FieldValue::Choice(_, text) => text.clone(),
        FieldValue::Multiline(text) => return Field::new(&field.key, &field.label, FieldValue::readonly_text(text, true)),
        FieldValue::Checkbox(checked) => (if *checked { "Yes" } else { "No" }).to_owned(),
        _ => return field,
    };
    Field::new(&field.key, &field.label, FieldValue::readonly_text(&text, false))
}

fn size_text(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

fn handler_text(route: &Route) -> String {
    match &route.handler.member {
        Some(member) => format!("{}::{member}", route.handler.class),
        None => route.handler.class.clone(),
    }
}

impl Draft {
    /// `route.method` may list several methods joined with `|`; `ANY` offers the usual ones.
    fn new(route: &Route) -> Self {
        let listed: Vec<String> = route.method.split('|').map(|method| method.trim().to_ascii_uppercase()).filter(|method| !method.is_empty()).collect();
        let mut methods: Vec<String> = if listed.is_empty() || listed.iter().any(|method| method == "ANY") {
            ANY_METHODS.iter().map(|method| (*method).to_owned()).collect()
        } else {
            listed.clone()
        };
        if methods.len() > 1 {
            methods.retain(|method| method != "HEAD");
        }
        let path = route.path.clone();
        let name = route.name.as_deref().map(|name| format!("  ({name})")).unwrap_or_default();
        Self {
            label: format!("{} {}  {}{name}", listed.join("|"), path, handler_text(route)),
            params: placeholders(&path).into_iter().map(|placeholder| (placeholder.name, String::new())).collect(),
            method: methods[0].clone(),
            methods,
            json: path.trim_start_matches('/').starts_with("api"),
            path,
            query: String::new(),
            body: String::new(),
            user: String::new(),
            rollback: true,
            outcome: None,
            sending: None,
        }
    }

    fn request(&self) -> Result<Request, Error> {
        let uri = fill_path(&self.path, &self.params).map_err(Error::Invalid)?;
        let query = self.query.trim().trim_start_matches('?').to_owned();
        if query.len() > MAX_QUERY || query.chars().any(|c| c.is_control() || c == ' ') {
            return Err(Error::Invalid("Enter the query as key=value pairs joined with &, without spaces (up to 4096 characters).".into()));
        }
        let user = self.user.trim();
        if user.chars().count() > MAX_USER || user.chars().any(char::is_control) {
            return Err(Error::Invalid("Enter the user's identifier, such as an email or username, on one line (up to 254 characters).".into()));
        }
        let mut body = None;
        let text = self.body.trim();
        if !text.is_empty() && !NO_BODY_METHODS.contains(&self.method.as_str()) {
            if text.len() > MAX_BODY {
                return Err(Error::Invalid("The body is larger than 16 KiB.".into()));
            }
            let value: Value = serde_json::from_str(text).map_err(|error| Error::Invalid(format!("The body is not valid JSON: {error}")))?;
            if !value.is_object() && !value.is_array() {
                return Err(Error::Invalid("The body must be a JSON object or array.".into()));
            }
            body = Some(value);
        }
        Ok(Request { method: self.method.clone(), uri, query, body, user: (!user.is_empty()).then(|| user.to_owned()), json: self.json, rollback: self.rollback })
    }

    fn fields(&self) -> Vec<Field> {
        let mut fields = vec![
            Field::new("route", "Route", FieldValue::readonly_text(&self.label, false)),
            Field::new("method", "Method", FieldValue::Choice(self.methods.clone(), self.method.clone())),
        ];
        let placeholders = placeholders(&self.path);
        for (name, value) in &self.params {
            let placeholder = placeholders.iter().find(|placeholder| placeholder.name == *name);
            let mut notes = Vec::new();
            if let Some(requirement) = placeholder.and_then(|placeholder| placeholder.requirement.as_deref()) {
                notes.push(format!("matches {requirement}"));
            }
            match placeholder.and_then(|placeholder| placeholder.default.as_deref()) {
                Some("") => notes.push("optional".to_owned()),
                Some(default) => notes.push(format!("optional, default {default}")),
                None => {}
            }
            let label = if notes.is_empty() { name.clone() } else { format!("{name} ({})", notes.join("; ")) };
            fields.push(Field::new(&format!("param:{name}"), &label, FieldValue::Text(value.clone())));
        }
        fields.extend([
            Field::new("query", "Query", FieldValue::Text(self.query.clone())),
            Field::new("body", "JSON body (POST, PUT, PATCH, DELETE)", FieldValue::Multiline(self.body.clone())),
            Field::new("user", "Act as user (email or username)", FieldValue::Text(self.user.clone())),
            Field::new("json", "Accept JSON", FieldValue::Checkbox(self.json)),
            Field::new("rollback", "Roll back database changes (every Doctrine connection)", FieldValue::Checkbox(self.rollback)),
        ]);
        match &self.outcome {
            None => {}
            Some(Outcome::NotSent(message) | Outcome::Error(message)) => fields.push(Field::new("result", "Result", FieldValue::readonly_text(message, true))),
            Some(Outcome::Response(response)) => {
                let mut summary = format!("{} {} · {:.1} ms · {}", response.status, response.reason, response.ms, size_text(response.size));
                if self.rollback && response.rolled_back {
                    summary.push_str(" · changes rolled back");
                }
                fields.push(Field::new("result", "Result", FieldValue::readonly_text(&summary, false)));
                let mut warnings = response.warnings.clone();
                if !NO_BODY_METHODS.contains(&self.method.as_str()) && !self.json && (400..500).contains(&response.status) {
                    warnings.push("Forms with CSRF protection reject this request: Symfony checks the token of each form, and Send Request does not add one.".into());
                }
                if !warnings.is_empty() {
                    fields.push(Field::new("warning", "Warning", FieldValue::readonly_text(&warnings.join("\n"), true)));
                }
                if let Some(redirect) = &response.redirect {
                    fields.push(Field::new("redirect", "Redirect", FieldValue::readonly_text(redirect, false)));
                }
                let headers = response.headers.iter().map(|(name, value)| format!("{name}: {value}")).collect::<Vec<_>>().join("\n");
                fields.push(Field::new("headers", "Response headers", FieldValue::readonly_text(&headers, true)));
                fields.push(Field::new("response-body", "Response body", FieldValue::readonly_text(&response.body, true)));
            }
        }
        fields
    }
}

impl State {
    pub(crate) fn select(&mut self, root: &str, route: &Route) -> Result<(), Error> {
        if !self.drafts.contains_key(root) && self.drafts.len() >= MAX_DRAFTS {
            return Err(Error::Invalid("Too many projects have a request open. Reload the Symfony plugin in Settings › Plugins.".into()));
        }
        self.drafts.insert(root.to_owned(), Draft::new(route));
        Ok(())
    }

    pub(crate) fn set_field(&mut self, root: &str, key: &str, value: &str) -> Result<(), Error> {
        let draft = self.drafts.get_mut(root).ok_or(Error::Stale)?;
        match key {
            "method" if draft.methods.iter().any(|method| method == value) => draft.method = value.to_owned(),
            "query" => draft.query = value.chars().take(2 * MAX_QUERY).collect(),
            "body" => draft.body = value.chars().take(4 * MAX_BODY).collect(),
            "user" => draft.user = value.chars().take(2 * MAX_USER).collect(),
            "json" => draft.json = value == "true",
            "rollback" => draft.rollback = value == "true",
            _ => {
                if let Some((_, slot)) = key.strip_prefix("param:").and_then(|name| draft.params.iter_mut().find(|(known, _)| known == name)) {
                    *slot = value.chars().take(512).collect();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn request(&self, root: &str) -> Result<Request, Error> {
        self.drafts.get(root).ok_or(Error::Stale)?.request()
    }

    /// Starts the draft of `root` through `start` and returns; a write method asks first. Nothing
    /// runs when the confirmation is cancelled or a request is already running.
    pub(crate) fn send(&mut self, root: &str, start: impl FnOnce(&[String]) -> Result<crate::console_runner::Running, Error>) -> Result<(), Error> {
        let draft = self.drafts.get_mut(root).ok_or(Error::Stale)?;
        if draft.sending.is_some() {
            return Ok(());
        }
        let request = draft.request()?;
        if let Some((title, body)) = write_confirm(root, &request) {
            if !host::confirm(&title, &body)? {
                return Ok(());
            }
        }
        draft.outcome = None;
        let nonce = crate::database::nonce();
        let run = start(&request.php_args(&nonce))?;
        draft.sending = Some(Sending { run, nonce, rollback: request.rollback });
        Ok(())
    }

    /// Reads what the running request of `root` printed, without waiting. `true` once it has ended
    /// and its outcome is stored. A hidden tab sends no polls: [`State::reap_overdue`] covers it.
    pub(crate) fn poll(&mut self, root: &str) -> bool {
        let Some(draft) = self.drafts.get_mut(root) else { return false };
        let Some(sending) = draft.sending.as_mut() else { return false };
        let outcome = match sending.run.step(0) {
            Ok(None) => return false,
            Ok(Some(finished)) => parse_output(&finished.stdout, &sending.nonce, sending.rollback),
            Err(error) => Outcome::Error(error.to_string()),
        };
        draft.sending = None;
        draft.outcome = Some(outcome);
        true
    }

    /// Stops every request past its time limit and stores that as its outcome, so a hidden tab
    /// shows it when it opens again. No host call unless one is stopped.
    pub(crate) fn reap_overdue(&mut self) {
        for draft in self.drafts.values_mut() {
            if let Some(error) = draft.sending.as_mut().and_then(|sending| sending.run.reap()) {
                draft.sending = None;
                draft.outcome = Some(Outcome::Error(error.to_string()));
            }
        }
    }

    /// Puts the running request of `root` past its time limit.
    #[cfg(test)]
    pub(crate) fn expire(&mut self, root: &str) {
        self.drafts.get_mut(root).and_then(|draft| draft.sending.as_mut()).expect("a running request").run.expire();
    }

    #[cfg(test)]
    pub(crate) fn is_sending(&self, root: &str) -> bool {
        self.drafts.get(root).is_some_and(|draft| draft.sending.is_some())
    }

    /// Stops the running request of `root`; it shows no outcome.
    pub(crate) fn cancel(&mut self, root: &str) {
        if let Some(draft) = self.drafts.get_mut(root) {
            draft.sending = None;
        }
    }

    pub(crate) fn render(&self, root: &str) -> View {
        let Some(draft) = self.drafts.get(root) else {
            return View::message("No request yet. Choose a route to build one.").button(Button::new(CHOOSE_ROUTE, "Choose Route…").icon("send"));
        };
        if draft.sending.is_some() {
            let form = Body::Form(draft.fields().into_iter().map(frozen).collect());
            return View::new(ui_kit::web::with_background_poll(form, POLL_MS, json!({"op": TICK})))
                .status("Sending…")
                .button(Button::new(CANCEL, "Cancel").icon("stop"))
                .button(Button::new("request-copy-curl", "Copy as cURL").icon("copy"));
        }
        View::new(Body::Form(draft.fields()))
            .button(Button::new("request-send", "Send Request").icon("run"))
            .button(Button::new("request-copy-curl", "Copy as cURL").icon("copy"))
            .button(Button::new(CHOOSE_ROUTE, "Choose Route…").icon("send"))
    }
}

/// The confirmation a write method needs: what roll back does not undo, or that nothing is undone.
fn write_confirm(root: &str, request: &Request) -> Option<(String, String)> {
    if !is_write(&request.method) {
        return None;
    }
    let target = format!("{} {}", request.method, request.uri);
    Some(if request.rollback {
        (format!("Send {target}?"), format!("Project: {root}\nDatabase changes are rolled back; emails, HTTP calls, Messenger messages and files the request makes are not."))
    } else {
        (format!("Send {target} without roll back?"), format!("Project: {root}\nDatabase writes, emails and files the request makes are kept.\nThis can't be undone."))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use ui_kit::routes::Handler;

    pub(crate) fn route(method: &str, path: &str) -> Route {
        Route { method: method.into(), path: path.into(), handler: Handler { class: "PostController".into(), member: Some("store".into()) }, name: None, location: None }
    }

    fn state(method: &str, path: &str) -> State {
        let mut state = State::default();
        state.select("/p", &route(method, path)).unwrap();
        state
    }

    const N: &str = "0123456789abcdef0123456789abcdef";
    const NOT_SENT: &str = r#"noise
@@symfony-request-0123456789abcdef0123456789abcdef@@{"notSent":true,"error":"Nothing was sent: Roll back database changes is on, but a transaction could not be started on every Doctrine connection (Doctrine is not installed)."}
"#;
    const SENT: &str = r#"@@symfony-request-0123456789abcdef0123456789abcdef@@{"status":201,"reason":"Created","headers":{"content-type":"application/json","set-cookie":"s=•••"},"body":"{}","size":2,"ms":4.5,"redirect":null,"warnings":[],"rollbackRequested":true,"rolledBack":true}
"#;
    const CREATED: &str = r#"{"status":201,"reason":"Created","headers":{},"body":"{}","size":2,"ms":4.5,"rolledBack":true}"#;

    fn php_host() -> MockHost {
        MockHost::install().with_capabilities(&["project.read", "storage", "process:php"])
    }

    fn started(args: &[String]) -> Result<crate::console_runner::Running, Error> {
        start(&host::projects()?.remove(0), args)
    }

    fn sending(state: &mut State) -> &mut Sending {
        state.drafts.get_mut("/p").unwrap().sending.as_mut().expect("a running request")
    }

    /// The running request ends with the helper's reply `json` on its marker line; whether the
    /// next poll shows it.
    fn reply(state: &mut State, json: &str) -> bool {
        let sending = sending(state);
        let raw = format!("noise\n@@symfony-request-{}@@{json}\n", sending.nonce);
        sending.run.finish(&raw);
        state.poll("/p")
    }

    #[test]
    fn placeholders_with_requirements_defaults_and_quantifiers() {
        let found = placeholders("/blog/{slug}/{id<\\d{2,4}>}/{page<\\d+>?1}/{tag?}/{!raw}");
        let shown: Vec<_> = found.iter().map(|p| (p.name.as_str(), p.requirement.as_deref(), p.default.as_deref())).collect();
        assert_eq!(shown, [("slug", None, None), ("id", Some("\\d{2,4}"), None), ("page", Some("\\d+"), Some("1")), ("tag", None, Some("")), ("raw", None, None)]);
        let params = vec![("slug".to_owned(), "a b/1".to_owned()), ("id".to_owned(), "42".to_owned()), ("raw".to_owned(), "x".to_owned())];
        assert_eq!(fill_path("/blog/{slug}/{id<\\d{2,4}>}/{!raw}/{page<\\d+>?1}", &params).unwrap(), "/blog/a%20b%2F1/42/x");
        assert_eq!(fill_path("/blog/{slug}", &[]).unwrap_err(), "Fill in the slug parameter.");
    }

    #[test]
    fn form_shows_a_field_per_placeholder_with_its_requirement_and_default() {
        let _host = MockHost::install();
        let state = state("GET|HEAD", "/posts/{id<\\d+>}/{page?1}");
        let fields = state.drafts["/p"].fields();
        let labels: Vec<_> = fields.iter().map(|field| (field.key.as_str(), field.label.as_str())).collect();
        assert_eq!(&labels[..4], [("route", "Route"), ("method", "Method"), ("param:id", "id (matches \\d+)"), ("param:page", "page (optional, default 1)")]);
        let FieldValue::Choice(methods, selected) = &fields[1].value else { panic!("method choice") };
        assert_eq!((methods.as_slice(), selected.as_str()), (["GET".to_owned()].as_slice(), "GET"));
        assert!(fields.iter().any(|field| field.key == "rollback" && field.value == FieldValue::Checkbox(true)));
        assert!(fields.iter().any(|field| field.key == "json" && field.value == FieldValue::Checkbox(false)));
        assert!(state.render("/p").toolbar.iter().any(|button| button.id == "request-send"));
        assert!(matches!(State::default().render("/p").body, Body::Message(_)));
    }

    #[test]
    fn any_offers_the_usual_methods_and_api_paths_accept_json() {
        let state = state("ANY", "/api/posts");
        let draft = &state.drafts["/p"];
        assert_eq!(draft.methods, ANY_METHODS.map(String::from));
        assert!(draft.json);
    }

    #[test]
    fn no_transaction_means_not_sent_and_rollback_stays_on() {
        let host = php_host();
        let mut state = state("POST", "/posts");
        let mut args_seen = Vec::new();
        host.answer_next_prompt(Some(&["ok"]));
        state
            .send("/p", |args| {
                args_seen = args.to_vec();
                started(args)
            })
            .unwrap();
        assert!(reply(&mut state, r#"{"notSent":true,"error":"Nothing was sent: Doctrine is not installed."}"#));
        let sent: Value = serde_json::from_str(args_seen.last().unwrap()).unwrap();
        assert_eq!(sent["rollback"], json!(true));
        let draft = &state.drafts["/p"];
        let Some(Outcome::NotSent(reason)) = &draft.outcome else { panic!("expected not sent") };
        assert!(reason.starts_with("Nothing was sent:"));
        assert!(draft.fields().iter().all(|field| field.key != "response-body" && field.key != "headers"));
    }

    #[test]
    fn a_write_always_asks_first_and_a_cancelled_confirm_runs_nothing() {
        let host = php_host();
        for rollback in ["true", "false"] {
            let mut state = state("DELETE", "/posts/{id}");
            state.set_field("/p", "param:id", "7").unwrap();
            state.set_field("/p", "rollback", rollback).unwrap();
            state.send("/p", |_| panic!("must not run without the confirm")).unwrap();
            assert!(state.drafts["/p"].outcome.is_none() && state.drafts["/p"].sending.is_none());
            let mut runs = 0;
            host.answer_next_prompt(Some(&["ok"]));
            state
                .send("/p", |args| {
                    runs += 1;
                    assert!(args.last().unwrap().contains(&format!(r#""rollback":{rollback}"#)));
                    started(args)
                })
                .unwrap();
            state.send("/p", |_| panic!("one request at a time")).unwrap();
            assert!(reply(&mut state, CREATED));
            assert_eq!(runs, 1);
            assert!(matches!(&state.drafts["/p"].outcome, Some(Outcome::Response(_))));
            assert!(state.render("/p").toolbar.iter().all(|button| button.label != "Send for Real (keeps database changes)"));
        }
        let mut reads = state("GET", "/posts");
        reads.set_field("/p", "rollback", "false").unwrap();
        reads.send("/p", started).unwrap();
        assert!(reply(&mut reads, CREATED));
        assert!(matches!(&reads.drafts["/p"].outcome, Some(Outcome::Response(_))), "a read sends on the first click");
    }

    #[test]
    fn the_write_confirm_says_what_roll_back_does_not_undo() {
        let mut state = state("DELETE", "/posts/{id}");
        state.set_field("/p", "param:id", "7").unwrap();
        let (title, body) = write_confirm("/p", &state.request("/p").unwrap()).unwrap();
        assert_eq!(title, "Send DELETE /posts/7?");
        assert_eq!(body, "Project: /p\nDatabase changes are rolled back; emails, HTTP calls, Messenger messages and files the request makes are not.");
        state.set_field("/p", "rollback", "false").unwrap();
        let (title, body) = write_confirm("/p", &state.request("/p").unwrap()).unwrap();
        assert_eq!(title, "Send DELETE /posts/7 without roll back?");
        assert_eq!(body, "Project: /p\nDatabase writes, emails and files the request makes are kept.\nThis can't be undone.");
        assert!(write_confirm("/p", &self::state("GET", "/posts").request("/p").unwrap()).is_none());
    }

    #[test]
    fn parse_output_reads_the_marker_not_sent_and_errors() {
        let _host = MockHost::install();
        let Outcome::Response(response) = parse_output(SENT, N, true) else { panic!("response") };
        assert!(response.rolled_back && response.warnings.is_empty() && response.status == 201);
        assert!(response.headers.iter().any(|(name, value)| name == "set-cookie" && value == "s=•••"));
        assert!(matches!(parse_output(NOT_SENT, N, true), Outcome::NotSent(reason) if reason.contains("Doctrine is not installed")));
        assert!(matches!(parse_output("PHP Fatal error", N, true), Outcome::Error(message) if message.contains("without a response")));
        assert!(matches!(parse_output(&format!("@@symfony-request-{N}@@{{nope"), N, false), Outcome::Error(message) if message == "Send Request got no readable result from PHP. Check the PHP output in the IDE log, then try again."));
        let boom = format!(r#"@@symfony-request-{N}@@{{"error":"RuntimeException: x","rollbackRequested":true,"rolledBack":false}}"#);
        assert!(matches!(parse_output(&boom, N, true), Outcome::Error(message) if message.starts_with("RuntimeException: x Roll back was on")));
        let kept = format!(r#"@@symfony-request-{N}@@{{"status":200,"reason":"OK","headers":{{}},"body":"","size":0,"ms":1,"warnings":["The firewall replaced the Act as user token"],"rolledBack":false}}"#);
        let Outcome::Response(response) = parse_output(&kept, N, true) else { panic!("response") };
        assert_eq!(response.warnings.len(), 2);
        let Outcome::Response(response) = parse_output(&kept, N, false) else { panic!("response") };
        assert_eq!(response.warnings, ["The firewall replaced the Act as user token"]);
    }

    #[test]
    fn a_marker_in_the_body_or_with_another_nonce_is_not_the_reply() {
        let _host = MockHost::install();
        let forged = format!(
            "{SENT}@@symfony-request-{}@@{{\"status\":500,\"reason\":\"Forged\",\"headers\":{{}},\"body\":\"\",\"size\":0,\"ms\":1,\"rolledBack\":true}}\nbody: @@symfony-request-{N}@@{{\"status\":418}}\n@@symfony-request@@{{\"status\":418}}\n",
            "f".repeat(32)
        );
        let Outcome::Response(response) = parse_output(&forged, N, true) else { panic!("response") };
        assert_eq!(response.status, 201);
        assert!(matches!(parse_output(&SENT.replace(N, &"f".repeat(32)), N, true), Outcome::Error(message) if message.contains("without a response")));
    }

    #[test]
    fn a_rejected_form_post_mentions_csrf() {
        let host = php_host();
        let mut state = state("POST", "/posts");
        host.answer_next_prompt(Some(&["ok"]));
        state.send("/p", started).unwrap();
        assert!(reply(&mut state, r#"{"status":422,"reason":"Unprocessable Content","headers":{},"body":"","size":0,"ms":1,"rolledBack":true}"#));
        let fields = state.drafts["/p"].fields();
        assert!(fields.iter().any(|field| field.key == "warning" && matches!(&field.value, FieldValue::Other { data, .. } if data.contains("CSRF"))));
    }


    #[test]
    fn invalid_input_is_refused_before_anything_runs() {
        let _host = MockHost::install();
        let mut state = state("POST", "/posts/{id}");
        assert!(state.send("/p", |_| panic!("must not run")).unwrap_err().to_string().contains("Fill in the id parameter."));
        state.set_field("/p", "param:id", "1").unwrap();
        state.set_field("/p", "body", "{nope").unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "body", "42").unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "body", &format!("[\"{}\"]", "x".repeat(MAX_BODY))).unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "body", "{}").unwrap();
        state.set_field("/p", "user", "a\u{7}b").unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "user", &"u".repeat(MAX_USER + 1)).unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "user", "ada@example.com").unwrap();
        state.set_field("/p", "query", "a b").unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "query", &"a".repeat(MAX_QUERY + 1)).unwrap();
        assert!(state.send("/p", |_| panic!("must not run")).is_err());
        state.set_field("/p", "query", "?a=1").unwrap();
        let request = state.request("/p").unwrap();
        assert_eq!((request.query.as_str(), request.user.as_deref()), ("a=1", Some("ada@example.com")));
        let mut full = State::default();
        for root in 0..MAX_DRAFTS {
            full.select(&format!("/p{root}"), &route("GET", "/")).unwrap();
        }
        assert!(full.select("/other", &route("GET", "/")).is_err());
        assert!(full.select("/p0", &route("GET", "/")).is_ok());
    }

    #[test]
    fn curl_is_built_from_the_same_request_that_is_sent() {
        let _host = MockHost::install();
        let mut state = state("POST", "/posts/{id}");
        state.set_field("/p", "param:id", "7").unwrap();
        state.set_field("/p", "query", "draft=1").unwrap();
        state.set_field("/p", "json", "true").unwrap();
        state.set_field("/p", "body", r#"{"title":"Example"}"#).unwrap();
        let request = state.request("/p").unwrap();
        assert_eq!(
            request.curl("http://localhost:8000/"),
            r#"curl -X POST 'http://localhost:8000/posts/7?draft=1' -H 'Accept: application/json' -H 'Content-Type: application/json' --data '{"title":"Example"}'"#
        );
        let sent: Value = serde_json::from_str(request.php_args(N).last().unwrap()).unwrap();
        assert_eq!((sent["uri"].as_str(), sent["query"].as_str(), sent["body"]["title"].as_str()), (Some("/posts/7"), Some("draft=1"), Some("Example")));
        state.set_field("/p", "json", "false").unwrap();
        assert!(state.request("/p").unwrap().curl("http://h").contains("--data-urlencode 'title=Example'"));
    }

    #[test]
    fn execution_goes_through_the_console_runner_locally_and_in_docker() {
        use crate::docker_target::tests::{choose_first, raw};
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "process:php", "process:docker"]);
        let project = host.invocation().project().unwrap();
        let args = state("GET", "/").request("/p").unwrap().php_args(N);
        let execute = |project: &Project, args: &[String]| crate::console_runner::collect(start(project, args)?, || false);
        let local = execute(&project, &args).unwrap();
        assert!(local.starts_with("started: php\n"), "{local}");
        assert!(local.contains("@@symfony-request-") && local.contains(N) && local.contains(r#"\"uri\":\"/\""#), "the script and its input go over stdin");

        choose_first(&host, raw());
        host.on_process("docker", |args, _| {
            assert_eq!(args[2], "inspect");
            Ok(openidea_plugin::ProcessOutput { status: Some(0), stdout: raw().to_string(), stderr: String::new() })
        });
        let remote = execute(&project, &args).unwrap();
        assert!(remote.starts_with("started: docker\n"), "{remote}");

        let denied = MockHost::install().with_capabilities(&["project.read"]);
        let error = execute(&denied.invocation().project().unwrap(), &args).unwrap_err().to_string();
        assert!(error.contains("process:php"), "{error}");
    }

    #[test]
    fn send_returns_at_once_and_the_form_polls_read_only_until_the_response() {
        let _host = php_host();
        let mut state = state("GET", "/posts/{id}");
        state.set_field("/p", "param:id", "7").unwrap();
        state.send("/p", started).unwrap();
        let View { body, status, toolbar, .. } = state.render("/p");
        assert_eq!(status.as_deref(), Some("Sending…"));
        let buttons: Vec<_> = toolbar.iter().map(|button| button.id.as_str()).collect();
        assert_eq!(buttons, [CANCEL, "request-copy-curl"]);
        let Body::Other { kind, data } = body else { panic!("a polled form") };
        let data: Value = serde_json::from_str(&data).unwrap();
        assert_eq!((kind.as_str(), &data["backgroundPoll"]), ("form", &json!({"intervalMs": POLL_MS, "message": {"op": TICK}})));
        assert!(data["fields"].as_array().unwrap().iter().all(|field| field["value"]["kind"] == "readonly-text"), "{data}");

        for _ in 0..3 {
            assert!(!state.poll("/p"), "a running request keeps polling");
        }
        assert!(reply(&mut state, SENT.split("@@").last().unwrap().trim()));
        assert!(matches!(&state.drafts["/p"].outcome, Some(Outcome::Response(response)) if response.status == 201));
        let View { body, status, toolbar, .. } = state.render("/p");
        assert!(matches!(body, Body::Form(_)) && status.is_none());
        assert!(toolbar.iter().any(|button| button.id == "request-send"));
        assert!(!state.poll("/p"), "a stale tick does nothing");
    }

    #[test]
    fn cancel_stops_the_request_and_shows_no_outcome() {
        let _host = php_host();
        let mut state = state("GET", "/posts");
        state.send("/p", started).unwrap();
        state.cancel("/p");
        assert!(state.drafts["/p"].sending.is_none() && state.drafts["/p"].outcome.is_none());
        assert!(!state.poll("/p"));
        assert!(matches!(state.render("/p").body, Body::Form(_)));
    }

    #[test]
    fn a_request_past_its_time_or_output_limit_is_stopped_with_the_reason() {
        let _host = php_host();
        let mut state = state("GET", "/posts");
        state.send("/p", started).unwrap();
        sending(&mut state).run.expire();
        assert!(state.poll("/p"));
        assert!(matches!(&state.drafts["/p"].outcome, Some(Outcome::Error(message)) if message == "The request ran longer than 60 seconds and was stopped."));

        state.send("/p", started).unwrap();
        sending(&mut state).run.finish(&"x".repeat(MAX_OUTPUT + 1));
        assert!(state.poll("/p"));
        assert!(matches!(&state.drafts["/p"].outcome, Some(Outcome::Error(message)) if message == "The request printed more than 1 MiB and was stopped."));
    }

    #[test]
    fn reaping_stops_only_an_overdue_request_and_keeps_the_reason_for_the_hidden_tab() {
        let _host = php_host();
        let mut state = state("GET", "/posts");
        state.send("/p", started).unwrap();
        state.reap_overdue();
        assert!(state.drafts["/p"].sending.is_some(), "within its limit");
        sending(&mut state).run.expire();
        state.reap_overdue();
        assert!(state.drafts["/p"].sending.is_none());
        assert!(matches!(&state.drafts["/p"].outcome, Some(Outcome::Error(message)) if message == "The request ran longer than 60 seconds and was stopped."));
        assert!(!state.poll("/p"));
    }

    const STUBS: &str = r#"<?php
namespace Symfony\Component\HttpFoundation {
    class Request { public $method; public $content; public static function create($uri, $method, $p, $c, $f, $s, $content) { $r = new static; $r->method = $method; $r->content = $content; return $r; } }
    class Response { public static $statusTexts = [200 => 'OK']; public $headers; function __construct() { $this->headers = new Headers; } function getStatusCode() { return 200; } function getContent() { return 'ok'; } }
    class Headers { function all() { return ['content-type' => ['text/plain'], 'set-cookie' => ['secret=hunter2'], 'authorization' => ['Bearer x']]; } function getCookies() { return [new Cookie]; } function get($n) { return null; } }
    class Cookie { function getName() { return 'session'; } }
}
namespace Symfony\Component\HttpKernel { interface TerminableInterface {} }
namespace Symfony\Component\Security\Core\User { interface UserInterface {} }
namespace Symfony\Component\Security\Core\Authentication\Token {
    class UsernamePasswordToken { public $user; function __construct($user, $firewall, $roles) { $this->user = $user; } function getUser() { return $this->user; } }
}
namespace {
    function mode() { return trim(file_get_contents(getcwd().'/mode.txt')); }
    class Conn {
        public $level = 0;
        function __construct(public $name) {}
        function beginTransaction() {
            if (mode() === 'throws') { throw new RuntimeException('no tx'); }
            if (mode() === 'audit-throws' && $this->name === 'audit') { throw new RuntimeException('SQLSTATE[HY000] [2002] mysql://u:p@ss@audit-db/log refused'); }
            if (mode() !== 'level0') { $this->level++; }
        }
        function isTransactionActive() { return $this->level > 0; }
        function rollBack() { $this->level--; file_put_contents(getcwd().'/rolledback.marker', $this->name."\n", FILE_APPEND); }
    }
    class Doctrine {
        public $c = [];
        function getConnectionNames() { return ['default' => 'doctrine.dbal.default_connection', 'audit' => 'doctrine.dbal.audit_connection']; }
        function getConnection($name = null) { return $this->c[$name ?? 'default'] ??= new Conn($name ?? 'default'); }
        function getManagers() { return []; }
    }
    class Storage { public $token; function setToken($t) { $this->token = mode() === 'replaced' ? null : $t; } function getToken() { return $this->token; } }
    class Container {
        public $services = [];
        function __construct() { $this->services['security.token_storage'] = new Storage; if (mode() !== 'nodoctrine') { $this->services['doctrine'] = new Doctrine; } $this->services['security.user_providers'] = new Provider; }
        function has($id) { return isset($this->services[$id]); }
        function get($id) { return $this->services[$id]; }
    }
    class User implements Symfony\Component\Security\Core\User\UserInterface { function getRoles() { return ['ROLE_USER']; } function getUserIdentifier() { return 'ada'; } }
    class Provider { function loadUserByIdentifier($id) { if ($id !== 'ada') { throw new RuntimeException('unknown'); } return new User; } }
}
namespace App {
    class Kernel implements \Symfony\Component\HttpKernel\TerminableInterface {
        public $container;
        function __construct($env, $debug) { file_put_contents(getcwd().'/env.txt', $env.'|'.var_export($debug, true)); }
        function boot() { if (mode() === 'bootfail') { throw new \RuntimeException('An exception occurred in the driver: postgresql://app:p@ss@db:5432/app unreachable'); } $this->container = new \Container; }
        function getContainer() { return $this->container; }
        function handle($r) { file_put_contents(getcwd().'/sent.marker', $r->method.' '.$r->content); return new \Symfony\Component\HttpFoundation\Response; }
        function terminate($r, $s) {}
    }
}
"#;

    /// What one run of the real helper did: its stdout, what the kernel got and which connections
    /// were rolled back, one name per line.
    struct Run {
        output: String,
        sent: Option<String>,
        rolled_back: String,
    }

    /// Runs the real helper script with `php` against stub classes; `None` when php is missing.
    fn run_php(mode: &str, rollback: bool, user: Option<&str>) -> Option<Run> {
        let dir = std::env::temp_dir().join(format!("symfony-request-{}-{mode}-{rollback}-{}", std::process::id(), user.unwrap_or("-")));
        std::fs::create_dir_all(dir.join("vendor")).ok()?;
        std::fs::write(dir.join("vendor/autoload.php"), STUBS).ok()?;
        std::fs::write(dir.join("mode.txt"), mode).ok()?;
        let request = Request { method: "POST".into(), uri: "/posts".into(), query: String::new(), body: Some(json!({"a": 1})), user: user.map(str::to_owned), json: true, rollback };
        let output = std::process::Command::new("php").args(request.php_args(N)).current_dir(&dir).env("APP_ENV", "test").env("APP_DEBUG", "0").output().ok()?;
        let sent = std::fs::read_to_string(dir.join("sent.marker")).ok();
        let rolled_back = std::fs::read_to_string(dir.join("rolledback.marker")).unwrap_or_default();
        let env = std::fs::read_to_string(dir.join("env.txt")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        if sent.is_some() {
            assert_eq!(env, "test|false");
        }
        Some(Run { output: String::from_utf8_lossy(&output.stdout).into_owned(), sent, rolled_back })
    }

    #[test]
    fn php_never_sends_with_rollback_on_unless_a_transaction_started() {
        let _host = MockHost::install();
        for mode in ["throws", "level0", "nodoctrine"] {
            let Some(Run { output, sent, .. }) = run_php(mode, true, None) else {
                eprintln!("php is not on PATH; skipping the helper script check");
                return;
            };
            assert!(sent.is_none(), "{mode}: the kernel must not run");
            assert!(matches!(parse_output(&output, N, true), Outcome::NotSent(reason) if reason.starts_with("Nothing was sent:")), "{mode}: {output}");
        }
        let Run { output, sent, .. } = run_php("nodoctrine", false, None).unwrap();
        assert_eq!(sent.as_deref(), Some(r#"POST {"a":1}"#), "rollback off sends for real");
        assert!(matches!(parse_output(&output, N, false), Outcome::Response(_)), "{output}");
    }

    #[test]
    fn php_opens_a_transaction_on_every_connection_or_sends_nothing() {
        let _host = MockHost::install();
        let Some(run) = run_php("ok", true, None) else { return };
        assert!(run.sent.is_some());
        let mut rolled: Vec<&str> = run.rolled_back.lines().collect();
        rolled.sort();
        assert_eq!(rolled, ["audit", "default"], "both connections are rolled back");

        let run = run_php("audit-throws", true, None).unwrap();
        assert!(run.sent.is_none(), "one connection without a transaction keeps the request from running");
        assert_eq!(run.rolled_back.trim(), "default", "the transaction already open is rolled back");
        let Outcome::NotSent(reason) = parse_output(&run.output, N, true) else { panic!("{}", run.output) };
        assert!(reason.contains("every Doctrine connection") && reason.contains("connection audit"), "{reason}");
        assert!(reason.contains("mysql://•••@audit-db/log") && !run.output.contains("p@ss") && !run.output.contains("u:p"), "{reason}");
    }

    #[test]
    fn php_removes_a_password_with_an_at_sign_from_boot_errors() {
        let _host = MockHost::install();
        let Some(run) = run_php("bootfail", true, None) else { return };
        let Outcome::NotSent(reason) = parse_output(&run.output, N, true) else { panic!("{}", run.output) };
        assert!(reason.contains("postgresql://•••@db:5432/app") && !run.output.contains("p@ss") && !run.output.contains("app:p"), "{reason}");
    }

    #[test]
    fn php_rolls_back_masks_secrets_and_acts_as_the_user() {
        let _host = MockHost::install();
        let Some(Run { output, sent, .. }) = run_php("ok", true, Some("ada")) else { return };
        assert!(sent.is_some());
        let Outcome::Response(response) = parse_output(&output, N, true) else { panic!("{output}") };
        assert!(response.rolled_back && response.warnings.is_empty(), "{output}");
        assert!(!output.contains("hunter2") && !output.contains("Bearer"));
        assert!(response.headers.iter().any(|(name, value)| name == "set-cookie" && value == "session=•••"));

        let Run { output, sent, .. } = run_php("ok", true, Some("bob")).unwrap();
        assert!(sent.is_none());
        assert!(matches!(parse_output(&output, N, true), Outcome::NotSent(reason) if reason.contains("no user bob")), "{output}");

        let Run { output, .. } = run_php("replaced", true, Some("ada")).unwrap();
        let Outcome::Response(response) = parse_output(&output, N, true) else { panic!("{output}") };
        assert!(response.warnings.iter().any(|warning| warning.contains("firewall replaced")), "{output}");
    }
}
