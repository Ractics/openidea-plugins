//! Request DTOs for controller actions (⌥⏎): create the missing class of a `#[MapRequestPayload]` /
//! `#[MapQueryString]` parameter, and move `$request->…->get('x')` reads into a new DTO.
use std::ops::Range as Span;

use crate::*;
use crate::intentions::{self, Edit};
use crate::php_text::{self, Visibility};
use openidea_plugin::file_actions::{FileAction, FileOperation};

const PREFIX: &str = "request-dto:";
const ATTRIBUTES: &str = "Symfony\\Component\\HttpKernel\\Attribute\\";
const REQUEST: &str = "Symfony\\Component\\HttpFoundation\\Request";
const DEFAULT_NAMESPACE: &str = "App\\Dto";
const INPUT: &str = "input";

/// What one action creates and changes; recomputed from the document when it is picked.
#[derive(Debug, PartialEq)]
pub(crate) struct Plan {
    title: String,
    kind: &'static str,
    fqcn: String,
    /// `public string $title`, `public ?int $page = null`.
    properties: Vec<String>,
    /// Edits of the open file, in order.
    edits: Vec<Edit>,
    /// What the edits do, for the confirm.
    changes: String,
    /// The changed method, for the confirm.
    method: Option<String>,
}

/// The method of the class at `at`, with its line-start..body-end span.
fn method_at(model: &php_text::ClassModel, text: &str, at: usize) -> Option<(php_text::Method, Span<usize>)> {
    model.methods.iter().find_map(|method| {
        let body = method.body.clone()?;
        let span = intentions::line_start(text, method.name_range.start)..body.end;
        (span.contains(&at) || span.end == at).then(|| (method.clone(), span))
    })
}

/// The type written before `$name` in a parameter, without attributes and `?`.
fn written_type(masked: &str, text: &str, param: &php_text::Param) -> Option<String> {
    let mut at = param.range.start;
    while masked[at..param.range.end].starts_with("#[") {
        at = php_text::skip_whitespace(masked, php_text::matching_close(masked, at + 1)? + 1);
    }
    let dollar = at + masked[at..param.range.end].find('$')?;
    let written = text[at..dollar].split_whitespace().last()?.trim_start_matches('?');
    (!written.contains(['|', '&', '('])).then(|| written.to_owned())
}

fn valid_fqcn(fqcn: &str) -> bool {
    fqcn.contains('\\') && fqcn.split('\\').all(crate::file_actions::valid_class_name)
}

/// "Create class X": a `#[MapRequestPayload]` / `#[MapQueryString]` parameter whose class is unknown.
fn missing_class(text: &str, at: usize, known: &dyn Fn(&str) -> bool) -> Option<Plan> {
    let model = php_text::class_model(text)?;
    let masked = php_text::mask(text);
    let param = model.methods.iter().flat_map(|method| &method.params).find(|param| param.range.contains(&at) || param.range.end == at)?;
    let mapped = php_text::attributes(text).into_iter().any(|attribute| {
        param.range.contains(&attribute.range.start)
            && [format!("{ATTRIBUTES}MapRequestPayload"), format!("{ATTRIBUTES}MapQueryString")].contains(&attribute.name)
    });
    if !mapped {
        return None;
    }
    let written = written_type(&masked, text, param)?;
    let imported = php_text::imports(text).iter().any(|import| Some(import.alias.as_str()) == written.split('\\').next());
    let unqualified = !written.contains('\\') && !imported;
    let fqcn = if unqualified { format!("{DEFAULT_NAMESPACE}\\{written}") } else { php_text::resolver(text)(&written) };
    if !valid_fqcn(&fqcn) || php_text::BUILT_IN_TYPES.contains(&written.to_ascii_lowercase().as_str()) || known(&fqcn) {
        return None;
    }
    let mut edits = Vec::new();
    let mut changes = String::new();
    if unqualified {
        let (import, _) = intentions::import_for(text, &fqcn)?;
        edits.extend(import);
        changes = format!("Adds use {fqcn}; to this file.\n");
    }
    Some(Plan {
        title: format!("Create class {}…", php_text::short_name(&fqcn)),
        kind: "quickfix",
        fqcn,
        properties: Vec::new(),
        edits,
        changes,
        method: None,
    })
}

#[derive(Debug)]
struct Read {
    range: Span<usize>,
    key: String,
    query: bool,
    /// `string`, `int` or `bool`.
    type_name: &'static str,
    default: Option<String>,
}

/// `->` after `at`, skipping spaces; the offset after it.
fn arrow(masked: &str, at: usize) -> Option<usize> {
    let at = php_text::skip_whitespace(masked, at);
    masked[at..].starts_with("->").then(|| php_text::skip_whitespace(masked, at + 2))
}

/// A plain quoted key: `'title'` or `"title"` without interpolation.
fn literal_key(text: &str) -> Option<String> {
    let text = text.trim();
    let quote = text.chars().next().filter(|quote| matches!(quote, '\'' | '"'))?;
    let inner = text.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.contains([quote, '\\', '$', '{'])).then(|| inner.to_owned())
}

/// Top-level arguments of the call whose `(` is at `open`, and the offset after its `)`.
fn arguments<'a>(text: &'a str, masked: &str, open: usize) -> Option<(Vec<&'a str>, usize)> {
    let close = php_text::matching_close(masked, open)?;
    let mut found = Vec::new();
    let (mut depth, mut start) = (0usize, open + 1);
    for at in open + 1..close {
        match masked.as_bytes()[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                found.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    found.push(text[start..close].trim());
    found.retain(|argument| !argument.is_empty());
    Some((found, close + 1))
}

fn cast_before(text: &str, at: usize) -> Option<&'static str> {
    let before = text[..at].trim_end().to_ascii_lowercase();
    let cast = before.rsplit_once('(')?.1.strip_suffix(')')?.trim().to_owned();
    match cast.as_str() {
        "int" | "integer" => Some("int"),
        "bool" | "boolean" => Some("bool"),
        _ => None,
    }
}

/// The request-data read starting at the variable at `start`: `None` for another use of the
/// variable, `Err` for a read that can't be moved.
fn read_at(text: &str, masked: &str, start: usize, end: usize) -> Option<Result<Read, String>> {
    let after = arrow(masked, end)?;
    let word = php_text::read_word(masked, after);
    let bag = &masked[word.clone()];
    let refused = |what: &str| Some(Err(format!("{what} can't be moved to a DTO.")));
    let (query, mut at) = match bag {
        "request" | "query" => (bag == "query", word.end),
        "getPayload" => {
            let open = php_text::skip_whitespace(masked, word.end);
            let Some((args, next)) = masked[open..].starts_with('(').then(|| arguments(text, masked, open)).flatten() else { return refused("$request->getPayload") };
            if !args.is_empty() {
                return refused("$request->getPayload(…)");
            }
            (false, next)
        }
        "toArray" => {
            let open = php_text::skip_whitespace(masked, word.end);
            let index = masked[open..].strip_prefix("()").map(|_| php_text::skip_whitespace(masked, open + 2));
            let Some(index) = index.filter(|index| masked[*index..].starts_with('[')) else { return refused("$request->toArray() without a key") };
            let close = php_text::matching_close(masked, index)?;
            return Some(match literal_key(&text[index + 1..close]) {
                Some(key) => Ok(Read { range: start..close + 1, key, query: false, type_name: cast_before(text, start).unwrap_or("string"), default: None }),
                None => Err(format!("The key {} is dynamic; only quoted keys can be moved.", text[index + 1..close].trim())),
            });
        }
        _ => return None,
    };
    let Some(method_at) = arrow(masked, at) else { return refused(&format!("$request->{bag}")) };
    let method = php_text::read_word(masked, method_at);
    let typed = match &masked[method.clone()] {
        "get" | "getString" => None,
        "getInt" => Some("int"),
        "getBoolean" => Some("bool"),
        other => return refused(&format!("$request->{bag}->{other}()")),
    };
    let open = php_text::skip_whitespace(masked, method.end);
    let Some((args, next)) = masked[open..].starts_with('(').then(|| arguments(text, masked, open)).flatten() else { return refused(&format!("$request->{bag}")) };
    at = next;
    let Some(first) = args.first() else { return refused("A read without a key") };
    let Some(key) = literal_key(first) else { return Some(Err(format!("The key {first} is dynamic; only quoted keys can be moved."))) };
    if args.len() > 2 {
        return refused(&format!("$request->{bag}->get() with more than two arguments"));
    }
    let type_name = typed.or_else(|| cast_before(text, start)).unwrap_or("string");
    Some(Ok(Read { range: start..at, key, query, type_name, default: args.get(1).map(|default| (*default).to_owned()) }))
}

fn is_controller(model: &php_text::ClassModel, text: &str, method: &php_text::Method) -> bool {
    let routed = php_text::attributes(text).iter().any(|attribute| {
        php_text::short_name(&attribute.name) == "Route"
            && matches!(&attribute.target, php_text::Target::Method { name_range, .. } if *name_range == method.name_range)
    });
    routed || model.fqcn.ends_with("Controller") || model.parent.as_deref().is_some_and(|parent| php_text::short_name(parent) == "AbstractController")
}

/// "Extract request data to a DTO…": reads of `$request` in the action at `at`. `None` when there
/// is nothing to extract, `Err` when the reads can't all move.
fn extraction(text: &str, at: usize, known: &dyn Fn(&str) -> bool) -> Option<Result<Plan, String>> {
    let model = php_text::class_model(text)?;
    let (method, span) = method_at(&model, text, at)?;
    if method.visibility != Visibility::Public || method.is_static || !is_controller(&model, text, &method) {
        return None;
    }
    let request = method.params.iter().find(|param| param.type_name.as_deref() == Some(REQUEST))?;
    let body = method.body.clone()?;
    let masked = php_text::mask(text);
    let variable = format!("${}", request.name);
    let mut reads = Vec::new();
    let mut other_use = false;
    for (found, _) in masked[body.clone()].match_indices(&variable) {
        let start = body.start + found;
        let end = start + variable.len();
        if masked.as_bytes().get(end).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_') {
            continue;
        }
        match read_at(text, &masked, start, end) {
            Some(Ok(read)) => reads.push(read),
            Some(Err(reason)) => return Some(Err(reason)),
            None => other_use = true,
        }
    }
    if reads.is_empty() {
        return None;
    }
    Some(plan_extraction(text, &model, &method, span, request, reads, other_use, known))
}

#[allow(clippy::too_many_arguments)]
fn plan_extraction(
    text: &str,
    model: &php_text::ClassModel,
    method: &php_text::Method,
    span: Span<usize>,
    request: &php_text::Param,
    reads: Vec<Read>,
    other_use: bool,
    known: &dyn Fn(&str) -> bool,
) -> Result<Plan, String> {
    let variable = format!("${}", request.name);
    if text[span.clone()].matches(&variable).count() != php_text::mask(text)[span.clone()].matches(&variable).count() {
        return Err(format!("{variable} is used inside a string; move those reads by hand first."));
    }
    if let Some(read) = reads.iter().find(|read| !crate::file_actions::php_ident(&read.key)) {
        return Err(format!("The key '{}' isn't a valid PHP property name.", read.key));
    }
    let query = reads[0].query;
    if reads.iter().any(|read| read.query != query) {
        return Err("This action reads both the query string and the request body. Split them first.".into());
    }
    let input = format!("${INPUT}");
    let uses_input = text[span.clone()].match_indices(&input).any(|(at, _)| {
        !text[span.start + at + input.len()..].starts_with(|next: char| next.is_ascii_alphanumeric() || next == '_')
    });
    if uses_input {
        return Err(format!("This action already uses ${INPUT}."));
    }

    let action = if method.name.eq_ignore_ascii_case("__invoke") {
        php_text::short_name(&model.fqcn).trim_end_matches("Controller").to_owned()
    } else {
        let mut chars = method.name.chars();
        chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
    };
    let name = format!("{action}Request");
    let fqcn = format!("{DEFAULT_NAMESPACE}\\{name}");
    if !valid_fqcn(&fqcn) || known(&fqcn) {
        return Err(format!("{fqcn} already exists."));
    }

    let mut properties: Vec<(bool, String)> = Vec::new();
    let mut seen = BTreeSet::new();
    for read in &reads {
        if !seen.insert(read.key.as_str()) {
            continue;
        }
        let optional = reads.iter().any(|other| other.key == read.key && other.default.is_some());
        let property = if optional { format!("public ?{} ${} = null", read.type_name, read.key) } else { format!("public {} ${}", read.type_name, read.key) };
        properties.push((optional, property));
    }
    properties.sort_by_key(|(optional, _)| *optional);

    let attribute = if query { "MapQueryString" } else { "MapRequestPayload" };
    let mut edits: Vec<Edit> = Vec::new();
    for fqcn in [format!("{ATTRIBUTES}{attribute}"), fqcn.clone()] {
        let (import, alias) = intentions::import_for(text, &fqcn).ok_or_else(|| format!("Another class is already imported as {}.", php_text::short_name(&fqcn)))?;
        if alias != php_text::short_name(&fqcn) {
            return Err(format!("{fqcn} is imported under another name."));
        }
        if let Some((range, line)) = import {
            match edits.iter_mut().find(|(other, _)| *other == range) {
                Some((_, existing)) => existing.push_str(line.trim_start_matches(['\r', '\n'])),
                None => edits.push((range, line)),
            }
        }
    }

    let parameter = format!("#[{attribute}] {name} ${INPUT}");
    let mut local: Vec<Edit> = Vec::new();
    if other_use {
        let at = method.params.last().map_or(method.parameters.start, |last| last.range.end);
        let separator = if method.params.is_empty() { "" } else { ", " };
        local.push((at..at, format!("{separator}{parameter}")));
    } else {
        local.push((request.range.clone(), parameter));
    }
    for read in &reads {
        let replacement = match &read.default {
            Some(default) => format!("(${INPUT}->{} ?? {default})", read.key),
            None => format!("${INPUT}->{}", read.key),
        };
        local.push((read.range.clone(), replacement));
    }
    local.sort_by_key(|(range, _)| range.start);

    let mut changed = text[span.clone()].to_owned();
    for (range, replacement) in local.iter().rev() {
        changed.replace_range(range.start - span.start..range.end - span.start, replacement);
    }
    edits.extend(local);

    let source = if query { "query string" } else { "request body" };
    let removed = if other_use { String::new() } else { format!(" and {variable} is removed") };
    Ok(Plan {
        title: "Extract request data to a DTO…".into(),
        kind: "refactor.extract",
        fqcn,
        properties: properties.into_iter().map(|(_, property)| property).collect(),
        edits,
        changes: format!(
            "Changes {}(): the {source} is mapped to ${INPUT} with #[{attribute}]{removed}.\nFields read without a default become required: a request without them gets a 422 response.\n",
            method.name
        ),
        method: Some(changed),
    })
}

/// The plan of the action at `at`, if any.
pub(crate) fn plan(text: &str, at: usize, known: &dyn Fn(&str) -> bool) -> Option<Plan> {
    missing_class(text, at, known).or_else(|| extraction(text, at, known).and_then(Result::ok))
}

/// The PHP source of a final DTO class with promoted constructor `properties`.
fn render(fqcn: &str, properties: &[String], strict: bool, eol: &str) -> String {
    let (namespace, name) = fqcn.rsplit_once('\\').unwrap_or(("", fqcn));
    let declare = if strict { "declare(strict_types=1);\n\n" } else { "" };
    let constructor = if properties.is_empty() {
        "    public function __construct()\n    {\n    }\n".to_owned()
    } else {
        let lines: String = properties.iter().map(|property| format!("        {property},\n")).collect();
        format!("    public function __construct(\n{lines}    ) {{\n    }}\n")
    };
    format!("<?php\n\n{declare}namespace {namespace};\n\nfinal class {name}\n{{\n{constructor}}}\n").replace('\n', eol)
}

/// The file Composer's PSR-4 autoload maps `fqcn` to.
fn target(project: &Project, root: &str, fqcn: &str) -> Result<String, Error> {
    let composer = project.read_file(&format!("{root}/composer.json"))?;
    let mappings = crate::psr4::mappings(&composer)?;
    let mapping = mappings
        .iter()
        .filter(|mapping| fqcn.starts_with(&format!("{}\\", mapping.namespace)))
        .max_by_key(|mapping| mapping.namespace.len())
        .ok_or_else(|| format!("composer.json has no PSR-4 folder for {fqcn}. Add its namespace under autoload › psr-4."))?;
    let relative = fqcn[mapping.namespace.len() + 1..].replace('\\', "/");
    Ok(format!("{root}/{}/{relative}.php", mapping.directory))
}

pub(crate) fn actions(owner: &Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    if doc.language != "php" {
        return Vec::new();
    }
    let Some(index) = owner.index_for(&doc.path) else { return Vec::new() };
    let Some(at) = text::offset(&doc.text, selection.start) else { return Vec::new() };
    let known = |class: &str| index.classes.contains_key(class);
    plan(&doc.text, at, &known)
        .map(|plan| FileAction {
            title: plan.title,
            kind: Some(plan.kind.into()),
            data: Some(format!("{PREFIX}{}:{at}", doc.version)),
            ..FileAction::default()
        })
        .into_iter()
        .collect()
}

pub(crate) fn handles(action: &FileAction) -> bool {
    action.data.as_deref().is_some_and(|data| data.starts_with(PREFIX))
}

pub(crate) fn resolve(owner: &Main, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let data = action.data.as_deref().and_then(|data| data.strip_prefix(PREFIX)).ok_or(Error::Stale)?;
    let (version, at) = data.split_once(':').ok_or(Error::Stale)?;
    if version != doc.version.to_string() {
        return Err(Error::Stale);
    }
    let at: usize = at.parse().map_err(|_| Error::Stale)?;
    let index = owner.index_for(&doc.path).ok_or(Error::Stale)?;
    let known = |class: &str| index.classes.contains_key(class);
    let plan = plan(&doc.text, at, &known).ok_or(Error::Stale)?;
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let root = project.root()?;
    let path = target(&project, &root, &plan.fqcn)?;
    crate::file_actions::vacant(&project, &path, &root)?;

    let eol = intentions::eol_of(&doc.text);
    let class = render(&plan.fqcn, &plan.properties, doc.text.contains("declare(strict_types=1)"), eol);
    let shown = path.strip_prefix(&format!("{root}/")).unwrap_or(&path);
    let name = php_text::short_name(&plan.fqcn);
    let method = plan.method.as_deref().map(|method| format!("\n{method}\n")).unwrap_or_default();
    let preview = format!("Creates {shown} (class {}) and opens it.\n{}Undo reverts every change.\n\n{class}{method}", plan.fqcn, plan.changes);
    let question = if plan.method.is_some() { format!("Move request data to {name}?") } else { format!("Create {name}?") };
    if !ui_kit::prompts::confirm_exact(&question, &preview)? {
        return Err(Error::Cancelled);
    }
    crate::file_actions::vacant(&project, &path, &root)?;
    let edits: Vec<TextEdit> = plan.edits.into_iter().map(|(range, text)| TextEdit { range: range_of(&doc.text, range.start, range.end), text }).collect();
    Ok(FileAction {
        title: action.title,
        kind: action.kind,
        operations: vec![FileOperation::Create { path: path.clone(), text: class, overwrite: false }],
        edits: if edits.is_empty() { Vec::new() } else { vec![(doc.path.clone(), edits)] },
        open: Some(path),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost};

    const CONTROLLER: &str = "<?php\nnamespace App\\Controller;\n\nuse Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\nuse Symfony\\Component\\HttpFoundation\\Request;\nuse Symfony\\Component\\HttpFoundation\\Response;\n\nclass PostController extends AbstractController\n{\n    public function create(Request $request): Response\n    {\n        $title = $request->request->get('title');\n        $page = (int) $request->request->get('page', 1);\n        $draft = $request->getPayload()->getBoolean('draft');\n        $tag = $request->toArray()['tag'];\n        return $this->json([$title, $page, $draft, $tag]);\n    }\n}\n";

    fn none(_: &str) -> bool {
        false
    }

    fn apply(text: &str, edits: &[Edit]) -> String {
        let mut out = text.to_owned();
        for (range, replacement) in edits.iter().rev() {
            out.replace_range(range.clone(), replacement);
        }
        out
    }

    fn extract(text: &str) -> Result<Plan, String> {
        extraction(text, text.find("$title =").unwrap(), &none).unwrap()
    }

    #[test]
    fn reads_move_to_a_typed_dto_and_the_request_parameter_goes() {
        let plan = extract(CONTROLLER).unwrap();
        assert_eq!(plan.fqcn, "App\\Dto\\CreateRequest");
        assert_eq!(plan.properties, ["public string $title", "public bool $draft", "public string $tag", "public ?int $page = null"]);
        let out = apply(CONTROLLER, &plan.edits);
        assert!(out.contains("use Symfony\\Component\\HttpFoundation\\Response;\nuse Symfony\\Component\\HttpKernel\\Attribute\\MapRequestPayload;\nuse App\\Dto\\CreateRequest;\n"));
        assert!(out.contains("public function create(#[MapRequestPayload] CreateRequest $input): Response"));
        assert!(out.contains("$title = $input->title;\n        $page = (int) ($input->page ?? 1);\n        $draft = $input->draft;\n        $tag = $input->tag;"));
        assert!(!out.contains("$request"));
        assert!(plan.method.unwrap().starts_with("    public function create(#[MapRequestPayload]"));
        let class = render(&plan.fqcn, &plan.properties, false, "\n");
        assert_eq!(class, "<?php\n\nnamespace App\\Dto;\n\nfinal class CreateRequest\n{\n    public function __construct(\n        public string $title,\n        public bool $draft,\n        public string $tag,\n        public ?int $page = null,\n    ) {\n    }\n}\n");
    }

    #[test]
    fn query_reads_map_the_query_string_and_other_uses_keep_the_request() {
        let text = CONTROLLER
            .replace("$title = $request->request->get('title');\n        $page = (int) $request->request->get('page', 1);\n        $draft = $request->getPayload()->getBoolean('draft');\n        $tag = $request->toArray()['tag'];\n", "$title = $request->query->get('title');\n        $page = $request->query->getInt('page');\n        $method = $request->getMethod();\n");
        let plan = extract(&text).unwrap();
        assert_eq!(plan.properties, ["public string $title", "public int $page"]);
        let out = apply(&text, &plan.edits);
        assert!(out.contains("public function create(Request $request, #[MapQueryString] CreateRequest $input): Response"));
        assert!(out.contains("$page = $input->page;\n        $method = $request->getMethod();"));
        assert!(out.contains("use Symfony\\Component\\HttpKernel\\Attribute\\MapQueryString;\n"));
    }

    #[test]
    fn dynamic_keys_mixed_sources_and_unsupported_reads_are_refused() {
        let swap = |from: &str, to: &str| extract(&CONTROLLER.replace(from, to)).unwrap_err();
        assert!(swap("get('title')", "get($key)").contains("dynamic"));
        assert!(swap("toArray()['tag']", "toArray()[$key]").contains("dynamic"));
        assert!(swap("get('title')", "get(\"t{$x}\")").contains("dynamic"));
        assert!(swap("$request->request->get('title')", "$request->query->get('title')").contains("both"));
        assert!(swap("$request->request->get('title')", "$request->request->all()").contains("can't be moved"));
        assert!(swap("get('title')", "get('first-name')").contains("valid PHP property"));
        assert!(swap("return $this->json", "$input = 1;\n        return $this->json").contains("$input"));
        assert!(extraction(CONTROLLER, CONTROLLER.find("$title =").unwrap(), &|class| class == "App\\Dto\\CreateRequest").unwrap().unwrap_err().contains("exists"));
        let plain = CONTROLLER.replace("$title = $request->request->get('title');\n        $page = (int) $request->request->get('page', 1);\n        $draft = $request->getPayload()->getBoolean('draft');\n        $tag = $request->toArray()['tag'];\n", "$title = $request->getMethod();\n");
        assert!(extraction(&plain, plain.find("$title =").unwrap(), &none).is_none());
    }

    const MAPPED: &str = "<?php\nnamespace App\\Controller;\n\nuse Symfony\\Component\\HttpKernel\\Attribute\\MapRequestPayload;\n\nclass PostController\n{\n    public function create(#[MapRequestPayload] PostInput $input): void\n    {\n    }\n}\n";

    #[test]
    fn a_missing_mapped_class_is_created_in_app_dto_when_unqualified() {
        let plan = missing_class(MAPPED, MAPPED.find("PostInput").unwrap(), &none).unwrap();
        assert_eq!((plan.title.as_str(), plan.fqcn.as_str()), ("Create class PostInput…", "App\\Dto\\PostInput"));
        assert!(apply(MAPPED, &plan.edits).contains("MapRequestPayload;\nuse App\\Dto\\PostInput;\n"));
        let imported = MAPPED.replace("Attribute\\MapRequestPayload;\n", "Attribute\\MapRequestPayload;\nuse App\\Api\\Input\\PostInput;\n");
        let plan = missing_class(&imported, imported.find("PostInput $").unwrap(), &none).unwrap();
        assert_eq!(plan.fqcn, "App\\Api\\Input\\PostInput");
        assert!(plan.edits.is_empty());
        assert!(missing_class(MAPPED, MAPPED.find("PostInput").unwrap(), &|class| class == "App\\Dto\\PostInput").is_none());
        let unmapped = MAPPED.replace("#[MapRequestPayload] ", "");
        assert!(missing_class(&unmapped, unmapped.find("PostInput").unwrap(), &none).is_none());
        assert_eq!(render("App\\Dto\\PostInput", &[], true, "\n"), "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Dto;\n\nfinal class PostInput\n{\n    public function __construct()\n    {\n    }\n}\n");
    }

    fn setup(text: &str) -> (MockHost, Main, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("symfony.lock", "{}");
        host.add_file("composer.json", r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#);
        host.add_file("src/Controller/PostController.php", text);
        let mut owner = Main::default();
        owner.indexes.insert("/project".into(), Index::default());
        let doc = Document { path: "/project/src/Controller/PostController.php".into(), language: "php".into(), version: 4, text: text.into() };
        (host, owner, doc)
    }

    fn caret(text: &str, needle: &str) -> Range {
        let at = text::position(text, text.find(needle).unwrap());
        Range::new(at, at)
    }

    #[test]
    fn the_extraction_creates_the_dto_and_edits_the_action_after_one_confirm() {
        let (host, mut owner, doc) = setup(CONTROLLER);
        let action = owner.file_actions(&doc, caret(CONTROLLER, "$title =")).into_iter().find(handles).unwrap();
        assert_eq!(action.title, "Extract request data to a DTO…");
        host.answer_next_prompt(Some(&[]));
        let out = owner.resolve_file_action(&doc, action).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = out.operations.as_slice() else { panic!("one create") };
        assert_eq!(path, "/project/src/Dto/CreateRequest.php");
        assert!(text.contains("final class CreateRequest"));
        assert_eq!(out.open.as_deref(), Some("/project/src/Dto/CreateRequest.php"));
        assert_eq!(out.edits.len(), 1);
        assert_eq!(out.edits[0].0, doc.path);
    }

    #[test]
    fn cancelled_stale_or_existing_targets_create_nothing() {
        let (host, mut owner, doc) = setup(MAPPED);
        let action = owner.file_actions(&doc, caret(MAPPED, "PostInput")).into_iter().find(handles).unwrap();
        assert_eq!(action.title, "Create class PostInput…");
        host.answer_next_prompt(None);
        assert!(matches!(owner.resolve_file_action(&doc, action.clone()), Err(Error::Cancelled)));
        let mut changed = doc.clone();
        changed.version += 1;
        assert!(matches!(owner.resolve_file_action(&changed, action.clone()), Err(Error::Stale)));
        host.add_file("src/Dto/PostInput.php", "<?php\n");
        assert!(owner.resolve_file_action(&doc, action).is_err());
    }
}
