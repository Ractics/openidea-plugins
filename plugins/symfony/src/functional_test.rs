//! **Generate a functional test for {route}…** on a routed public controller method: a
//! `WebTestCase` under the autoload-dev tests folder with one test per HTTP method of the routes,
//! or only the missing test methods added to an existing test class. Always confirmed first.
use crate::php_text::{self, ClassModel, Visibility, short_name};
use crate::route_defs::{self, RouteDef};
use crate::*;
use openidea_plugin::file_actions::{FileAction, FileOperation};

const PREFIX: &str = "functional-test:";
const MAX_SOURCE: usize = 256 << 10;

/// The routes served by the public method around `offset`, with that method's name.
fn routes_at(index: &Index, doc: &Document, offset: usize) -> Option<(ClassModel, String, Vec<RouteDef>)> {
    if doc.language != "php" || doc.text.len() > MAX_SOURCE {
        return None;
    }
    let model = php_text::class_model(&doc.text)?;
    let method = model.methods.iter().find(|method| {
        let start = intentions::line_start(&doc.text, method.name_range.start);
        let end = method.body.as_ref().map_or(method.name_range.end, |body| body.end);
        (start..=end).contains(&offset)
    })?;
    if method.visibility != Visibility::Public || method.is_static || method.name == "__construct" {
        return None;
    }
    let class = short_name(&model.fqcn);
    let routes: Vec<RouteDef> = route_defs::php_route_defs(&doc.path, &doc.text)
        .into_iter()
        .chain(index.route_defs.iter().filter(|def| def.file != doc.path).cloned())
        .filter(|def| def.class == class && def.member.as_deref() == Some(method.name.as_str()))
        .collect();
    let name = method.name.clone();
    (!routes.is_empty()).then_some((model, name, routes))
}

fn label(def: &RouteDef) -> String {
    let methods = if def.methods.is_empty() { "GET".to_owned() } else { def.methods.join("|") };
    format!("{methods} {}", def.path)
}

pub(crate) fn actions(owner: &Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let (Some(index), Some(offset)) = (owner.index_for(&doc.path), text::offset(&doc.text, selection.start)) else {
        return Vec::new();
    };
    let Some((_, _, routes)) = routes_at(index, doc, offset) else {
        return Vec::new();
    };
    vec![FileAction {
        title: format!("Generate a functional test for {}…", label(&routes[0])),
        kind: Some("quickfix".into()),
        data: Some(format!("{PREFIX}{}:{offset}", doc.version)),
        ..FileAction::default()
    }]
}

pub(crate) fn handles(action: &FileAction) -> bool {
    action.data.as_deref().is_some_and(|data| data.starts_with(PREFIX))
}

/// `{id}` → `1`, `{slug}` → `test`, `{page<\d+>?1}` → `1`, `{_locale}` → `en`.
pub(crate) fn sample_path(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = rest[open + 1..open + close].trim_start_matches('!');
        rest = &rest[open + close + 1..];
        let (head, default) = inner.split_once('?').unwrap_or((inner, ""));
        let (name, requirement) = head.split_once('<').unwrap_or((head, ""));
        let lower = name.to_ascii_lowercase();
        let numeric = requirement.contains("\\d")
            || lower == "id"
            || lower.ends_with("id")
            || ["page", "year", "month", "day", "number"].contains(&lower.as_str());
        out.push_str(match (default, name) {
            (default, _) if !default.is_empty() => default,
            (_, "_locale") => "en",
            (_, "_format") => "html",
            _ if numeric => "1",
            _ => "test",
        });
    }
    out.push_str(rest);
    out
}

/// The status a non-GET request is expected to answer, read from the action's body.
fn expected_status(masked_body: &str) -> u16 {
    let has = |needle: &str| masked_body.contains(needle);
    if has("redirect") || has("Redirect") {
        return if has("HTTP_SEE_OTHER") || has("303") { 303 } else { 302 };
    }
    if has("HTTP_CREATED") || has("201") {
        return 201;
    }
    if has("HTTP_NO_CONTENT") || has("204") {
        return 204;
    }
    200
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// `(test name, method source)` for each HTTP method of each route, skipping names in `taken`.
fn test_methods(method: &str, routes: &[RouteDef], status: u16, eol: &str, taken: &[String]) -> Vec<(String, String)> {
    let base = format!("test{}", capitalized(method.trim_start_matches('_')));
    let mut names: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for route in routes {
        let verbs = if route.methods.is_empty() { vec!["GET".to_owned()] } else { route.methods.clone() };
        for verb in verbs {
            let verb = verb.to_ascii_uppercase();
            let stem = if verb == "GET" { base.clone() } else { format!("{base}{}", capitalized(&verb.to_ascii_lowercase())) };
            let name = std::iter::once(stem.clone()).chain((2..).map(|n| format!("{stem}{n}"))).find(|name| !names.contains(name)).unwrap_or(stem);
            names.push(name.clone());
            if taken.contains(&name) {
                continue;
            }
            let path = sample_path(&route.path).replace('\\', "\\\\").replace('\'', "\\'");
            let (request, assertion) = if verb == "GET" {
                (format!("$client->request('GET', '{path}');"), "self::assertResponseIsSuccessful();".to_owned())
            } else {
                (format!("$client->request('{verb}', '{path}', []);"), format!("self::assertResponseStatusCodeSame({status});"))
            };
            let source = format!(
                "    public function {name}(): void\n    {{\n        $client = static::createClient();\n        {request}\n\n        {assertion}\n    }}\n"
            )
            .replace('\n', eol);
            out.push((name, source));
        }
    }
    out
}

/// `(namespace, folder)` of the tests from composer.json `autoload-dev`, else `App\Tests` in `tests/`.
fn tests_mapping(composer: &str) -> (String, String) {
    let fallback = ("App\\Tests".to_owned(), "tests".to_owned());
    let Ok(json) = serde_json::from_str::<serde_json::Value>(composer) else {
        return fallback;
    };
    let Some(map) = json.pointer("/autoload-dev/psr-4").and_then(|value| value.as_object()) else {
        return fallback;
    };
    let entries = map.iter().filter_map(|(namespace, folder)| Some((namespace.trim_end_matches('\\').to_owned(), folder.as_str()?.trim_end_matches('/').to_owned())));
    let valid = |(namespace, folder): &(String, String)| {
        !namespace.is_empty() && !folder.is_empty() && !folder.starts_with('/') && !folder.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    };
    let entries: Vec<_> = entries.filter(valid).collect();
    entries.iter().find(|(namespace, _)| namespace.ends_with("\\Tests") || namespace == "Tests").or(entries.first()).cloned().unwrap_or(fallback)
}

pub(crate) fn resolve(owner: &Main, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let data = action.data.as_deref().and_then(|data| data.strip_prefix(PREFIX)).ok_or(Error::Stale)?;
    let (version, offset) = data.split_once(':').ok_or(Error::Stale)?;
    if version != doc.version.to_string() {
        return Err(Error::Stale);
    }
    let offset: usize = offset.parse().map_err(|_| Error::Stale)?;
    let index = owner.index_for(&doc.path).ok_or(Error::Stale)?;
    let (model, method, routes) = routes_at(index, doc, offset).ok_or(Error::Stale)?;
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let root = project.root()?;

    let prefix = index.psr4_prefixes.iter().filter(|prefix| model.fqcn.starts_with(prefix.as_str())).max_by_key(|prefix| prefix.len());
    let relative = match prefix {
        Some(prefix) => model.fqcn[prefix.len()..].to_owned(),
        None => format!("Controller\\{}", short_name(&model.fqcn)),
    };
    let (namespace, folder) = tests_mapping(&project.read_file("composer.json").unwrap_or_default());
    let target = format!("{root}/{folder}/{}Test.php", relative.replace('\\', "/"));
    let class = format!("{}Test", short_name(&relative));
    let test_namespace = match relative.rsplit_once('\\') {
        Some((sub, _)) => format!("{namespace}\\{sub}"),
        None => namespace,
    };

    let masked = php_text::mask(&doc.text);
    let body = model.method(&method).and_then(|found| found.body.clone()).unwrap_or(0..0);
    let status = expected_status(&masked[body]);

    let existing = match project.read_file(&target) {
        Ok(text) => Some(text),
        Err(Error::NotFound(_)) => None,
        Err(error) => return Err(error),
    };

    let route = label(&routes[0]);
    let Some(existing) = existing else {
        file_actions::vacant(&project, &target, &root)?;
        let eol = intentions::eol_of(&doc.text);
        let methods: Vec<String> = test_methods(&method, &routes, status, eol, &[]).into_iter().map(|(_, source)| source).collect();
        let text = format!(
            "<?php\n\nnamespace {test_namespace};\n\nuse Symfony\\Bundle\\FrameworkBundle\\Test\\WebTestCase;\n\nfinal class {class} extends WebTestCase\n{{\n"
        )
        .replace('\n', eol)
            + &methods.join(eol)
            + "}"
            + eol;
        let preview = format!("Creates {target} and opens it. Nothing is overwritten.\nDelete the file to undo.\n\n{text}");
        if !ui_kit::prompts::confirm_exact(&format!("Create {class}?"), &preview)? {
            return Err(Error::Cancelled);
        }
        return Ok(FileAction {
            title: format!("Generate a functional test for {route}"),
            kind: Some("quickfix".into()),
            operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
            open: Some(target),
            ..FileAction::default()
        });
    };

    let test_model = php_text::class_model(&existing).ok_or("Couldn't read the class in the existing test file. Add the test by hand.")?;
    let taken: Vec<String> = test_model.methods.iter().map(|method| method.name.clone()).collect();
    let eol = intentions::eol_of(&existing);
    let added = test_methods(&method, &routes, status, eol, &taken);
    if added.is_empty() {
        return Err(format!("{} already has a test for {route}.", file_name(&target)).into());
    }
    let close = test_model.body.end;
    if existing.as_bytes().get(close) != Some(&b'}') {
        return Err("Couldn't find the end of the test class. Add the test by hand.".into());
    }
    let names: Vec<&str> = added.iter().map(|(name, _)| name.as_str()).collect();
    let insert: String = added.iter().map(|(_, source)| format!("{eol}{source}")).collect();
    let before = existing[..close].trim_end_matches([' ', '\t', '\r', '\n']).len();
    let edit = TextEdit { range: range_of(&existing, before, close), text: format!("{eol}{insert}") };
    let preview = format!("Adds {} to {target}. Other tests are kept.\nUndo in the editor.\n\n{insert}", names.join(", "));
    if !ui_kit::prompts::confirm_exact(&format!("Add {} to {class}?", names.join(", ")), &preview)? {
        return Err(Error::Cancelled);
    }
    Ok(FileAction {
        title: format!("Generate a functional test for {route}"),
        kind: Some("quickfix".into()),
        edits: vec![(target.clone(), vec![edit])],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, doc};
    use openidea_plugin::file_actions::FileActions;
    use openidea_plugin::mock::MockHost;

    const CONTROLLER: &str = "<?php\nnamespace App\\Controller\\Admin;\n\nclass PostController\n{\n    #[Route('/admin/post/{id<\\d+>}', name: 'post_show')]\n    public function show(int $id)\n    {\n        return $this->render('post/show.html.twig');\n    }\n\n    #[Route('/admin/post/{slug}/edit', name: 'post_edit', methods: ['GET', 'POST'])]\n    public function edit(string $slug)\n    {\n        return $this->redirectToRoute('post_show', [], Response::HTTP_SEE_OTHER);\n    }\n\n    public function helper() {}\n}\n";
    const PATH: &str = "/project/src/Controller/Admin/PostController.php";
    const TARGET: &str = "/project/tests/Controller/Admin/PostControllerTest.php";

    fn action_at(plugin: &mut Main, text: &str, needle: &str) -> Option<FileAction> {
        let at = text::position(text, text.find(needle).unwrap());
        plugin.file_actions(&doc(PATH, "php", text), Range::new(at, at)).into_iter().find(handles)
    }

    #[test]
    fn samples_fill_placeholders() {
        assert_eq!(sample_path("/post/{id}/{slug}/{page<\\d+>?3}/{_locale}"), "/post/1/test/3/en");
        assert_eq!(sample_path("/x/{!userId}"), "/x/1");
    }

    #[test]
    fn creates_a_web_test_case_with_one_test_per_http_method() {
        let host = MockHost::install();
        host.add_file(PATH, CONTROLLER);
        let mut plugin = activated(&host);
        assert!(action_at(&mut plugin, CONTROLLER, "function helper").is_none());

        let action = action_at(&mut plugin, CONTROLLER, "function edit").unwrap();
        assert_eq!(action.title, "Generate a functional test for GET|POST /admin/post/{slug}/edit…");
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc(PATH, "php", CONTROLLER), action).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = resolved.operations.as_slice() else { panic!("{:?}", resolved.operations) };
        assert_eq!(path, TARGET);
        assert!(text.starts_with("<?php\n\nnamespace App\\Tests\\Controller\\Admin;\n\nuse Symfony\\Bundle\\FrameworkBundle\\Test\\WebTestCase;\n\nfinal class PostControllerTest extends WebTestCase\n{\n"), "{text}");
        assert!(text.contains("    public function testEdit(): void\n    {\n        $client = static::createClient();\n        $client->request('GET', '/admin/post/test/edit');\n\n        self::assertResponseIsSuccessful();\n    }\n"), "{text}");
        assert!(text.contains("public function testEditPost(): void\n    {\n        $client = static::createClient();\n        $client->request('POST', '/admin/post/test/edit', []);\n\n        self::assertResponseStatusCodeSame(303);"), "{text}");
        assert!(text.ends_with("    }\n}\n"), "{text}");
    }

    #[test]
    fn adds_only_the_missing_test_to_an_existing_class_and_refuses_when_none_is_missing() {
        let host = MockHost::install();
        host.add_file(PATH, CONTROLLER);
        let existing = "<?php\n\nnamespace App\\Tests\\Controller\\Admin;\n\nfinal class PostControllerTest extends WebTestCase\n{\n    public function testShow(): void\n    {\n    }\n}\n";
        host.add_file("tests/Controller/Admin/PostControllerTest.php", existing);
        let mut plugin = activated(&host);

        let show = action_at(&mut plugin, CONTROLLER, "function show").unwrap();
        let error = plugin.resolve_file_action(&doc(PATH, "php", CONTROLLER), show).unwrap_err().to_string();
        assert!(error.contains("already has a test for GET /admin/post/{id<\\d+>}"), "{error}");

        let edit = action_at(&mut plugin, CONTROLLER, "redirectToRoute").unwrap();
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc(PATH, "php", CONTROLLER), edit).unwrap();
        assert!(resolved.operations.is_empty());
        let [(path, edits)] = resolved.edits.as_slice() else { panic!() };
        assert_eq!(path, TARGET);
        let mut out = existing.to_owned();
        let start = text::offset(existing, edits[0].range.start).unwrap();
        let end = text::offset(existing, edits[0].range.end).unwrap();
        out.replace_range(start..end, &edits[0].text);
        assert!(out.contains("    {\n    }\n\n    public function testEdit(): void\n"), "{out}");
        assert!(out.contains("public function testEditPost(): void"), "{out}");
        assert!(out.ends_with("    }\n}\n"), "{out}");
    }

    #[test]
    fn cancelled_review_and_changed_document_return_no_effects() {
        let host = MockHost::install();
        host.add_file(PATH, CONTROLLER);
        let mut plugin = activated(&host);
        let action = action_at(&mut plugin, CONTROLLER, "function show").unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc(PATH, "php", CONTROLLER), action.clone()), Err(Error::Cancelled)));
        let mut changed = doc(PATH, "php", CONTROLLER);
        changed.version = 2;
        assert!(matches!(plugin.resolve_file_action(&changed, action), Err(Error::Stale)));
    }
}
