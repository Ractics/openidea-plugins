//! **Inject Foo instead of the container** on `$this->container->get(…)`, `$container->get(…)` or
//! a controller's `$this->get(…)`: the service becomes a promoted `private readonly` constructor
//! parameter (or, in a controller action, **Inject Foo as action argument**) and the call becomes
//! `$this->foo` / `$foo`. The service class comes from `Foo::class`, a class-name id or the indexed
//! service definitions; when it can't be found, the action says so and changes nothing.
use crate::php_text::{self, ClassModel, Visibility, short_name};
use crate::*;
use openidea_plugin::file_actions::FileAction;

const PREFIX: &str = "container-inject:";
const MAX_SOURCE: usize = 256 << 10;
const CONTROLLER_PARENTS: [&str; 2] = ["Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController", "Symfony\\Bundle\\FrameworkBundle\\Controller\\Controller"];
const CALLS: [&str; 3] = ["$this->container->get(", "$container->get(", "$this->get("];

struct Call {
    range: std::ops::Range<usize>,
    argument: String,
}

fn is_controller(model: &ClassModel) -> bool {
    model.parent.as_deref().is_some_and(|parent| CONTROLLER_PARENTS.contains(&parent))
}

/// The container call around `offset`.
fn call_at(text: &str, masked: &str, model: &ClassModel, offset: usize) -> Option<Call> {
    CALLS.iter().find_map(|needle| {
        if *needle == "$this->get(" && !is_controller(model) {
            return None;
        }
        masked.match_indices(needle).find_map(|(start, _)| {
            let previous = masked[..start].chars().next_back();
            if previous.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '>') {
                return None;
            }
            let open = start + needle.len() - 1;
            let close = php_text::matching_close(masked, open)?;
            let argument = text[open + 1..close].trim();
            ((start..=close + 1).contains(&offset) && !argument.is_empty() && !argument.contains(',')).then(|| Call { range: start..close + 1, argument: argument.to_owned() })
        })
    })
}

/// The service class of `Foo::class`, `'App\Foo'` or a service id defined in the project.
fn service_of(index: &Index, text: &str, argument: &str) -> Option<String> {
    if let Some(class) = argument.strip_suffix("::class") {
        let class = class.trim();
        let valid = !class.is_empty() && class.split('\\').filter(|part| !part.is_empty()).all(file_actions::php_ident);
        return valid.then(|| php_text::resolver(text)(class));
    }
    let quote = argument.chars().next().filter(|ch| matches!(ch, '\'' | '"'))?;
    let id = argument.strip_prefix(quote)?.strip_suffix(quote)?;
    if id.is_empty() || id.contains(['$', quote]) {
        return None;
    }
    if id.contains('\\') {
        let class = id.replace("\\\\", "\\");
        let class = class.trim_start_matches('\\');
        return class.split('\\').all(file_actions::php_ident).then(|| class.to_owned());
    }
    crate::service_class(index, id, 0)
}

/// `$logger` for `Psr\Log\LoggerInterface`, `$entityManager` for `EntityManagerInterface`.
fn variable_of(class: &str) -> String {
    let short = short_name(class);
    let short = short.strip_suffix("Interface").filter(|rest| !rest.is_empty()).unwrap_or(short);
    let mut chars = short.chars();
    chars.next().map_or_else(String::new, |first| first.to_lowercase().chain(chars).collect())
}

fn enclosing<'a>(model: &'a ClassModel, offset: usize) -> Option<&'a php_text::Method> {
    model.methods.iter().find(|method| method.body.as_ref().is_some_and(|body| body.contains(&offset)))
}

/// A route action: a public, non-static, non-magic method of a controller (or `__invoke`).
fn in_action(model: &ClassModel, offset: usize) -> bool {
    let controller = is_controller(model) || short_name(&model.fqcn).ends_with("Controller");
    enclosing(model, offset).is_some_and(|method| controller && method.visibility == Visibility::Public && !method.is_static && (!method.name.starts_with("__") || method.name == "__invoke"))
}

pub(crate) fn actions(listing: &crate::file_actions::Listing, doc: &Document, selection: Range) -> Vec<FileAction> {
    if doc.language != "php" || doc.text.len() > MAX_SOURCE {
        return Vec::new();
    }
    let (Some(index), Some(offset)) = (listing.index, text::offset(&doc.text, selection.start)) else {
        return Vec::new();
    };
    let Some(model) = listing.php.model() else {
        return Vec::new();
    };
    let Some(call) = call_at(&doc.text, listing.php.masked(), model, offset) else {
        return Vec::new();
    };
    let name = service_of(index, &doc.text, &call.argument).map_or_else(|| call.argument.clone(), |class| short_name(&class).to_owned());
    let mut actions = Vec::new();
    if enclosing(&model, offset).is_none_or(|method| method.name != "__construct" && !method.is_static) {
        actions.push(FileAction {
            title: format!("Inject {name} instead of the container"),
            kind: Some("refactor.rewrite".into()),
            data: Some(format!("{PREFIX}constructor:{}:{offset}", doc.version)),
            ..FileAction::default()
        });
    }
    if in_action(&model, offset) {
        actions.push(FileAction {
            title: format!("Inject {name} as action argument"),
            kind: Some("refactor.rewrite".into()),
            data: Some(format!("{PREFIX}argument:{}:{offset}", doc.version)),
            ..FileAction::default()
        });
    }
    actions
}

pub(crate) fn handles(action: &FileAction) -> bool {
    action.data.as_deref().is_some_and(|data| data.starts_with(PREFIX))
}

/// The edit adding `parameter` at the end of the parameter list `parameters` (`(` to `)` inclusive).
fn append_parameter(text: &str, masked: &str, parameters: std::ops::Range<usize>, parameter: &str) -> (usize, String) {
    let close = parameters.end - 1;
    let before_close = masked[parameters.start + 1..close].trim_end();
    let last = parameters.start + 1 + before_close.len();
    if before_close.is_empty() {
        return (close, parameter.to_owned());
    }
    if !masked[parameters.clone()].contains('\n') {
        return (last, format!(", {parameter}"));
    }
    let eol = intentions::eol_of(text);
    let line = parameters.start + 1 + before_close.rfind('\n').map_or(0, |found| found + 1);
    let indent = intentions::indent_at(text, (line..last).find(|at| !text[*at..].starts_with([' ', '\t'])).unwrap_or(last));
    if before_close.ends_with(',') {
        (last, format!("{eol}{indent}{parameter},"))
    } else {
        (last, format!(",{eol}{indent}{parameter}"))
    }
}

pub(crate) fn resolve(owner: &Main, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let data = action.data.as_deref().and_then(|data| data.strip_prefix(PREFIX)).ok_or(Error::Stale)?;
    let mut parts = data.split(':');
    let (Some(mode), Some(version), Some(Ok(offset))) = (parts.next(), parts.next(), parts.next().map(str::parse::<usize>)) else {
        return Err(Error::Stale);
    };
    if version != doc.version.to_string() || doc.text.len() > MAX_SOURCE {
        return Err(Error::Stale);
    }
    let index = owner.index_for(&doc.path).ok_or(Error::Stale)?;
    let text = &doc.text;
    let model = php_text::class_model(text).ok_or(Error::Stale)?;
    let masked = php_text::mask(text);
    let call = call_at(text, &masked, &model, offset).ok_or(Error::Stale)?;
    let class = service_of(index, text, &call.argument).ok_or_else(|| {
        Error::from(format!("Couldn't find the class of the service {}. Inject it by hand.", call.argument).as_str())
    })?;
    let variable = variable_of(&class);
    let (import, type_name) = match intentions::import_for(text, &class) {
        Some((import, alias)) => (import, alias),
        None => (None, format!("\\{class}")),
    };

    let mut edits: Vec<(usize, String)> = Vec::new();
    let replacement;
    if mode == "argument" {
        let method = enclosing(&model, offset).filter(|_| in_action(&model, offset)).ok_or(Error::Stale)?;
        match method.params.iter().find(|param| param.name == variable) {
            Some(param) if param.type_name.as_deref() == Some(class.as_str()) => {}
            Some(_) => return Err(format!("{}() already has a ${variable} argument of another type. Rename it first.", method.name).into()),
            None => edits.push(append_parameter(text, &masked, method.parameters.clone(), &format!("{type_name} ${variable}"))),
        }
        replacement = format!("${variable}");
    } else {
        let promoted = model.method("__construct").and_then(|ctor| ctor.params.iter().find(|param| param.name == variable && param.promoted));
        let property = model.properties.iter().find(|property| property.name == variable);
        let existing_type = promoted.map(|param| param.type_name.clone()).or(property.map(|property| property.type_name.clone()));
        match existing_type {
            Some(Some(found)) if found == class => {}
            Some(_) => return Err(format!("{} already has a ${variable} property of another type. Rename it first.", short_name(&model.fqcn)).into()),
            None => {
                let parameter = format!("private readonly {type_name} ${variable}");
                match model.method("__construct") {
                    Some(ctor) => edits.push(append_parameter(text, &masked, ctor.parameters.clone(), &parameter)),
                    None => {
                        if let Some(parent) = model.parent.as_deref().filter(|_| !is_controller(&model)) {
                            return Err(format!("{} extends {} and has no constructor. Add one that calls parent::__construct(), then try again.", short_name(&model.fqcn), short_name(parent)).into());
                        }
                        let eol = intentions::eol_of(text);
                        let indent = model.methods.first().map_or("    ", |method| intentions::indent_at(text, method.name_range.start));
                        let indent = if indent.is_empty() { "    " } else { indent };
                        let at = php_text::line_end(&masked, model.body.start);
                        let constructor = format!("{indent}public function __construct(\n{indent}{indent}{parameter},\n{indent}) {{\n{indent}}}\n\n").replace('\n', eol);
                        edits.push((at, constructor));
                    }
                }
            }
        }
        replacement = format!("$this->{variable}");
    }

    let mut text_edits = vec![TextEdit { range: range_of(text, call.range.start, call.range.end), text: replacement }];
    text_edits.extend(edits.into_iter().map(|(at, inserted)| TextEdit { range: range_of(text, at, at), text: inserted }));
    if let Some((range, inserted)) = import {
        text_edits.push(TextEdit { range: range_of(text, range.start, range.end), text: inserted });
    }
    Ok(FileAction {
        title: action.title,
        kind: Some("refactor.rewrite".into()),
        edits: vec![(doc.path.clone(), text_edits)],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, doc};
    use openidea_plugin::file_actions::FileActions;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/Controller/MailController.php";

    fn apply(text: &str, edits: &[TextEdit]) -> String {
        let mut out = text.to_owned();
        let mut sorted: Vec<&TextEdit> = edits.iter().collect();
        sorted.sort_by_key(|edit| std::cmp::Reverse((edit.range.start.line, edit.range.start.character)));
        for edit in sorted {
            let start = text::offset(text, edit.range.start).unwrap();
            let end = text::offset(text, edit.range.end).unwrap();
            out.replace_range(start..end, &edit.text);
        }
        out
    }

    fn run(plugin: &mut Main, path: &str, text: &str, needle: &str, title: &str) -> Result<String, Error> {
        let at = text::position(text, text.find(needle).unwrap());
        let actions = plugin.file_actions(&doc(path, "php", text), Range::new(at, at));
        let action = actions.into_iter().find(|action| action.title == title).unwrap_or_else(|| panic!("no {title}"));
        let resolved = plugin.resolve_file_action(&doc(path, "php", text), action)?;
        Ok(apply(text, &resolved.edits[0].1))
    }

    #[test]
    fn controller_get_by_id_becomes_a_new_promoted_constructor_parameter() {
        let host = MockHost::install();
        let text = "<?php\nnamespace App\\Controller;\n\nuse Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\n\nclass MailController extends AbstractController\n{\n    public function send()\n    {\n        $this->get('app.mailer')->send();\n    }\n}\n";
        host.add_file("src/Controller/MailController.php", text);
        let mut plugin = activated(&host);
        let out = run(&mut plugin, PATH, text, "get('app", "Inject Mailer instead of the container").unwrap();
        assert_eq!(
            out,
            "<?php\nnamespace App\\Controller;\n\nuse Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\nuse App\\Mailer;\n\nclass MailController extends AbstractController\n{\n    public function __construct(\n        private readonly Mailer $mailer,\n    ) {\n    }\n\n    public function send()\n    {\n        $this->mailer->send();\n    }\n}\n"
        );
        let out = run(&mut plugin, PATH, text, "get('app", "Inject Mailer as action argument").unwrap();
        assert!(out.contains("public function send(Mailer $mailer)\n    {\n        $mailer->send();"), "{out}");
    }

    #[test]
    fn container_get_class_joins_the_existing_constructor_and_keeps_the_parent_call() {
        let host = MockHost::install();
        let path = "/project/src/Service/Report.php";
        let text = "<?php\nnamespace App\\Service;\n\nuse Psr\\Log\\LoggerInterface;\n\nclass Report extends Base\n{\n    public function __construct(\n        private ContainerInterface $container,\n    ) {\n        parent::__construct();\n    }\n\n    public function run(): void\n    {\n        $this->container->get(LoggerInterface::class)->info('x');\n    }\n}\n";
        host.add_file("src/Service/Report.php", text);
        let mut plugin = activated(&host);
        let at = text::position(text, text.find("$this->container->get").unwrap());
        assert!(plugin.file_actions(&doc(path, "php", text), Range::new(at, at)).iter().all(|action| !action.title.contains("action argument")));
        let out = run(&mut plugin, path, text, "$this->container->get", "Inject LoggerInterface instead of the container").unwrap();
        assert!(out.contains("        private ContainerInterface $container,\n        private readonly LoggerInterface $logger,\n    ) {\n        parent::__construct();\n    }"), "{out}");
        assert!(out.contains("$this->logger->info('x');"), "{out}");
        assert!(!out.contains("use Psr\\Log\\LoggerInterface;\nuse Psr"), "{out}");
    }

    #[test]
    fn unknown_service_and_parent_without_constructor_are_refused() {
        let host = MockHost::install();
        let text = "<?php\nnamespace App\\Controller;\n\nuse Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\n\nclass MailController extends AbstractController\n{\n    public function send()\n    {\n        $this->get('vendor.thing')->send();\n    }\n}\n";
        host.add_file("src/Controller/MailController.php", text);
        let mut plugin = activated(&host);
        let error = run(&mut plugin, PATH, text, "get('vendor", "Inject 'vendor.thing' instead of the container").unwrap_err().to_string();
        assert!(error.contains("Couldn't find the class of the service 'vendor.thing'"), "{error}");

        let path = "/project/src/Command/Sync.php";
        let command = "<?php\nnamespace App\\Command;\n\nuse App\\Mailer;\n\nclass Sync extends Command\n{\n    protected function execute(): int\n    {\n        $container->get(Mailer::class);\n    }\n}\n";
        host.add_file("src/Command/Sync.php", command);
        let error = run(&mut plugin, path, command, "$container", "Inject Mailer instead of the container").unwrap_err().to_string();
        assert!(error.contains("Sync extends Command and has no constructor"), "{error}");
        assert!(plugin.file_actions(&doc(path, "php", "<?php\nclass A\n{\n    public function x() { $this->get('a'); }\n}\n"), Range::default()).iter().all(|action| !handles(action)));
    }
}
