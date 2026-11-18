//! Monolog records from the tail of a log file: the line format
//! `[datetime] channel.LEVEL: message {context} {extra}` and JSON lines. Context is masked,
//! exceptions give stack frames. Adapted from the Laravel plugin's runtime_errors.rs.
use serde_json::{Map, Value, json};

pub(crate) const MAX_ENTRIES: usize = 500;
const MAX_FRAMES: usize = 64;
const MAX_EXCEPTIONS: usize = 8;
const MAX_MESSAGE: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
const MAX_CONTEXT: usize = 4096;
const SECRET_KEYS: [&str; 10] = ["password", "passwd", "secret", "token", "authorization", "api_key", "apikey", "api-key", "cookie", "credential"];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Frame {
    pub path: String,
    pub line: u32,
    pub call: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Exception {
    pub class: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Entry {
    pub time: String,
    pub channel: String,
    pub level: String,
    pub message: String,
    /// Masked compact JSON, empty when there is none.
    pub context: String,
    pub extra: String,
    pub exceptions: Vec<Exception>,
    pub frames: Vec<Frame>,
}

fn level(name: &str) -> bool {
    matches!(name, "DEBUG" | "INFO" | "NOTICE" | "WARNING" | "ERROR" | "CRITICAL" | "ALERT" | "EMERGENCY")
}

fn cut(value: &str, limit: usize) -> &str {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// `scheme://user:pass@host` → `scheme://[redacted]@host`.
fn hide_url_credentials(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        output.push_str(head);
        let authority = tail.find(|c: char| c == '/' || c.is_whitespace() || c == '"').unwrap_or(tail.len());
        match tail[..authority].rfind('@') {
            Some(user) if tail[..user].contains(':') => {
                output.push_str("[redacted]");
                rest = &tail[user..];
            }
            _ => rest = tail,
        }
    }
    output.push_str(rest);
    output
}

/// Bounded display text with control characters replaced and `password=…`-style values redacted.
pub(crate) fn display(value: &str, limit: usize) -> String {
    let input: String = cut(value, limit).chars().map(|c| if c.is_control() && !matches!(c, '\n' | '\t') { '�' } else { c }).collect();
    let input = hide_url_credentials(&input);
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < input.len() {
        let mut candidate: Option<(usize, usize, bool)> = None;
        for key in ["password", "passwd", "authorization", "api_key", "api-key", "access_token", "refresh_token", "secret", "token"] {
            let mut search = cursor;
            while let Some(relative) = lower[search..].find(key) {
                let start = search + relative;
                search = start + key.len();
                if start.checked_sub(1).and_then(|at| bytes.get(at)).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
                    continue;
                }
                let mut at = start + key.len();
                if matches!(bytes.get(at), Some(b'"' | b'\'')) {
                    at += 1;
                }
                while bytes.get(at).is_some_and(|b| b.is_ascii_whitespace() && *b != b'\n') {
                    at += 1;
                }
                if !matches!(bytes.get(at), Some(b':' | b'=')) {
                    continue;
                }
                at += 1;
                while bytes.get(at).is_some_and(|b| b.is_ascii_whitespace() && *b != b'\n') {
                    at += 1;
                }
                if at == input.len() {
                    continue;
                }
                if candidate.is_none_or(|(old, _, _)| start < old) {
                    candidate = Some((start, at, key == "authorization"));
                }
                break;
            }
        }
        let Some((_, start, authorization)) = candidate else {
            output.push_str(&input[cursor..]);
            break;
        };
        output.push_str(&input[cursor..start]);
        let mut finish = start;
        if matches!(bytes[start], b'"' | b'\'') {
            let quote = bytes[start];
            finish += 1;
            while finish < bytes.len() {
                if bytes[finish] == b'\\' {
                    finish = (finish + 2).min(bytes.len());
                    continue;
                }
                finish += 1;
                if bytes[finish - 1] == quote {
                    break;
                }
            }
        } else {
            while finish < bytes.len()
                && !matches!(bytes[finish], b'\n' | b'\r' | b',' | b'}' | b']' | b';')
                && (authorization || !bytes[finish].is_ascii_whitespace())
            {
                finish += 1;
            }
        }
        if finish == start {
            output.push_str(&input[start..start + 1]);
            cursor = start + 1;
        } else {
            output.push_str("[redacted]");
            cursor = finish;
        }
    }
    output
}

fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    SECRET_KEYS.iter().any(|secret| key.contains(secret))
}

/// Context with secret-looking keys replaced, strings redacted and depth and size bounded.
fn mask(value: &Value, depth: usize) -> Value {
    match value {
        Value::Object(map) if depth < 8 => Value::Object(
            map.iter()
                .take(100)
                .map(|(key, value)| (key.clone(), if secret_key(key) { json!("[redacted]") } else { mask(value, depth + 1) }))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) if depth < 8 => Value::Array(items.iter().take(100).map(|value| mask(value, depth + 1)).collect()),
        Value::Object(_) | Value::Array(_) => json!("…"),
        Value::String(text) => Value::String(display(text, 1024)),
        other => other.clone(),
    }
}

fn masked(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(value) if value.as_array().is_some_and(Vec::is_empty) || value.as_object().is_some_and(Map::is_empty) => String::new(),
        Some(value) => {
            let text = mask(value, 0).to_string();
            if text.len() > MAX_CONTEXT { format!("{}…", cut(&text, MAX_CONTEXT)) } else { text }
        }
    }
}

/// Windows paths become `C:/…`; anything but an absolute path with a positive line is refused.
fn valid_frame(path: &str, line: u32, call: &str) -> Option<Frame> {
    let path = if path.as_bytes().get(1) == Some(&b':') { path.replace('\\', "/") } else { path.to_owned() };
    let absolute = path.starts_with('/') || (path.as_bytes().get(1) == Some(&b':') && path.as_bytes().get(2) == Some(&b'/'));
    if line == 0 || path.len() > 4096 || !absolute || path.chars().any(char::is_control) {
        return None;
    }
    // Trace arguments can hold credentials: keep the callable only.
    let call = call.split_once('(').map_or_else(|| call.to_owned(), |(name, _)| format!("{name}(…)"));
    Some(Frame { path, line, call: display(&call, 512) })
}

fn location(value: &str, call: &str) -> Option<Frame> {
    let (path, line) = value.trim().rsplit_once(':')?;
    valid_frame(path, line.parse().ok()?, call)
}

/// `#0 /app/src/A.php(12): App\A->run()` or `/app/src/A.php:12`.
fn frame(line: &str) -> Option<Frame> {
    if let Some(tail) = line.trim_start().strip_prefix('#') {
        let (number, rest) = tail.split_once(' ')?;
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (source, call) = rest.split_once("): ")?;
        let (path, line) = source.rsplit_once('(')?;
        return valid_frame(path, line.parse().ok()?, call);
    }
    location(line, "")
}

fn push_frame(entry: &mut Entry, frame: Frame) {
    if entry.frames.len() < MAX_FRAMES && !entry.frames.iter().any(|old| old.path == frame.path && old.line == frame.line) {
        entry.frames.push(frame);
    }
}

fn push_exception(entry: &mut Entry, class: &str, message: &str) {
    if entry.exceptions.len() < MAX_EXCEPTIONS && (!class.is_empty() || !message.is_empty()) {
        entry.exceptions.push(Exception { class: display(class, 256), message: display(message, MAX_MESSAGE) });
    }
}

/// The line formatter's exception text: `[object] (Class(code: 0): message at /path:12)`, then
/// `[previous exception] [object] (…)` per cause and, with stack traces on, `#N path(line): call` lines.
fn exception_text(entry: &mut Entry, text: &str, escaped: bool) {
    let unescape = |value: &str| if escaped { value.replace("\\\\", "\\").replace("\\\"", "\"") } else { value.to_owned() };
    for segment in text.split("[object] (").skip(1).take(MAX_EXCEPTIONS) {
        let (head, trace) = segment.split_once("[stacktrace]").unwrap_or((segment, ""));
        let head = head.trim_end().trim_end_matches("[previous exception]").trim_end();
        if let Some((class, rest)) = head.split_once("(code: ") {
            let described = rest.split_once("): ").map_or("", |(_, message)| message);
            let (message, origin) = described.rsplit_once(" at ").unwrap_or((described, ""));
            push_exception(entry, &unescape(class), &unescape(message));
            if let Some(origin) = origin.split(')').next().and_then(|origin| location(&unescape(origin), "")) {
                push_frame(entry, origin);
            }
        }
        for line in trace.lines().take(MAX_FRAMES + 1) {
            if let Some(frame) = frame(&unescape(line)) {
                push_frame(entry, frame);
            }
        }
    }
}

fn structured_frame(value: &Value) -> Option<Frame> {
    if let Some(text) = value.as_str() {
        return frame(text);
    }
    let file = value["file"].as_str()?;
    let call = match (value["class"].as_str(), value["function"].as_str()) {
        (Some(class), Some(function)) => format!("{class}{}{function}", if value["type"].as_str() == Some("->") { "->" } else { "::" }),
        (_, Some(function)) => function.into(),
        _ => String::new(),
    };
    match value["line"].as_u64().and_then(|n| u32::try_from(n).ok()) {
        Some(line) => valid_frame(file, line, &call),
        None => location(file, &call),
    }
}

/// The JSON formatter's exception: `{class, message, file: "path:line", trace: [...], previous: {...}}`.
fn exception(entry: &mut Entry, value: &Value, depth: usize) {
    if depth >= MAX_EXCEPTIONS || value.is_null() {
        return;
    }
    if let Some(text) = value.as_str() {
        exception_text(entry, text, false);
        return;
    }
    push_exception(entry, value["class"].as_str().unwrap_or(""), value["message"].as_str().unwrap_or(""));
    if let Some(origin) = structured_frame(value) {
        push_frame(entry, origin);
    }
    for frame in value["trace"].as_array().into_iter().flatten().take(MAX_FRAMES).filter_map(structured_frame) {
        push_frame(entry, frame);
    }
    exception(entry, &value["previous"], depth + 1);
}

fn with_context(mut entry: Entry, context: Option<&Value>, extra: Option<&Value>) -> Entry {
    if let Some(found) = context.map(|context| &context["exception"]).filter(|found| !found.is_null()) {
        exception(&mut entry, found, 0);
    }
    let mut context = context.cloned();
    if !entry.exceptions.is_empty() {
        if let Some(Value::Object(map)) = &mut context {
            map.remove("exception");
        }
    }
    entry.context = masked(context.as_ref());
    entry.extra = masked(extra);
    entry
}

/// Leftmost split of `message {context} {extra}` where the tail is exactly two JSON values.
fn split_trailing_json(body: &str) -> Option<(&str, Value, Value)> {
    let bytes = body.as_bytes();
    let candidates = (0..bytes.len()).filter(|&at| matches!(bytes[at], b'{' | b'[') && (at == 0 || bytes[at - 1] == b' ')).take(64);
    for at in candidates {
        let tail = body[at..].trim_end();
        let mut values = serde_json::Deserializer::from_str(tail).into_iter::<Value>();
        let (Some(Ok(context)), Some(Ok(extra))) = (values.next(), values.next()) else { continue };
        if values.byte_offset() == tail.len() {
            return Some((body[..at].trim_end(), context, extra));
        }
    }
    None
}

fn json_entry(value: &Value) -> Option<Entry> {
    let severity = value["level_name"].as_str().or_else(|| value["level"].as_str()).map(str::to_ascii_uppercase).or_else(|| {
        Some(
            match value["level"].as_u64()? {
                100 => "DEBUG",
                200 => "INFO",
                250 => "NOTICE",
                300 => "WARNING",
                400 => "ERROR",
                500 => "CRITICAL",
                550 => "ALERT",
                600 => "EMERGENCY",
                _ => return None,
            }
            .into(),
        )
    })?;
    if !level(&severity) {
        return None;
    }
    let time = value["datetime"].as_str().or_else(|| value["datetime"]["date"].as_str()).unwrap_or("");
    let entry = Entry {
        time: display(time, 64),
        channel: display(value["channel"].as_str().unwrap_or(""), 128),
        level: severity,
        message: display(value["message"].as_str()?, MAX_MESSAGE),
        ..Entry::default()
    };
    Some(with_context(entry, Some(&value["context"]), Some(&value["extra"])))
}

/// `[datetime] channel.LEVEL: rest` → (datetime, channel, level, rest).
fn line_header(line: &str) -> Option<(&str, &str, &str, &str)> {
    let (time, rest) = line.strip_prefix('[')?.split_once("] ")?;
    if time.len() > 64 || !time.as_bytes().first()?.is_ascii_digit() {
        return None;
    }
    let (label, body) = rest.split_once(": ").or_else(|| rest.strip_suffix(':').map(|label| (label, "")))?;
    let (channel, severity) = label.rsplit_once('.')?;
    (channel.len() <= 128 && !channel.contains(' ') && level(severity)).then_some((time, channel, severity, body))
}

fn starts_record(line: &str) -> bool {
    line_header(line).is_some() || line.starts_with('{') && line.contains("\"message\"")
}

fn record(lines: &[&str]) -> Option<Entry> {
    let first = lines.first()?;
    if first.starts_with('{') {
        return crate::strict_json::parse(first, MAX_RECORD).ok().as_ref().and_then(json_entry);
    }
    let (time, channel, severity, first_body) = line_header(first)?;
    let entry = Entry { time: display(time, 64), channel: display(channel, 128), level: severity.into(), ..Entry::default() };
    let mut body = first_body.to_owned();
    for line in &lines[1..] {
        if body.len() + line.len() >= MAX_RECORD {
            break;
        }
        body.push('\n');
        body.push_str(line);
    }
    if let Some((message, context, extra)) = split_trailing_json(&body) {
        let entry = Entry { message: display(message, MAX_MESSAGE), ..entry };
        return Some(with_context(entry, Some(&context), Some(&extra)));
    }
    // Multi-line context (stack traces on): its JSON strings hold raw line breaks, so read it as text.
    let message = first_body.find(" {\"").map_or(first_body, |at| &first_body[..at]);
    let mut entry = Entry { message: display(message, MAX_MESSAGE), ..entry };
    exception_text(&mut entry, &body, true);
    for line in &lines[1..] {
        if let Some(frame) = frame(line) {
            push_frame(&mut entry, frame);
        }
    }
    Some(entry)
}

/// Entries of a log tail, oldest first, at most the newest [`MAX_ENTRIES`]; the total found is returned too.
/// Lines before the first record (a cut first record) are skipped.
pub(crate) fn parse(text: &str) -> (Vec<Entry>, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = (0..lines.len()).filter(|&at| starts_record(lines[at])).collect();
    let total = starts.len();
    let kept = &starts[total.saturating_sub(MAX_ENTRIES)..];
    let entries = kept
        .iter()
        .enumerate()
        .filter_map(|(at, &start)| {
            let end = kept.get(at + 1).copied().unwrap_or(lines.len());
            record(&lines[start..end.min(start + 2 * MAX_FRAMES + 8)])
        })
        .collect();
    (entries, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYMFONY_LINE: &str = r#"[2026-11-12T09:56:27.231217+00:00] request.ERROR: Uncaught PHP Exception Symfony\Component\HttpKernel\Exception\NotFoundHttpException: "No route found for "GET http://127.0.0.1:58708/"" at RouterListener.php line 187 {"exception":"[object] (Symfony\\Component\\HttpKernel\\Exception\\NotFoundHttpException(code: 0): No route found for \"GET http://127.0.0.1:58708/\" at /app/vendor/symfony/http-kernel/EventListener/RouterListener.php:187)\n[previous exception] [object] (Symfony\\Component\\Routing\\Exception\\NoConfigurationException(code: 0):  at /app/src/Controller/Home.php:19)"} []"#;

    #[test]
    fn symfony_line_format_gives_message_context_and_each_exception_origin() {
        let (entries, total) = parse(&format!("partial tail of a cut record\n{SYMFONY_LINE}\n[2026-11-12T10:00:00+00:00] app.INFO: Saved {{\"id\":42,\"password\":\"hunter2\"}} {{\"uid\":\"abc\"}}\n"));
        assert_eq!(total, 2);
        let error = &entries[0];
        assert_eq!((error.level.as_str(), error.channel.as_str()), ("ERROR", "request"));
        assert!(error.message.ends_with("at RouterListener.php line 187"));
        assert_eq!(error.exceptions[0].class, r"Symfony\Component\HttpKernel\Exception\NotFoundHttpException");
        assert_eq!(error.exceptions[1].class, r"Symfony\Component\Routing\Exception\NoConfigurationException");
        assert_eq!(error.frames.iter().map(|f| (f.path.as_str(), f.line)).collect::<Vec<_>>(), [("/app/vendor/symfony/http-kernel/EventListener/RouterListener.php", 187), ("/app/src/Controller/Home.php", 19)]);
        assert_eq!(error.context, "");
        let info = &entries[1];
        assert_eq!(info.message, "Saved");
        assert_eq!(info.context, r#"{"id":42,"password":"[redacted]"}"#);
        assert_eq!(info.extra, r#"{"uid":"abc"}"#);
    }

    #[test]
    fn message_ending_in_json_keeps_it_and_empty_context_is_blank() {
        let (entries, _) = parse(r#"[2026-11-12 10:00:00] app.DEBUG: payload {"a":1} [] []"#);
        assert_eq!(entries[0].message, r#"payload {"a":1}"#);
        assert_eq!((entries[0].context.as_str(), entries[0].extra.as_str()), ("", ""));
    }

    #[test]
    fn stack_traces_in_the_line_format_span_lines() {
        let text = "[2026-11-12 10:00:00] app.CRITICAL: Boom {\"exception\":\"[object] (RuntimeException(code: 0): Boom at /app/src/A.php:12)\n[stacktrace]\n#0 /app/src/B.php(30): App\\B->run('PRIVATE_ARG')\n#1 /app/vendor/c.php(4): call()\n\"} []\n[2026-11-12 10:00:01] app.INFO: next [] []";
        let (entries, total) = parse(text);
        assert_eq!(total, 2);
        assert_eq!(entries[0].message, "Boom");
        assert_eq!(entries[0].exceptions[0].class, "RuntimeException");
        assert_eq!(entries[0].frames.iter().map(|f| f.line).collect::<Vec<_>>(), [12, 30, 4]);
        assert_eq!(entries[0].frames[1].call, r"App\B->run(…)");
    }

    #[test]
    fn json_lines_with_structured_exceptions_and_masked_context() {
        let line = json!({"datetime":"2026-11-12T10:00:00+00:00","channel":"messenger","level":400,"message":"Handler failed","context":{"exception":{"class":"LogicException","message":"bad","code":0,"file":"/app/src/H.php:8","trace":["/app/src/W.php:3"],"previous":{"class":"PDOException","message":"gone","file":"C:\\www\\src\\D.php:5"}},"Authorization":"Bearer x","dsn":"mysql://root:pw@db/app"},"extra":{}}).to_string();
        let (entries, _) = parse(&line);
        let entry = &entries[0];
        assert_eq!(entry.level, "ERROR");
        assert_eq!(entry.exceptions.iter().map(|e| e.class.as_str()).collect::<Vec<_>>(), ["LogicException", "PDOException"]);
        assert_eq!(entry.frames.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["/app/src/H.php", "/app/src/W.php", "C:/www/src/D.php"]);
        assert_eq!(entry.context, r#"{"Authorization":"[redacted]","dsn":"mysql://[redacted]@db/app"}"#);
    }

    #[test]
    fn keeps_the_newest_entries_within_bounds() {
        let text: String = (0..MAX_ENTRIES + 20).map(|i| format!("[2026-11-12 10:00:00] app.INFO: line {i} [] []\n")).collect();
        let (entries, total) = parse(&text);
        assert_eq!((entries.len(), total), (MAX_ENTRIES, MAX_ENTRIES + 20));
        assert_eq!(entries[0].message, "line 20");
        assert!(frame("#1 /app/a.php(0): run()").is_none());
        assert!(frame("relative.php:3").is_none());
    }

    #[test]
    fn display_redacts_assignments_and_url_credentials() {
        let shown = display("é password='private value' token=abc https://user:pw@host/x Authorization: Bearer zzz", 4096);
        for secret in ["private value", "abc", "pw@", "zzz"] {
            assert!(!shown.contains(secret), "{shown}");
        }
        assert!(shown.starts_with("é password=[redacted]"));
        assert_eq!(display("tokenizer and token: x", 4096), "tokenizer and token: [redacted]");
    }
}
