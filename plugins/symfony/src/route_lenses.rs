//! Code lenses above routed controller methods and YAML route definitions: `GET|POST /path` (opens
//! the route), Copy URL, Copy as cURL, Create HTTP Request and Send Request. Each lens names its
//! route; the command reads that route again from the current source of its file before acting.
use crate::index::Index;
use crate::php_text::{self, Visibility, short_name};
use crate::route_defs::{self, RouteDef};
use openidea_plugin::language_features::CodeLens;
use openidea_plugin::providers::Document;
use openidea_plugin::{Error, Invocation, Location, Position, Range, host, text};
use serde_json::json;
use ui_kit::routes::{Handler, Route};

const MAX_SOURCE: usize = 256 * 1024;
const MAX_LENSES: usize = 512;
const ACTIONS: [(&str, &str); 4] =
    [("route-copy-url", "Copy URL"), ("route-copy-curl", "Copy as cURL"), ("route-create-http", "Create HTTP Request"), ("route-send-request", "Send Request")];

/// The argument of a lens command: everything that names the route, not where it sits in the file.
fn target(def: &RouteDef) -> String {
    json!({"file": def.file, "name": def.name, "path": def.path, "methods": def.methods, "class": def.class, "member": def.member}).to_string()
}

/// Whether a command argument came from a lens rather than from a Routes row.
pub(crate) fn is_target(arg: &str) -> bool {
    arg.starts_with('{')
}

fn defs_in(path: &str, source: &str) -> Vec<RouteDef> {
    if path.ends_with(".php") {
        return route_defs::php_route_defs(path, source);
    }
    if path.ends_with(".yaml") || path.ends_with(".yml") {
        return route_defs::yaml_route_defs(path, source);
    }
    Vec::new()
}

/// Lens anchors: each public controller method with its routes (attributes read from the open text,
/// other files from the index), or each route of an indexed YAML routing file.
fn anchors(index: &Index, doc: &Document) -> Vec<(Position, RouteDef)> {
    if doc.text.len() > MAX_SOURCE {
        return Vec::new();
    }
    if doc.language == "yaml" {
        if !index.route_defs.iter().any(|def| def.file == doc.path) {
            return Vec::new();
        }
        return route_defs::yaml_route_defs(&doc.path, &doc.text).into_iter().map(|def| (def.at, def)).collect();
    }
    if doc.language != "php" {
        return Vec::new();
    }
    let Some(model) = php_text::class_model(&doc.text) else { return Vec::new() };
    let class = short_name(&model.fqcn);
    let here = route_defs::php_route_defs(&doc.path, &doc.text);
    let elsewhere = index.route_defs.iter().filter(|def| def.file != doc.path);
    let defs: Vec<&RouteDef> = here.iter().chain(elsewhere).filter(|def| def.class == class).collect();
    let mut found = Vec::new();
    for method in &model.methods {
        if !matches!(method.visibility, Visibility::Public) || method.is_static || method.name == "__construct" {
            continue;
        }
        let at = text::position(&doc.text, method.name_range.start);
        found.extend(defs.iter().filter(|def| def.member.as_deref() == Some(method.name.as_str())).map(|def| (at, (*def).clone())));
    }
    found
}

pub(crate) fn lenses(index: &Index, doc: &Document) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    for (at, def) in anchors(index, doc) {
        let range = Range::new(at, at);
        let argument = Some(target(&def));
        let methods = if def.methods.is_empty() { "ANY".to_owned() } else { def.methods.join("|") };
        lenses.push(CodeLens { range, title: format!("{methods} {}", def.path), command: "go-to-route".into(), argument: argument.clone() });
        for (command, title) in ACTIONS {
            lenses.push(CodeLens { range, title: title.into(), command: command.into(), argument: argument.clone() });
        }
        if lenses.len() >= MAX_LENSES {
            break;
        }
    }
    lenses
}

/// The lens's route as it is in its file now, in this project. A route edited since the lens was
/// shown is refused rather than guessed.
pub(crate) fn resolve(ctx: &Invocation, raw: &str) -> Result<RouteDef, Error> {
    let changed = || Error::Invalid("This route changed since the lens was shown. Use the updated lens.".into());
    let value: serde_json::Value = serde_json::from_str(raw).ok().filter(|_| raw.len() <= 8192).ok_or_else(changed)?;
    let file = value["file"].as_str().ok_or_else(changed)?;
    let project = ctx.project().ok_or("Open a project first.")?;
    let root = project.root()?;
    let owned = crate::under(file, &root) && host::project_of(file).and_then(|owner| owner.root().ok()).as_deref() == Some(root.as_str());
    if !owned || !(file.ends_with(".php") || file.ends_with(".yaml") || file.ends_with(".yml")) {
        return Err(changed());
    }
    let source = project.read_file(file).map_err(|_| changed())?;
    if source.len() > MAX_SOURCE {
        return Err(changed());
    }
    defs_in(file, &source).into_iter().find(|def| target(def) == raw).ok_or_else(changed)
}

/// The route for curl, HTTP files and the Request tab: methods joined with `|`, `ANY` when unrestricted.
pub(crate) fn route_of(def: &RouteDef) -> Route {
    let width = def.path.encode_utf16().count() as u32;
    Route {
        method: if def.methods.is_empty() { "ANY".into() } else { def.methods.join("|") },
        path: def.path.clone(),
        handler: Handler { class: if def.class.is_empty() { "Closure".into() } else { def.class.clone() }, member: def.member.clone() },
        name: def.name.clone(),
        location: Some(Location { path: def.file.clone(), range: Range::new(def.at, Position::new(def.at.line, def.at.character + width)) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, doc};
    use openidea_plugin::language_features::LanguageFeatures;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::Plugin;

    const CONTROLLER: &str = "<?php\nnamespace App\\Controller;\nclass BlogController\n{\n    #[Route('/blog', name: 'blog_index')]\n    public function index()\n    {\n        return $this->render('blog/index.html.twig', ['posts' => [], 'title' => 'x']);\n    }\n\n    public function list() {}\n\n    private function helper() {}\n}\n";
    const ROUTES: &str = "blog_list:\n    path: /blog/list/{page<\\d+>?1}\n    controller: App\\Controller\\BlogController::list\n    methods: [GET, POST]\n";

    fn titles(lenses: &[CodeLens]) -> Vec<(u32, &str)> {
        lenses.iter().map(|lens| (lens.range.start.line, lens.title.as_str())).collect()
    }

    #[test]
    fn lenses_for_attribute_and_yaml_routes_and_none_when_off() {
        let host = MockHost::install().with_capabilities(&["project.read", "clipboard"]);
        host.add_file("config/routes.yaml", ROUTES);
        let mut plugin = activated(&host);
        let controller = doc("/project/src/Controller/BlogController.php", "php", CONTROLLER);
        let lenses = plugin.code_lenses(&controller);
        assert_eq!(
            titles(&lenses),
            [
                (5, "ANY /blog"),
                (5, "Copy URL"),
                (5, "Copy as cURL"),
                (5, "Create HTTP Request"),
                (5, "Send Request"),
                (10, "GET|POST /blog/list/{page<\\d+>?1}"),
                (10, "Copy URL"),
                (10, "Copy as cURL"),
                (10, "Create HTTP Request"),
                (10, "Send Request"),
            ]
        );
        assert_eq!(lenses[0].command, "go-to-route");
        assert_eq!(lenses[2].command, "route-copy-curl");

        let yaml = plugin.code_lenses(&doc("/project/config/routes.yaml", "yaml", ROUTES));
        assert_eq!(titles(&yaml)[0], (0, "GET|POST /blog/list/{page<\\d+>?1}"));
        assert_eq!(yaml.len(), 5);
        assert!(plugin.code_lenses(&doc("/project/config/services.yaml", "yaml", "blog:\n    path: /x\n")).is_empty());

        host.set_setting("routeLenses", "false");
        assert!(plugin.code_lenses(&controller).is_empty());
    }

    #[test]
    fn lens_commands_copy_the_curl_of_their_route_and_refuse_a_changed_one() {
        let host = MockHost::install().with_capabilities(&["project.read", "clipboard"]);
        host.add_file("config/routes.yaml", ROUTES);
        let mut plugin = activated(&host);
        let ctx = host.invocation();
        let lenses = plugin.code_lenses(&doc("/project/src/Controller/BlogController.php", "php", CONTROLLER));
        let curl = |at: usize| lenses[at].argument.clone();

        host.answer_next_prompt(Some(&["http://localhost:8000"]));
        plugin.run_command("route-copy-curl", &ctx, curl(2).as_deref()).unwrap();
        assert_eq!(host.clipboard().as_deref(), Some("curl -X GET 'http://localhost:8000/blog'"));

        host.answer_next_prompt(Some(&["POST"]));
        host.answer_next_prompt(Some(&["https://shop.test"]));
        plugin.run_command("route-copy-curl", &ctx, curl(7).as_deref()).unwrap();
        assert_eq!(host.clipboard().as_deref(), Some("curl -X POST 'https://shop.test/blog/list/{page<\\d+>?1}'"));

        host.answer_next_prompt(Some(&["http://localhost:8000"]));
        plugin.run_command("route-copy-url", &ctx, curl(1).as_deref()).unwrap();
        assert_eq!(host.clipboard().as_deref(), Some("http://localhost:8000/blog"));

        plugin.run_command("go-to-route", &ctx, curl(0).as_deref()).unwrap();
        assert_eq!(host.opened_locations().last().unwrap(), &("/project/src/Controller/BlogController.php".to_owned(), 4, 13));

        host.add_file("config/routes.yaml", &ROUTES.replace("/blog/list", "/blog/all"));
        let error = plugin.run_command("route-copy-curl", &ctx, curl(7).as_deref()).unwrap_err().to_string();
        assert!(error.contains("route changed"), "{error}");
        let forged = json!({"file": "/elsewhere/routes.yaml", "name": "x", "path": "/", "methods": [], "class": "", "member": null}).to_string();
        assert!(plugin.run_command("route-copy-curl", &ctx, Some(&forged)).is_err());
    }
}
