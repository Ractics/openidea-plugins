//! Event listeners and subscribers declared in project sources: `#[AsEventListener]`,
//! `getSubscribedEvents()` return arrays, `kernel.event_listener` / `kernel.event_subscriber`
//! service tags and Doctrine's `#[AsDoctrineListener]` / `#[AsEntityListener]`. Text-based and
//! bounded like the other parsers; `vendor/` listeners are not known.
use std::ops::Range;

use openidea_plugin::Position;

use crate::index::{self, Entry, Lines};
use crate::php_checks::{arguments, named, trimmed};
use crate::php_text::{self, ClassModel, Target};

/// Where listeners usually live; any other file is read only when a service tag names its class.
pub const GLOBS: [&str; 4] = ["src/**/*Listener.php", "src/**/*Subscriber.php", "src/**/EventListener/**/*.php", "src/**/EventSubscriber/**/*.php"];

/// Whether a project-relative file matches [`GLOBS`].
pub fn candidate(relative: &str) -> bool {
    let Some(rest) = relative.strip_prefix("src/").and_then(|rest| rest.strip_suffix(".php")) else { return false };
    rest.ends_with("Listener") || rest.ends_with("Subscriber") || rest.split('/').rev().skip(1).any(|directory| matches!(directory, "EventListener" | "EventSubscriber"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Attribute,
    Subscriber,
    ServiceTag,
    Doctrine,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Attribute => "Attribute",
            Kind::Subscriber => "Subscriber",
            Kind::ServiceTag => "Service tag",
            Kind::Doctrine => "Doctrine",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    /// `kernel.request`, or an event class FQCN.
    pub event: String,
    pub class: String,
    pub method: String,
    pub priority: i64,
    pub kind: Kind,
    /// The file the fact was read from; saving it reads it again.
    pub source: String,
    /// Where it opens: the method, else the class, else the tag.
    pub target: Entry,
}

impl Listener {
    /// Row identity in the Events tab.
    pub fn id(&self) -> String {
        format!("{}|{}::{}|{}", self.event, self.class, self.method, self.kind.label())
    }
}

/// Event order within the tab: by event, highest priority first; duplicates dropped.
pub fn sort(listeners: &mut Vec<Listener>) {
    listeners.sort_by(|a, b| (&a.event, -a.priority, &a.class, &a.method).cmp(&(&b.event, -b.priority, &b.class, &b.method)));
    let mut seen = std::collections::BTreeSet::new();
    listeners.retain(|listener| seen.insert(listener.id()));
}

fn quoted(value: &str) -> Option<String> {
    let quote = value.chars().next().filter(|quote| matches!(quote, '\'' | '"'))?;
    value[1..].strip_suffix(quote).filter(|inner| !inner.contains(quote)).map(str::to_owned)
}

/// The event an argument names: a string, `Foo::class` (kept as the FQCN), or a constant of
/// Symfony's `KernelEvents`, `ConsoleEvents`, `FormEvents`, `SecurityEvents`, `WorkflowEvents`
/// (`KernelEvents::REQUEST` is `kernel.request`) or Doctrine's `Events` (`Events::prePersist`).
fn event_name(value: &str, resolve: &dyn Fn(&str) -> String) -> Option<String> {
    let value = value.trim();
    if let Some(name) = quoted(value) {
        return Some(name);
    }
    let (class, constant) = value.split_once("::")?;
    let (class, constant) = (class.trim(), constant.trim());
    if constant == "class" {
        return Some(resolve(class).trim_start_matches('\\').to_owned());
    }
    let prefix = match php_text::short_name(class) {
        "KernelEvents" => "kernel",
        "ConsoleEvents" => "console",
        "FormEvents" => "form",
        "SecurityEvents" => "security",
        "WorkflowEvents" => "workflow",
        "Events" => return Some(constant.to_owned()),
        _ => return Some(value.to_owned()),
    };
    Some(format!("{prefix}.{}", constant.to_ascii_lowercase()))
}

fn priority(value: Option<&str>) -> i64 {
    value.and_then(|value| value.trim().parse().ok()).unwrap_or(0)
}

/// Named and positional arguments of an attribute, by parameter name.
fn attribute_arguments<'a>(text: &'a str, masked: &str, inside: &Range<usize>, positional: &[&str]) -> Vec<(String, &'a str)> {
    if inside.start == 0 || masked.as_bytes().get(inside.start - 1) != Some(&b'(') {
        return Vec::new();
    }
    arguments(masked, inside.start - 1)
        .into_iter()
        .enumerate()
        .filter_map(|(at, argument)| match named(masked, argument.clone()) {
            Some((name, value)) => Some((masked[name].to_owned(), text[value].trim())),
            None => positional.get(at).map(|name| ((*name).to_owned(), text[argument].trim())),
        })
        .collect()
}

fn argument<'a>(arguments: &[(String, &'a str)], name: &str) -> Option<&'a str> {
    arguments.iter().find(|(key, _)| key == name).map(|(_, value)| *value)
}

/// The class of the first parameter of `method`: the event when none is named.
fn parameter_event(model: &ClassModel, method: &str) -> Option<String> {
    let type_name = model.method(method)?.params.first()?.type_name.clone()?;
    (!php_text::BUILT_IN_TYPES.contains(&type_name.as_str())).then(|| type_name.trim_start_matches('\\').to_owned())
}

/// Symfony's default listener method: `on` + the camel-cased event (`onKernelRequest`), else `__invoke`.
fn listener_method(model: Option<&ClassModel>, event: &str) -> String {
    let camel: String = event
        .split(|c: char| !c.is_ascii_alphanumeric())
        .flat_map(|part| {
            let mut chars = part.chars();
            chars.next().map(|first| first.to_ascii_uppercase()).into_iter().chain(chars)
        })
        .collect();
    let camel = format!("on{camel}");
    match model {
        Some(model) if model.method(&camel).is_none() && model.method("__invoke").is_some() => "__invoke".into(),
        _ => camel,
    }
}

/// Doctrine calls the method named after the event, else `__invoke`.
fn doctrine_method(model: &ClassModel, event: &str) -> String {
    if model.method(event).is_none() && model.method("__invoke").is_some() {
        return "__invoke".into();
    }
    event.to_owned()
}

struct Class<'a> {
    path: &'a str,
    model: ClassModel,
    lines: Lines<'a>,
}

impl Class<'_> {
    fn listener(&self, event: String, method: String, priority: i64, kind: Kind, source: &str) -> Listener {
        let at = self.model.method(&method).map_or(self.model.name_range.start, |found| found.name_range.start);
        Listener {
            event,
            class: self.model.fqcn.clone(),
            method,
            priority,
            kind,
            source: source.to_owned(),
            target: Entry { detail: None, path: self.path.to_owned(), at: self.lines.position(at) },
        }
    }
}

/// `[method, priority]`.
fn method_and_priority(text: &str, masked: &str, open: usize) -> Option<(String, i64)> {
    let items = arguments(masked, open);
    let method = quoted(text[items.first()?.clone()].trim())?;
    Some((method, priority(items.get(1).map(|item| &text[item.clone()]))))
}

/// The `=>` of an array entry, outside nested brackets.
fn arrow(masked: &str, entry: &Range<usize>) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    for at in entry.clone() {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'=' if depth == 0 && bytes.get(at + 1) == Some(&b'>') => return Some(at),
            _ => {}
        }
    }
    None
}

/// The `return [...]` of `getSubscribedEvents()`: `'event' => 'method'`, `=> ['method', priority]`
/// and `=> [['method', priority], ...]`.
fn subscribed(text: &str, masked: &str, body: &Range<usize>, resolve: &dyn Fn(&str) -> String) -> Vec<(String, String, i64)> {
    let Some(found) = masked[body.clone()].find("return") else { return Vec::new() };
    let open = php_text::skip_whitespace(masked, body.start + found + "return".len());
    if masked.as_bytes().get(open) != Some(&b'[') {
        return Vec::new();
    }
    let mut events = Vec::new();
    for entry in arguments(masked, open) {
        let Some(arrow) = arrow(masked, &entry) else { continue };
        let Some(event) = event_name(&text[entry.start..arrow], resolve) else { continue };
        let value = trimmed(masked, arrow + 2..entry.end);
        match masked.as_bytes().get(value.start) {
            Some(b'\'' | b'"') => events.extend(quoted(text[value].trim()).map(|method| (event, method, 0))),
            Some(b'[') => {
                let items = arguments(masked, value.start);
                let nested = items.first().is_some_and(|item| masked[item.clone()].trim_start().starts_with('['));
                if !nested {
                    events.extend(method_and_priority(text, masked, value.start).map(|(method, priority)| (event, method, priority)));
                    continue;
                }
                for item in items {
                    let item = trimmed(masked, item);
                    if let Some((method, priority)) = method_and_priority(text, masked, item.start) {
                        events.push((event.clone(), method, priority));
                    }
                }
            }
            _ => {}
        }
    }
    events
}

/// Listeners a PHP class declares itself: attributes and `getSubscribedEvents()`.
pub fn php(path: &str, text: &str) -> Vec<Listener> {
    let markers = ["AsEventListener", "getSubscribedEvents", "AsDoctrineListener", "AsEntityListener"];
    if !markers.iter().any(|marker| text.contains(marker)) {
        return Vec::new();
    }
    let Some(model) = php_text::class_model(text) else { return Vec::new() };
    let class = Class { path, model, lines: Lines::new(text) };
    let masked = php_text::mask(text);
    let resolve = php_text::resolver(text);
    let mut found = Vec::new();

    for attribute in php_text::attributes(text) {
        match (php_text::short_name(&attribute.name), &attribute.target) {
            ("AsEventListener", target @ (Target::Class { .. } | Target::Method { .. })) => {
                let arguments = attribute_arguments(text, &masked, &attribute.arguments, &["event", "method", "priority", "dispatcher"]);
                let named_method = match target {
                    Target::Method { name, .. } => Some(name.clone()),
                    _ => argument(&arguments, "method").and_then(quoted),
                };
                let event = argument(&arguments, "event")
                    .and_then(|event| event_name(event, &resolve))
                    .or_else(|| parameter_event(&class.model, named_method.as_deref().unwrap_or("__invoke")));
                let Some(event) = event else { continue };
                let method = named_method.unwrap_or_else(|| listener_method(Some(&class.model), &event));
                found.push(class.listener(event, method, priority(argument(&arguments, "priority")), Kind::Attribute, path));
            }
            (name @ ("AsDoctrineListener" | "AsEntityListener"), Target::Class { .. }) => {
                let positional: &[&str] = if name == "AsDoctrineListener" { &["event", "priority", "connection"] } else { &["event", "method", "lazy", "entityManager", "entity", "priority"] };
                let arguments = attribute_arguments(text, &masked, &attribute.arguments, positional);
                let Some(event) = argument(&arguments, "event").and_then(|event| event_name(event, &resolve)) else { continue };
                let method = argument(&arguments, "method").and_then(quoted).unwrap_or_else(|| doctrine_method(&class.model, &event));
                found.push(class.listener(event, method, priority(argument(&arguments, "priority")), Kind::Doctrine, path));
            }
            _ => {}
        }
    }

    if let Some(body) = class.model.method("getSubscribedEvents").and_then(|method| method.body.clone()) {
        for (event, method, priority) in subscribed(text, &masked, &body, &resolve) {
            found.push(class.listener(event, method, priority, Kind::Subscriber, path));
        }
    }
    found
}

/// A `kernel.event_listener` or `kernel.event_subscriber` tag of a service file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceTag {
    pub class: String,
    pub subscriber: bool,
    pub event: Option<String>,
    pub method: Option<String>,
    pub priority: i64,
    pub path: String,
    pub at: Position,
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches(['\'', '"']).trim_start_matches('\\').to_owned()
}

fn tag_kind(name: &str) -> Option<bool> {
    match name {
        "kernel.event_listener" => Some(false),
        "kernel.event_subscriber" => Some(true),
        _ => None,
    }
}

/// The `key: value` pairs of the tag whose name is at `at`: a `{ … }` on its line, or the
/// block mapping of its `- ` item.
fn yaml_tag_attributes(text: &str, at: usize) -> Vec<(String, String)> {
    let pair = |part: &str| part.split_once(':').map(|(key, value)| (key.trim().to_owned(), unquote(index::without_comment(value))));
    let line_start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let line_end = text[at..].find('\n').map_or(text.len(), |found| at + found);
    let (before, after) = (&text[line_start..at], &text[at..line_end]);
    if let (Some(open), Some(close)) = (before.rfind('{'), after.find('}')) {
        return index::split_flow(&text[line_start + open + 1..at + close], 0).into_iter().filter_map(|(_, part)| pair(part)).collect();
    }

    let lines: Vec<&str> = text.lines().collect();
    let number = text[..at].matches('\n').count();
    let content = |line: &str| {
        let trimmed = line.trim_start();
        (line.len() - trimmed.len(), trimmed.to_owned())
    };
    let (indent, trimmed) = content(lines[number]);
    let item = trimmed.starts_with("- ");
    if !item && !trimmed.starts_with("name") {
        return Vec::new();
    }
    let column = if item { indent + 2 } else { indent };
    let sibling = |line: &str| {
        let (indent, trimmed) = content(line);
        indent == column && !trimmed.is_empty() && !trimmed.starts_with('-') && !trimmed.starts_with('#')
    };
    let mut first = number;
    while !item && first > 0 {
        let (indent, trimmed) = content(lines[first - 1]);
        if sibling(lines[first - 1]) {
            first -= 1;
            continue;
        }
        if indent + 2 == column && trimmed.starts_with("- ") {
            first -= 1;
        }
        break;
    }
    let mut last = number;
    while lines.get(last + 1).is_some_and(|line| sibling(line)) {
        last += 1;
    }
    lines[first..=last].iter().filter_map(|line| pair(line.trim_start().trim_start_matches("- "))).collect()
}

fn service_tag(class: String, subscriber: bool, attribute: &dyn Fn(&str) -> Option<String>, path: &str, at: Position) -> ServiceTag {
    ServiceTag {
        class,
        subscriber,
        event: attribute("event").filter(|event| !event.is_empty()),
        method: attribute("method").filter(|method| !method.is_empty()),
        priority: priority(attribute("priority").as_deref()),
        path: path.to_owned(),
        at,
    }
}

/// Event tags of `services:` in a YAML service file; services without a class are skipped.
pub fn yaml_tags(path: &str, text: &str) -> Vec<ServiceTag> {
    let keys = index::yaml_keys(text);
    let lines = Lines::new(text);
    let mut tags = Vec::new();
    for (start, end) in index::yaml_tag_slots(text) {
        let Some(subscriber) = tag_kind(text[start..end].trim()) else { continue };
        let Some(service) = keys.iter().filter(|key| key.parents == ["services"] && key.offset < start).last() else { continue };
        let class = keys
            .iter()
            .find(|key| key.key == "class" && key.parents.len() == 2 && key.parents[0] == "services" && key.parents[1] == service.key)
            .map_or_else(|| unquote(&service.key), |key| unquote(&key.value));
        if !class.contains('\\') {
            continue;
        }
        let attributes = yaml_tag_attributes(text, start);
        let attribute = |name: &str| attributes.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
        tags.push(service_tag(class, subscriber, &attribute, path, lines.position(start)));
    }
    tags
}

/// Event `<tag>` elements of an XML service file, with their `<service>`'s class.
pub fn xml_tags(path: &str, text: &str) -> Vec<ServiceTag> {
    let visible = index::blank_xml_comments(text);
    let services = index::xml_tags(&visible, "service");
    let lines = Lines::new(text);
    let mut tags = Vec::new();
    for (tag, at) in index::xml_tags(&visible, "tag") {
        let Some(subscriber) = index::xml_attribute(tag, "name").and_then(|(name, _)| tag_kind(name)) else { continue };
        let Some((service, _)) = services.iter().filter(|(_, start)| *start < at).last() else { continue };
        let class = index::xml_attribute(service, "class").or_else(|| index::xml_attribute(service, "id")).map(|(class, _)| unquote(class)).unwrap_or_default();
        if !class.contains('\\') {
            continue;
        }
        let attribute = |name: &str| index::xml_attribute(tag, name).map(|(value, _)| value.to_owned());
        tags.push(service_tag(class, subscriber, &attribute, path, lines.position(at)));
    }
    tags
}

/// The listeners a service tag registers, read from its class file when the project has it.
pub fn tag_listeners(tag: &ServiceTag, class_file: Option<(&str, &str)>) -> Vec<Listener> {
    if tag.subscriber {
        let Some((path, text)) = class_file else { return Vec::new() };
        return php(path, text)
            .into_iter()
            .filter(|listener| listener.kind == Kind::Subscriber)
            .map(|listener| Listener { kind: Kind::ServiceTag, source: tag.path.clone(), ..listener })
            .collect();
    }

    let class = class_file.and_then(|(path, text)| Some(Class { path, model: php_text::class_model(text)?, lines: Lines::new(text) }));
    let model = class.as_ref().map(|class| &class.model);
    let event = tag.event.clone().or_else(|| parameter_event(model?, tag.method.as_deref().unwrap_or("__invoke")));
    let Some(event) = event else { return Vec::new() };
    let method = tag.method.clone().unwrap_or_else(|| listener_method(model, &event));
    let listener = match &class {
        Some(class) => class.listener(event, method, tag.priority, Kind::ServiceTag, &tag.path),
        None => Listener {
            event,
            class: tag.class.clone(),
            method,
            priority: tag.priority,
            kind: Kind::ServiceTag,
            source: tag.path.clone(),
            target: Entry { detail: None, path: tag.path.clone(), at: tag.at },
        },
    };
    vec![listener]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(listeners: &[Listener]) -> Vec<(String, String, i64, &'static str)> {
        listeners.iter().map(|listener| (listener.event.clone(), format!("{}::{}", php_text::short_name(&listener.class), listener.method), listener.priority, listener.kind.label())).collect()
    }

    fn row(event: &str, listener: &str, priority: i64, kind: &'static str) -> (String, String, i64, &'static str) {
        (event.to_owned(), listener.to_owned(), priority, kind)
    }

    #[test]
    fn attributes_on_classes_and_methods() {
        let text = "<?php\nnamespace App\\EventListener;\nuse Symfony\\Component\\EventDispatcher\\Attribute\\AsEventListener;\nuse Symfony\\Component\\HttpKernel\\KernelEvents;\nuse Symfony\\Component\\HttpKernel\\Event\\ResponseEvent;\nuse App\\Event\\OrderPlaced;\n\n#[AsEventListener(event: KernelEvents::REQUEST, priority: 20)]\n#[AsEventListener('kernel.exception', 'onError', -5)]\nfinal class Things\n{\n    public function onKernelRequest(): void {}\n    public function onError(): void {}\n    #[AsEventListener]\n    public function onResponse(ResponseEvent $event): void {}\n    #[AsEventListener(event: OrderPlaced::class)]\n    public function placed($event): void {}\n}\n";
        let found = php("/p/src/EventListener/Things.php", text);
        assert_eq!(rows(&found), [
            row("kernel.request", "Things::onKernelRequest", 20, "Attribute"),
            row("kernel.exception", "Things::onError", -5, "Attribute"),
            row("Symfony\\Component\\HttpKernel\\Event\\ResponseEvent", "Things::onResponse", 0, "Attribute"),
            row("App\\Event\\OrderPlaced", "Things::placed", 0, "Attribute"),
        ]);
        assert_eq!(found[0].target.at, Position::new(11, 20));
        assert_eq!(found[0].class, "App\\EventListener\\Things");

        let invokable = "<?php\nnamespace App;\n#[AsEventListener(event: 'app.custom')]\nclass Invokable { public function __invoke(): void {} }\n";
        assert_eq!(rows(&php("/p/src/Invokable.php", invokable)), [row("app.custom", "Invokable::__invoke", 0, "Attribute")]);
        assert!(php("/p/src/Plain.php", "<?php\nclass Plain {}\n").is_empty());
    }

    #[test]
    fn subscribers_list_strings_pairs_and_lists() {
        let text = "<?php\nnamespace App\\EventSubscriber;\nuse Symfony\\Component\\Console\\ConsoleEvents;\nuse App\\Event\\Shipped;\nclass Mail implements EventSubscriberInterface\n{\n    public static function getSubscribedEvents(): array\n    {\n        // comment => 'x'\n        return [\n            ConsoleEvents::COMMAND => 'onCommand',\n            'kernel.response' => ['onResponse', 10],\n            Shipped::class => [\n                ['first', 10],\n                ['second', -10],\n            ],\n            FormEvents::PRE_SET_DATA => ['preSet'],\n        ];\n    }\n    public function onCommand(): void {}\n}\n";
        let found = php("/p/src/EventSubscriber/Mail.php", text);
        assert_eq!(rows(&found), [
            row("console.command", "Mail::onCommand", 0, "Subscriber"),
            row("kernel.response", "Mail::onResponse", 10, "Subscriber"),
            row("App\\Event\\Shipped", "Mail::first", 10, "Subscriber"),
            row("App\\Event\\Shipped", "Mail::second", -10, "Subscriber"),
            row("form.pre_set_data", "Mail::preSet", 0, "Subscriber"),
        ]);
        assert_eq!(found[0].target.at.line, 19);
        assert_eq!(found[1].target.at.line, 4);
    }

    #[test]
    fn doctrine_listeners_use_the_event_as_method() {
        let text = "<?php\nnamespace App\\Doctrine;\nuse Doctrine\\ORM\\Events;\n#[AsDoctrineListener(event: Events::postPersist, priority: 500)]\n#[AsEntityListener(event: Events::preUpdate, method: 'touch', entity: User::class)]\n#[AsEntityListener(Events::prePersist)]\nclass UserListener\n{\n    public function postPersist(): void {}\n    public function touch(): void {}\n    public function __invoke(): void {}\n}\n";
        assert_eq!(rows(&php("/p/src/Doctrine/UserListener.php", text)), [
            row("postPersist", "UserListener::postPersist", 500, "Doctrine"),
            row("preUpdate", "UserListener::touch", 0, "Doctrine"),
            row("prePersist", "UserListener::__invoke", 0, "Doctrine"),
        ]);
    }

    #[test]
    fn yaml_and_xml_service_tags() {
        let yaml = "services:\n    _defaults:\n        autowire: true\n    App\\Listener\\Exceptions:\n        tags:\n            - { name: kernel.event_listener, event: kernel.exception, priority: 10 }\n            - name: kernel.event_listener\n              event: 'kernel.request'\n              method: early\n    app.mail:\n        class: App\\Mail\\Subscriber\n        tags: [kernel.event_subscriber]\n    app.nameless:\n        tags: [{ name: kernel.event_listener, event: x }]\n";
        let tags = yaml_tags("/p/config/services.yaml", yaml);
        let summary: Vec<_> = tags.iter().map(|tag| (tag.class.as_str(), tag.subscriber, tag.event.as_deref(), tag.method.as_deref(), tag.priority, tag.at.line)).collect();
        assert_eq!(summary, [
            ("App\\Listener\\Exceptions", false, Some("kernel.exception"), None, 10, 5),
            ("App\\Listener\\Exceptions", false, Some("kernel.request"), Some("early"), 0, 6),
            ("App\\Mail\\Subscriber", true, None, None, 0, 11),
        ]);

        let xml = "<services>\n  <!-- <service id=\"x\" class=\"App\\Hidden\"><tag name=\"kernel.event_listener\" event=\"a\"/></service> -->\n  <service id=\"app.x\" class=\"App\\X\">\n    <tag name=\"kernel.event_listener\" event=\"kernel.view\" method=\"onView\" priority=\"-3\"/>\n  </service>\n</services>\n";
        let tags = xml_tags("/p/config/services.xml", xml);
        assert_eq!(tags.len(), 1);
        assert_eq!((tags[0].class.as_str(), tags[0].event.as_deref(), tags[0].method.as_deref(), tags[0].priority, tags[0].at.line), ("App\\X", Some("kernel.view"), Some("onView"), -3, 3));

        let class = "<?php\nnamespace App\\Listener;\nclass Exceptions\n{\n    public function onKernelException(): void {}\n}\n";
        let listeners = tag_listeners(&yaml_tags("/p/config/services.yaml", yaml)[0], Some(("/p/src/Listener/Exceptions.php", class)));
        assert_eq!(rows(&listeners), [row("kernel.exception", "Exceptions::onKernelException", 10, "Service tag")]);
        assert_eq!((listeners[0].target.path.as_str(), listeners[0].target.at.line, listeners[0].source.as_str()), ("/p/src/Listener/Exceptions.php", 4, "/p/config/services.yaml"));
        let unread = tag_listeners(&tags[0], None);
        assert_eq!((unread[0].target.path.as_str(), unread[0].method.as_str()), ("/p/config/services.xml", "onView"));
    }

    #[test]
    fn sorting_groups_events_by_priority_and_drops_duplicates() {
        let make = |event: &str, method: &str, priority| Listener {
            event: event.into(), class: "A".into(), method: method.into(), priority, kind: Kind::Attribute, source: String::new(),
            target: Entry { detail: None, path: String::new(), at: Position::new(0, 0) },
        };
        let mut list = vec![make("b", "x", 0), make("a", "low", -1), make("a", "high", 5), make("a", "high", 5)];
        sort(&mut list);
        assert_eq!(list.iter().map(|l| l.method.as_str()).collect::<Vec<_>>(), ["high", "low", "x"]);
        assert!(candidate("src/EventListener/Foo.php") && candidate("src/Mail/OrderSubscriber.php") && candidate("src/Shop/EventSubscriber/Deep/X.php"));
        assert!(!candidate("src/Service/Mailer.php") && !candidate("templates/Listener.php"));
    }
}
