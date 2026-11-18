//! The Components and Events tabs: searchable tables over the source index. No PHP runs.
use openidea_plugin::views::{Body, Button, View};
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, host};
use serde_json::{Value, json};

use crate::index::{Entry, Index};
use crate::php_text::short_name;

pub(crate) const COMPONENTS_VIEW: &str = "symfony-components";
pub(crate) const EVENTS_VIEW: &str = "symfony-events";
pub(crate) const EVENTS_MODE: &str = "events-mode";

/// What a row of these tabs opens: the class (or anonymous template), the Stimulus controller, the listener method.
pub(crate) fn entry<'a>(index: &'a Index, group: &str, name: &str) -> Option<&'a Entry> {
    match group {
        "component" | "anonymous" => index.components.get(name),
        "stimulus" => index.stimulus.get(name),
        "event" => index.events.iter().find(|listener| listener.id() == name).map(|listener| &listener.target),
        _ => None,
    }
}

pub(crate) fn count(index: &Index, view: &str) -> Option<usize> {
    match view {
        COMPONENTS_VIEW => Some(index.component_info.len() + index.stimulus.len()),
        EVENTS_VIEW => Some(index.events.len()),
        _ => None,
    }
}

fn relative<'a>(root: &str, path: &'a str) -> &'a str {
    path.strip_prefix(root).unwrap_or(path).trim_start_matches(['/', '\\'])
}

fn resources(mut data: Value, items: Vec<Value>) -> Body {
    data["showRowIcons"] = true.into();
    data["items"] = items.into();
    Body::Other { kind: "resources".into(), data: data.to_string() }
}

fn with_partial(mut status: String, index: &Index) -> String {
    if index.partial {
        status.push_str(" · Index incomplete (discovery, read or time limit)");
    }
    status
}

/// The tab's view; `tree` groups the Events tab by event.
pub(crate) fn render(view: &str, index: &Index, ctx: &Invocation, tree: bool) -> View {
    let root = ctx.project().and_then(|project| project.root().ok()).unwrap_or_default();
    let reindex = Button::new("reindex", "Reindex").icon("refresh");
    if view == COMPONENTS_VIEW {
        let installed = ctx.project().and_then(|project| project.read_file("composer.json").ok()).is_some_and(|composer| composer.contains("\"symfony/ux-twig-component\""));
        return View::new(components(index, &root, installed)).button(reindex).status(&with_partial(
            format!("{} Twig components · {} Stimulus controllers", index.component_info.len(), index.stimulus.len()),
            index,
        ));
    }
    let events: std::collections::BTreeSet<&str> = index.events.iter().map(|listener| listener.event.as_str()).collect();
    View::new(events_table(index, &root, tree))
        .button(reindex)
        .button(Button::new(EVENTS_MODE, if tree { "Flat" } else { "Tree" }).icon(if tree { "list" } else { "hierarchy" }))
        .status(&with_partial(format!("{} listeners on {} events", index.events.len(), events.len()), index))
}

fn components(index: &Index, root: &str, installed: bool) -> Body {
    let mut items: Vec<Value> = index
        .component_info
        .iter()
        .map(|(name, info)| {
            let component = &info.component;
            let (group, kind, icon) = match (&component.class, component.live) {
                (None, _) => ("anonymous", "Anonymous", "file-html"),
                (Some(_), true) => ("component", "Live", "class"),
                (Some(_), false) => ("component", "Twig", "class"),
            };
            let file = index.components.get(name).map_or("", |entry| relative(root, &entry.path));
            let class = component.class.as_deref().map(short_name).unwrap_or_default();
            let cells = [format!("<twig:{name}>"), kind.into(), class.into(), component.template.clone(), component.props.join(", "), file.into()];
            let mut details: Vec<Value> = component.class.iter().map(|class| json!({"label":"Class","value":class})).collect();
            if !info.blocks.is_empty() {
                details.push(json!({"label":"Blocks","value":info.blocks.join(", ")}));
            }
            json!({"key":format!("{group}:{name}"),"cells":cells,"icon":icon,"details":details,"searchText":component.class.as_deref().unwrap_or_default()})
        })
        .collect();
    items.extend(index.stimulus.iter().map(|(name, entry)| {
        json!({"key":format!("stimulus:{name}"),"cells":[name,"Stimulus","","","",relative(root, &entry.path)],"icon":"file-js"})
    }));
    let empty = if installed {
        "Add a class with #[AsTwigComponent] or #[AsLiveComponent] in a Components folder of src/, a template under templates/components/, or a Stimulus controller under assets/controllers/, then click Reindex."
    } else {
        "Twig components need symfony/ux-twig-component: run composer require symfony/ux-twig-component, add a component, then click Reindex. Stimulus controllers under assets/controllers/ are listed too."
    };
    resources(
        json!({"columns":["Component","Kind","Class","Template","Props","File"],"columnStyles":["code","text","code","muted","locale","muted"],
            "columnWeights":[2.5,1,2,3,2.5,3],"searchPlaceholder":"Filter components…","facetLabels":{"1":"All kinds"},"facets":[1],
            "emptyTitle":"No components in this project","emptyText":empty,"emptyAction":"reindex"}),
        items,
    )
}

fn events_table(index: &Index, root: &str, tree: bool) -> Body {
    let items = index
        .events
        .iter()
        .map(|listener| {
            let cells = [
                listener.event.clone(),
                format!("{}::{}", short_name(&listener.class), listener.method),
                listener.priority.to_string(),
                listener.kind.label().into(),
                relative(root, &listener.source).into(),
            ];
            json!({"key":format!("event:{}", listener.id()),"cells":cells,"icon":"method","details":[{"label":"Class","value":listener.class}],"searchText":listener.class})
        })
        .collect();
    let mut data = json!({"columns":["Event","Listener","Priority","Kind","File"],"columnStyles":["code","code","locale","text","muted"],
        "columnWeights":[3,3.5,1,1.5,3],"searchPlaceholder":"Filter events or listeners…","facetLabels":{"3":"All kinds"},"facets":[3],
        "emptyTitle":"No event listeners in this project",
        "emptyText":"Add a class with #[AsEventListener], an EventSubscriberInterface subscriber or a kernel.event_listener tag in config/services.yaml, then click Reindex.",
        "emptyAction":"reindex"});
    if tree {
        data["groupBy"] = 0.into();
        data["displayColumns"] = json!([1, 2, 3, 4]);
    }
    resources(data, items)
}

/// Open Template, Open Class and Copy Tag on a Components row, or on a component picked from the palette.
pub(crate) fn component_command(id: &str, index: Option<&Index>, arg: Option<&str>) -> Result<Option<String>, Error> {
    let index = index.ok_or("No Symfony project is indexed.")?;
    let wants_class = id == "component-open-class";
    let row = arg.and_then(|key| key.split_once(':')).filter(|(group, _)| matches!(*group, "component" | "anonymous")).map(|(_, name)| name.to_owned());
    let name = match row {
        Some(name) => name,
        None => {
            let items: Vec<PickItem> = index
                .component_info
                .iter()
                .filter(|(_, info)| !wants_class || info.component.class.is_some())
                .map(|(name, info)| PickItem::new(name, &format!("<twig:{name}>")).description(&info.component.template))
                .collect();
            if items.is_empty() {
                return Err(if wants_class { "No component classes are indexed." } else { "No Twig components are indexed." }.into());
            }
            match host::pick("Choose a component", &items)? {
                Some(name) => name,
                None => return Ok(None),
            }
        }
    };
    let Some(info) = index.component_info.get(&name) else {
        return Err("This component is no longer listed. Click Reindex in the Symfony tool window.".into());
    };
    let open = |entry: &Entry| host::open_location(&entry.path, entry.at.line, entry.at.character);
    match id {
        "component-copy-tag" => {
            let tag = format!("<twig:{name} />");
            match host::copy_to_clipboard(&tag) {
                Ok(()) => host::notify(NotificationLevel::Info, crate::SOURCE, &format!("Copied {tag}.")),
                Err(Error::Denied(_)) => host::notify(NotificationLevel::Warning, crate::SOURCE, &ui_kit::denied::message("Copy Tag", "clipboard", "Symfony")),
                Err(error) => return Err(error),
            }
        }
        "component-open-class" => {
            let entry = index.components.get(&name).filter(|_| info.component.class.is_some());
            open(entry.ok_or(format!("{name} is an anonymous component: it has no class. Use Open Template."))?)?;
        }
        _ => {
            let template = &info.component.template;
            let entry = index.templates.get(template).ok_or(format!("{template} of {name} isn't in the project. Create it, then click Reindex."))?;
            open(entry)?;
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::activated;
    use openidea_plugin::Plugin;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::views::{ViewEvent, Views};

    fn body_json(view: &View) -> Value {
        let Body::Other { kind, data } = &view.body else { panic!("expected a resources body") };
        assert_eq!(kind, "resources");
        serde_json::from_str(data).unwrap()
    }

    fn project(host: &MockHost) -> crate::Main {
        let mut plugin = activated(host);
        host.add_file("composer.json", r#"{"require": {"symfony/framework-bundle": "^7.0", "symfony/ux-twig-component": "^2.0"}, "autoload": {"psr-4": {"App\\": "src/"}}}"#);
        host.add_file("src/Twig/Components/Alert.php", "<?php\nnamespace App\\Twig\\Components;\n#[AsTwigComponent]\nclass Alert\n{\n    public string $type = 'info';\n}\n");
        host.add_file("src/Twig/Components/Counter.php", "<?php\nnamespace App\\Twig\\Components;\n#[AsLiveComponent]\nclass Counter\n{\n    #[LiveProp]\n    public int $count = 0;\n    #[ExposeInTemplate]\n    private string $label = '';\n}\n");
        host.add_file("templates/components/Alert.html.twig", "<div>{% block content %}{% endblock %}</div>");
        host.add_file("templates/components/Card.html.twig", "{% props title %}<div>{{ title }}</div>");
        host.add_file("assets/controllers/hello_controller.js", "export default class {}");
        host.add_file(
            "src/EventSubscriber/Locale.php",
            "<?php\nnamespace App\\EventSubscriber;\nuse Symfony\\Component\\HttpKernel\\KernelEvents;\nclass Locale implements EventSubscriberInterface\n{\n    public static function getSubscribedEvents(): array\n    {\n        return [KernelEvents::REQUEST => [['setLocale', 20]]];\n    }\n    public function setLocale(): void {}\n}\n",
        );
        host.add_file("src/EventListener/Exceptions.php", "<?php\nnamespace App\\EventListener;\n#[AsEventListener(event: 'kernel.request', priority: 50)]\nclass Exceptions\n{\n    public function onKernelRequest(): void {}\n}\n");
        host.add_file("src/Mail/Notifier.php", "<?php\nnamespace App\\Mail;\nclass Notifier\n{\n    public function onTerminate(): void {}\n}\n");
        host.add_file("config/services.yaml", "services:\n    App\\Mail\\Notifier:\n        tags:\n            - { name: kernel.event_listener, event: kernel.terminate, method: onTerminate }\n");
        plugin.index_project(&host.invocation().project().unwrap());
        plugin
    }

    #[test]
    fn components_tab_lists_twig_live_anonymous_and_stimulus() {
        let host = MockHost::install();
        let mut plugin = project(&host);
        let ctx = host.invocation();
        let view = plugin.render(COMPONENTS_VIEW, None, &ctx).unwrap();
        let data = body_json(&view);
        let rows: Vec<(String, Vec<String>)> = data["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["key"].as_str().unwrap().to_owned(), item["cells"].as_array().unwrap().iter().map(|cell| cell.as_str().unwrap().to_owned()).collect()))
            .collect();
        let row = |key: &str| rows.iter().find(|(found, _)| found == key).map(|(_, cells)| cells.clone()).unwrap_or_else(|| panic!("{key} in {rows:?}"));
        assert_eq!(row("component:Alert"), ["<twig:Alert>", "Twig", "Alert", "components/Alert.html.twig", "type", "src/Twig/Components/Alert.php"]);
        assert_eq!(row("component:Counter")[1..5], ["Live", "Counter", "components/Counter.html.twig", "count, label"]);
        assert_eq!(row("anonymous:Card")[..5], ["<twig:Card>", "Anonymous", "", "components/Card.html.twig", "title"]);
        assert_eq!(row("stimulus:hello"), ["hello", "Stimulus", "", "", "", "assets/controllers/hello_controller.js"]);
        assert_eq!(data["facets"], json!([1]));
        assert!(view.toolbar.iter().any(|button| button.id == "reindex"));
        assert_eq!(view.status.as_deref(), Some("3 Twig components · 1 Stimulus controllers"));

        plugin.handle(COMPONENTS_VIEW, ViewEvent::Activate("anonymous:Card".into()), &ctx).unwrap();
        plugin.handle(COMPONENTS_VIEW, ViewEvent::Activate("component:Alert".into()), &ctx).unwrap();
        let opened: Vec<String> = host.opened_locations().into_iter().map(|(path, ..)| path).collect();
        assert_eq!(opened, ["/project/templates/components/Card.html.twig", "/project/src/Twig/Components/Alert.php"]);

        plugin.run_command("component-open-template", &ctx, Some("component:Alert")).unwrap();
        assert_eq!(host.opened_locations().last().unwrap().0, "/project/templates/components/Alert.html.twig");
        assert!(plugin.run_command("component-open-class", &ctx, Some("anonymous:Card")).is_err());
        plugin.run_command("component-copy-tag", &ctx, Some("anonymous:Card")).unwrap();
        assert_eq!(host.clipboard().as_deref(), Some("<twig:Card />"));
    }

    #[test]
    fn components_empty_state_names_the_package() {
        let host = MockHost::install();
        let mut plugin = activated(&host);
        let view = plugin.render(COMPONENTS_VIEW, None, &host.invocation()).unwrap();
        let data = body_json(&view);
        assert!(data["items"].as_array().unwrap().is_empty());
        assert!(data["emptyText"].as_str().unwrap().contains("symfony/ux-twig-component"));
        assert_eq!(data["emptyAction"], "reindex");
    }

    #[test]
    fn events_tab_groups_listeners_by_event_with_priority_first() {
        let host = MockHost::install();
        let mut plugin = project(&host);
        let ctx = host.invocation();
        let view = plugin.render(EVENTS_VIEW, None, &ctx).unwrap();
        let data = body_json(&view);
        let cells: Vec<Vec<String>> =
            data["items"].as_array().unwrap().iter().map(|item| item["cells"].as_array().unwrap().iter().map(|cell| cell.as_str().unwrap().to_owned()).collect()).collect();
        assert_eq!(cells, [
            ["kernel.request", "Exceptions::onKernelRequest", "50", "Attribute", "src/EventListener/Exceptions.php"],
            ["kernel.request", "Locale::setLocale", "20", "Subscriber", "src/EventSubscriber/Locale.php"],
            ["kernel.terminate", "Notifier::onTerminate", "0", "Service tag", "config/services.yaml"],
        ]);
        assert_eq!((data["groupBy"].clone(), data["facets"].clone()), (json!(0), json!([3])));
        assert_eq!(view.status.as_deref(), Some("3 listeners on 2 events"));

        plugin.handle(EVENTS_VIEW, ViewEvent::Button(EVENTS_MODE.into()), &ctx).unwrap();
        let flat = plugin.render(EVENTS_VIEW, None, &ctx).unwrap();
        assert!(body_json(&flat)["groupBy"].is_null());
        assert_eq!(flat.toolbar.iter().find(|button| button.id == EVENTS_MODE).unwrap().label, "Tree");

        let key = data["items"][2]["key"].as_str().unwrap().to_owned();
        plugin.handle(EVENTS_VIEW, ViewEvent::Activate(key), &ctx).unwrap();
        let (path, line, _) = host.opened_locations().last().cloned().unwrap();
        assert_eq!((path.as_str(), line), ("/project/src/Mail/Notifier.php", 4));
    }

    #[test]
    fn saving_a_listener_updates_only_its_rows() {
        let host = MockHost::install();
        let mut plugin = project(&host);
        let project = host.invocation().project().unwrap();
        host.add_file("src/EventListener/Exceptions.php", "<?php\nnamespace App\\EventListener;\n#[AsEventListener(event: 'kernel.exception')]\nclass Exceptions\n{\n    public function onKernelException(): void {}\n}\n");
        let index = plugin.indexes.get_mut("/project").unwrap();
        assert!(index.update_file(&project, "/project", "src/EventListener/Exceptions.php"));
        let events: Vec<&str> = index.events.iter().map(|listener| listener.event.as_str()).collect();
        assert_eq!(events, ["kernel.exception", "kernel.request", "kernel.terminate"]);
    }
}
