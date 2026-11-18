//! **Extract to translation key…** on a selected string literal in Twig (inside `{{ }}` or `{% %}`)
//! or PHP: asks for the key, adds it to the default locale's YAML file of the domain and replaces
//! the literal with `'key'|trans`, `$this->translator->trans('key')`, `$translator->trans('key')`
//! or `t('key')`. Selected Twig text keeps the older **Extract translation** action.
use crate::*;
use openidea_plugin::file_actions::{FileAction, FileOperation};

const PREFIX: &str = "translation-extract:";
const MAX_SOURCE: usize = 128 << 10;
const T_FUNCTION: &str = "Symfony\\Component\\Translation\\t";

/// The quoted literal the selection covers (with or without its quotes) and its text.
fn literal(text: &str, start: usize, end: usize) -> Option<(std::ops::Range<usize>, String)> {
    let slice = text.get(start..end)?;
    let start = start + slice.len() - slice.trim_start().len();
    let end = start + slice.trim().len();
    let bytes = text.as_bytes();
    let quoted = |from: usize, to: usize| to >= from + 2 && matches!(bytes[from], b'\'' | b'"') && bytes[to - 1] == bytes[from];
    let range = if quoted(start, end) {
        start..end
    } else if start > 0 && end < bytes.len() && quoted(start - 1, end + 1) {
        start - 1..end + 1
    } else {
        return None;
    };
    let quote = bytes[range.start] as char;
    let value = &text[range.start + 1..range.end - 1];
    let plain = !value.trim().is_empty() && value.len() <= 500 && !value.contains([quote, '\\', '\n', '\r']) && !(quote == '"' && value.contains(['$', '#']));
    plain.then(|| (range, value.to_owned()))
}

/// The literal at the selection, if it is a Twig expression string not already translated.
fn twig_literal(text: &str, start: usize, end: usize) -> Option<(std::ops::Range<usize>, String)> {
    let (range, value) = literal(text, start, end)?;
    let before = &text[..range.start];
    let opened = before.rfind("{{").max(before.rfind("{%"));
    let closed = before.rfind("}}").max(before.rfind("%}"));
    if opened.is_none() || opened < closed || text[range.end..].trim_start().starts_with("|trans") {
        return None;
    }
    Some((range, value))
}

/// The literal at the selection, if it is a PHP string in code not already passed to `trans()` / `t()`.
fn php_literal(text: &str, masked: &str, start: usize, end: usize) -> Option<(std::ops::Range<usize>, String)> {
    let (range, value) = literal(text, start, end)?;
    let inner = masked.get(range.start + 1..range.end - 1)?;
    let in_code = masked.as_bytes()[range.start] == text.as_bytes()[range.start] && inner.trim().is_empty();
    let before = masked[..range.start].trim_end();
    let translated = before.ends_with("trans(") || before.ends_with("t(") || before.ends_with("TranslatableMessage(");
    (in_code && !translated && php_text::class_model(text).is_some()).then_some((range, value))
}

fn target(doc: &Document, start: usize, end: usize) -> Option<(std::ops::Range<usize>, String)> {
    if doc.text.len() > MAX_SOURCE || end <= start {
        return None;
    }
    match doc.language.as_str() {
        "twig" => twig_literal(&doc.text, start, end),
        "php" => php_literal(&doc.text, &php_text::mask(&doc.text), start, end),
        _ => None,
    }
}

pub(crate) fn actions(owner: &Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let (Some(start), Some(end)) = (text::offset(&doc.text, selection.start), text::offset(&doc.text, selection.end)) else {
        return Vec::new();
    };
    if owner.index_for(&doc.path).is_none() || target(doc, start, end).is_none() {
        return Vec::new();
    }
    vec![FileAction {
        title: "Extract to translation key…".into(),
        kind: Some("refactor.extract".into()),
        data: Some(format!("{PREFIX}{}:{start}:{end}", doc.version)),
        ..FileAction::default()
    }]
}

pub(crate) fn handles(action: &FileAction) -> bool {
    action.data.as_deref().is_some_and(|data| data.starts_with(PREFIX))
}

fn unquoted(value: &str) -> &str {
    let value = value.trim();
    value.strip_prefix(['\'', '"']).and_then(|inner| inner.strip_suffix(['\'', '"'])).unwrap_or(value)
}

/// `framework.default_locale` (else `en`) and the translations folder (`framework.translator.default_path`, else `translations`).
pub(crate) fn locale_and_folder(project: &Project, root: &str) -> (String, String) {
    let mut locale = "en".to_owned();
    let mut folder = format!("{root}/translations");
    for file in ["config/packages/framework.yaml", "config/packages/translation.yaml"] {
        let Ok(text) = project.read_file(file) else { continue };
        for key in index::yaml_keys(&text) {
            let value = unquoted(&key.value);
            if key.key == "default_locale" && key.parents == ["framework"] && !value.is_empty() && value.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
                locale = value.to_owned();
            }
            if key.key == "default_path" && key.parents == ["framework", "translator"] {
                if let Some(relative) = value.strip_prefix("%kernel.project_dir%/").filter(|relative| !relative.split('/').any(|part| part.is_empty() || part == "..")) {
                    folder = format!("{root}/{relative}");
                }
            }
        }
    }
    (locale, folder)
}

/// The domain's YAML file for the locale: `messages+intl-icu.en.yaml` or `messages.en.yaml` (`.yml` too).
fn locale_file<'a>(files: &'a [String], domain: &str, locale: &str) -> Option<&'a String> {
    files.iter().find(|file| {
        let name = file_name(file);
        [format!("{domain}+intl-icu.{locale}."), format!("{domain}.{locale}.")].iter().any(|stem| name.strip_prefix(stem.as_str()).is_some_and(|ext| ext == "yaml" || ext == "yml"))
    })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 200 && !key.contains(|ch: char| ch.is_whitespace() || ch.is_control() || "'\"|{}\\".contains(ch))
}

/// The PHP call replacing the literal, plus the `use function` edit when `t()` is used.
fn php_replacement(text: &str, at: usize, key: &str) -> (String, Option<(usize, String)>) {
    let Some(model) = php_text::class_model(text) else {
        return (format!("t('{key}')"), None);
    };
    let constructor_has = model.method("__construct").is_some_and(|ctor| ctor.params.iter().any(|param| param.promoted && param.name == "translator"));
    let method = model.methods.iter().find(|method| method.body.as_ref().is_some_and(|body| body.contains(&at)));
    if (constructor_has || model.properties.iter().any(|property| property.name == "translator")) && method.is_none_or(|method| !method.is_static) {
        return (format!("$this->translator->trans('{key}')"), None);
    }
    if method.is_some_and(|method| method.params.iter().any(|param| param.name == "translator")) {
        return (format!("$translator->trans('{key}')"), None);
    }
    if php_text::mask(text).contains(&format!("use function {T_FUNCTION};")) {
        return (format!("t('{key}')"), None);
    }
    match php_text::insert_use(text, T_FUNCTION) {
        Some(edit) if !edit.text.is_empty() => (format!("t('{key}')"), Some((edit.offset, edit.text.replacen("use ", "use function ", 1)))),
        _ => (format!("\\{T_FUNCTION}('{key}')"), None),
    }
}

pub(crate) fn resolve(owner: &Main, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let data = action.data.as_deref().and_then(|data| data.strip_prefix(PREFIX)).ok_or(Error::Stale)?;
    let mut parts = data.split(':');
    if parts.next() != Some(doc.version.to_string().as_str()) {
        return Err(Error::Stale);
    }
    let (Some(Ok(start)), Some(Ok(end))) = (parts.next().map(str::parse::<usize>), parts.next().map(str::parse::<usize>)) else {
        return Err(Error::Stale);
    };
    let (range, value) = target(doc, start, end).ok_or(Error::Stale)?;
    let index = owner.index_for(&doc.path).ok_or(Error::Stale)?;
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let root = project.root()?;
    let twig = doc.language == "twig";
    let domain = if twig {
        generators::default_domain(&doc.text).ok_or("This template's translation domain isn't a plain name. Add the key by hand.")?
    } else {
        "messages".to_owned()
    };

    let prefix = if twig {
        index.template_of(&doc.path).map(str::to_owned)
    } else {
        php_text::class_model(&doc.text).map(|model| php_text::short_name(&model.fqcn).trim_end_matches("Controller").to_lowercase()).filter(|prefix| !prefix.is_empty())
    };
    let keys = index.translations.get(&domain);
    let taken = |key: &str| keys.is_some_and(|keys| keys.contains_key(key));
    let base = generators::translation_key(prefix.as_deref(), &value).unwrap_or_else(|| "text".into());
    let suggestion = std::iter::once(base.clone()).chain((2..20).map(|n| format!("{base}_{n}"))).find(|key| !taken(key)).unwrap_or(base);
    let Some(key) = host::input("Translation key", Some(&suggestion))? else {
        return Err(Error::Cancelled);
    };
    let key = key.trim().to_owned();
    if !valid_key(&key) {
        return Err("Use a key without spaces, quotes, braces or |, such as blog.title.".into());
    }
    if taken(&key) {
        return Err(format!("The key {key} already exists in {domain}. Choose another key.").into());
    }

    let (locale, folder) = locale_and_folder(&project, &root);
    let files = index.translation_files.get(&domain).map(Vec::as_slice).unwrap_or_default();
    let mut operations = Vec::new();
    let mut edits = Vec::new();
    let file = match locale_file(files, &domain, &locale) {
        Some(file) => {
            let existing = owner.translation_target(doc, file, &key)?;
            let (at, line) = generators::translation_line(&existing, &key, &value);
            edits.push((file.clone(), vec![TextEdit { range: range_of(&existing, at, at), text: line }]));
            file.clone()
        }
        None => {
            let file = format!("{folder}/{domain}.{locale}.yaml");
            file_actions::vacant(&project, &file, &root)?;
            operations.push(FileOperation::Create { path: file.clone(), text: generators::translation_line("", &key, &value).1, overwrite: false });
            file
        }
    };

    let mut source = Vec::new();
    if twig {
        source.push(TextEdit { range: range_of(&doc.text, range.start, range.end), text: format!("'{key}'|trans") });
    } else {
        let (call, import) = php_replacement(&doc.text, range.start, &key);
        source.push(TextEdit { range: range_of(&doc.text, range.start, range.end), text: call });
        if let Some((at, line)) = import {
            source.push(TextEdit { range: range_of(&doc.text, at, at), text: line });
        }
    }
    edits.push((doc.path.clone(), source));
    Ok(FileAction {
        title: format!("Extract to translation key {key} in {}", file_name(&file)),
        kind: Some("refactor.extract".into()),
        operations,
        edits,
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, doc};
    use openidea_plugin::file_actions::FileActions;
    use openidea_plugin::mock::MockHost;

    const CONTROLLER: &str = "<?php\nnamespace App\\Controller;\n\nuse Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\n\nclass PostController extends AbstractController\n{\n    public function save()\n    {\n        $this->addFlash('success', 'Post saved');\n    }\n\n    public function hint(TranslatorInterface $translator)\n    {\n        return 'Hint text';\n    }\n}\n";
    const PHP: &str = "/project/src/Controller/PostController.php";

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

    fn select(plugin: &mut Main, path: &str, language: &str, text: &str, needle: &str) -> Option<FileAction> {
        let start = text.find(needle).unwrap();
        let selection = Range::new(text::position(text, start), text::position(text, start + needle.len()));
        plugin.file_actions(&doc(path, language, text), selection).into_iter().find(handles)
    }

    #[test]
    fn twig_string_becomes_a_trans_key_in_the_default_locale_file() {
        let host = MockHost::install();
        let template = "{% block body %}\n<input placeholder=\"{{ 'Search posts' }}\">\n{{ 'app.title'|trans }}\n{% endblock %}\n";
        host.add_file("templates/blog/search.html.twig", template);
        let mut plugin = activated(&host);
        let path = "/project/templates/blog/search.html.twig";
        assert!(select(&mut plugin, path, "twig", template, "'app.title'").is_none());
        assert!(select(&mut plugin, path, "twig", template, "placeholder").is_none());

        let action = select(&mut plugin, path, "twig", template, "Search posts").unwrap();
        host.answer_next_prompt(Some(&["blog.search.placeholder"]));
        let resolved = plugin.resolve_file_action(&doc(path, "twig", template), action).unwrap();
        assert_eq!(resolved.edits.len(), 2);
        let (file, edits) = &resolved.edits[0];
        assert_eq!(file, "/project/translations/messages.en.yaml");
        assert_eq!(apply("app:\n    title: Blog\n", edits), "app:\n    title: Blog\n'blog.search.placeholder': 'Search posts'\n");
        assert_eq!(apply(template, &resolved.edits[1].1), template.replace("'Search posts'", "'blog.search.placeholder'|trans"));
    }

    #[test]
    fn php_strings_use_the_translator_in_scope_or_t_with_its_import() {
        let host = MockHost::install();
        host.add_file("src/Controller/PostController.php", CONTROLLER);
        host.add_file("config/packages/translation.yaml", "framework:\n    default_locale: fr\n");
        let mut plugin = activated(&host);

        let action = select(&mut plugin, PHP, "php", CONTROLLER, "'Post saved'").unwrap();
        host.answer_next_prompt(Some(&["post.saved"]));
        let resolved = plugin.resolve_file_action(&doc(PHP, "php", CONTROLLER), action).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = resolved.operations.as_slice() else { panic!() };
        assert_eq!((path.as_str(), text.as_str()), ("/project/translations/messages.fr.yaml", "'post.saved': 'Post saved'\n"));
        let out = apply(CONTROLLER, &resolved.edits[0].1);
        assert!(out.contains("$this->addFlash('success', t('post.saved'));"), "{out}");
        assert!(out.contains("use Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController;\nuse function Symfony\\Component\\Translation\\t;\n"), "{out}");

        let action = select(&mut plugin, PHP, "php", CONTROLLER, "Hint text").unwrap();
        host.answer_next_prompt(Some(&["post.hint"]));
        let resolved = plugin.resolve_file_action(&doc(PHP, "php", CONTROLLER), action).unwrap();
        let out = apply(CONTROLLER, &resolved.edits[0].1);
        assert!(out.contains("return $translator->trans('post.hint');"), "{out}");
    }

    #[test]
    fn suggests_a_free_key_and_refuses_taken_or_invalid_keys() {
        let host = MockHost::install();
        host.add_file("src/Controller/PostController.php", CONTROLLER);
        let mut plugin = activated(&host);
        let action = select(&mut plugin, PHP, "php", CONTROLLER, "Post saved").unwrap();
        host.answer_next_prompt(Some(&["app.title"]));
        let error = plugin.resolve_file_action(&doc(PHP, "php", CONTROLLER), action.clone()).unwrap_err().to_string();
        assert!(error.contains("already exists in messages"), "{error}");
        host.answer_next_prompt(Some(&["two words"]));
        let error = plugin.resolve_file_action(&doc(PHP, "php", CONTROLLER), action.clone()).unwrap_err().to_string();
        assert!(error.contains("without spaces"), "{error}");
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc(PHP, "php", CONTROLLER), action), Err(Error::Cancelled)));
        assert_eq!(generators::translation_key(Some("post"), "Post saved").as_deref(), Some("post.post_saved"));
    }
}
