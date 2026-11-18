//! Rename a Twig template (moving its file), a translation key or a service id in one
//! reviewed FileAction. Every spelling of the name in the scanned project sources must be a
//! proven reference; one dynamic or unknown use refuses the whole rename. These names are not
//! published as host index anchors: the host writes the new name into every anchor unchecked,
//! nested YAML keys are not one token, and an anchor cannot move a file.
use crate::*;
use openidea_plugin::file_actions::{FileAction, FileOperation};
use std::ops::Range as Bytes;

const PREFIX: &str = "literal-rename:";
const MAX_FILES: usize = 2000;
const MAX_TOTAL: usize = 32 << 20;
const MAX_NAME: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum What {
    Template,
    Translation,
    Service,
}

/// `domain` is set for translations, `file` for templates.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Target {
    what: What,
    name: String,
    domain: String,
    file: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Request {
    root: String,
    path: String,
    version: u64,
    selection: Range,
    target: Target,
}

/// One proven spelling: its byte range and, for a nested YAML translation key, the parent
/// path the new key has to keep.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Spot {
    range: Bytes<usize>,
    parent: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Change {
    path: String,
    text: String,
    spots: Vec<Spot>,
}

struct Proof {
    what: What,
    /// Translations only; `None` is a domain that can't be known from the source.
    domain: Option<String>,
}

fn name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_-./\\".contains(&b) || b >= 0x80
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_number(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

fn language(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()? {
        "php" => Some("php"),
        "twig" => Some("twig"),
        "yaml" | "yml" => Some("yaml"),
        "xml" => Some("xml"),
        _ => None,
    }
}

fn is_translation_file(path: &str) -> bool {
    path.contains("/translations/")
}

/// The quote around `s..e`, when the same quote closes it.
fn quoted(text: &str, s: usize, e: usize) -> Option<u8> {
    let q = *text.as_bytes().get(s.checked_sub(1)?)?;
    (matches!(q, b'\'' | b'"') && text.as_bytes().get(e) == Some(&q)).then_some(q)
}

/// The literal is not inside a PHP comment or heredoc.
fn php_code_literal(text: &str, s: usize, e: usize) -> bool {
    let Some(q) = quoted(text, s, e) else { return false };
    let masked = crate::php_text::mask(text);
    masked.as_bytes().get(s - 1) == Some(&q) && masked.as_bytes().get(e) == Some(&q)
}

/// Nothing after the closing quote extends the name at run time.
fn static_tail(text: &str, e: usize, what: What) -> bool {
    let tail = text.get(e + 1..).unwrap_or("").trim_start();
    if ["-%}", "~%}", "-}}", "~}}"].iter().any(|end| tail.starts_with(end)) {
        return true;
    }
    if what == What::Translation && tail.strip_prefix('|').is_some_and(|rest| rest.trim_start().starts_with("trans")) {
        return true;
    }
    !tail.starts_with(['~', '.', '?', '+', '-', '|', '['])
}

/// The plain value of a `key: value` YAML line, without quotes.
fn yaml_value(text: &str, key: &index::YamlKey) -> Option<Bytes<usize>> {
    let end = text[key.offset..].find('\n').map_or(text.len(), |i| key.offset + i);
    let mut at = key.offset + key.key.len();
    if matches!(text.as_bytes().get(at), Some(b'\'' | b'"')) {
        at += 1;
    }
    at += text.get(at..end)?.find(':')? + 1;
    let raw = &text[at..end];
    let start = at + raw.len() - raw.trim_start().len();
    let value = index::without_comment(&text[start..end]).trim_end();
    if let Some(q) = value.as_bytes().first().filter(|q| matches!(q, b'\'' | b'"')) {
        return (value.len() >= 2 && value.as_bytes()[value.len() - 1] == *q).then(|| start + 1..start + value.len() - 1);
    }
    (!value.is_empty()).then(|| start..start + value.len())
}

fn in_xml_comment(text: &str, at: usize) -> bool {
    text[..at].rfind("<!--").is_some_and(|open| !text[open..at].contains("-->"))
}

fn xml_service(text: &str, s: usize, e: usize) -> Option<Proof> {
    quoted(text, s, e)?;
    if in_xml_comment(text, s) {
        return None;
    }
    let before = text[..s - 1].strip_suffix('=')?;
    let attribute = &before[before.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':')).len()..];
    let open = before.rfind('<')?;
    if text[open..s].contains('>') {
        return None;
    }
    let tag_end = text[open..].find('>').map_or(text.len(), |i| open + i);
    let tag = &text[open..tag_end];
    let tag_name = &tag[1..tag[1..].find(|c: char| c.is_whitespace() || c == '/').map_or(tag.len(), |i| i + 1)];
    let definition = tag_name == "service" && attribute == "id";
    let reference = (tag_name == "service" && matches!(attribute, "alias" | "parent" | "decorates"))
        || (attribute == "id" && index::xml_attribute(tag, "type").is_some_and(|(kind, _)| kind == "service"));
    (definition || reference).then_some(Proof { what: What::Service, domain: None })
}

fn yaml_extra(path: &str, text: &str, s: usize, e: usize) -> Option<Proof> {
    if !path.contains("/config/") || text[line_start(text, s)..s].contains('#') {
        return None;
    }
    for key in index::yaml_keys(text) {
        if key.parents == ["services"] && key.offset == s && key.key.len() == e - s && !key.key.starts_with('_') {
            return Some(Proof { what: What::Service, domain: None });
        }
        if yaml_value(text, &key) != Some(s..e) {
            continue;
        }
        if key.key == "template" {
            return Some(Proof { what: What::Template, domain: None });
        }
        if key.parents.len() == 2 && key.parents[0] == "services" && matches!(key.key.as_str(), "alias" | "parent" | "decorates") {
            return Some(Proof { what: What::Service, domain: None });
        }
    }
    None
}

fn php_extra(text: &str, s: usize, e: usize) -> Option<Proof> {
    let at_sign = s > 0 && text.as_bytes()[s - 1] == b'@';
    let (start, end) = if at_sign { (s - 1, e) } else { (s, e) };
    if !php_code_literal(text, start, end) {
        return None;
    }
    let before = text[line_start(text, start)..start - 1].trim_end();
    if before.contains("#[Autowire(") && (before.ends_with("service:") || (at_sign && (before.ends_with("Autowire(") || before.ends_with("value:")))) {
        return Some(Proof { what: What::Service, domain: None });
    }
    let label = before.strip_suffix("=>").map(str::trim_end).is_some_and(|key| key.ends_with("'label'") || key.ends_with("\"label\""));
    if !at_sign && label && text.contains("AbstractType") {
        let domain = (!text.contains("translation_domain")).then(|| DEFAULT_DOMAIN.to_owned());
        return Some(Proof { what: What::Translation, domain });
    }
    None
}

fn twig_form_theme(text: &str, s: usize, e: usize) -> Option<Proof> {
    ownership::twig_strings(text)?.contains(&(s..e)).then_some(())?;
    refs::twig_tags(text)
        .any(|(name, rest, offset)| name == "form_theme" && offset <= s && e <= offset + rest.len())
        .then_some(Proof { what: What::Template, domain: None })
}

/// Whether `s..e` of a source is a proven template, translation or service spelling.
fn classify(doc: &Document, s: usize, e: usize) -> Option<Proof> {
    if let Some(r) = ownership::reference(doc, s).filter(|r| r.start == s && r.end == e) {
        let what = match r.kind {
            Kind::Template => What::Template,
            Kind::Translation => What::Translation,
            Kind::Service => What::Service,
            _ => return None,
        };
        if !static_tail(&doc.text, e, what) {
            return None;
        }
        let domain = (what == What::Translation).then(|| crate::domain_of(&doc.language, &doc.text, &r)).flatten();
        return Some(Proof { what, domain });
    }
    match doc.language.as_str() {
        "php" => php_extra(&doc.text, s, e),
        "twig" => twig_form_theme(&doc.text, s, e),
        "yaml" => yaml_extra(&doc.path, &doc.text, s, e),
        "xml" if doc.path.contains("/config/") => xml_service(&doc.text, s, e),
        _ => None,
    }
}

/// Every key of a YAML or XLIFF translation file: full key, the ranges to rewrite and the
/// parent path a nested YAML key keeps.
fn translation_keys(path: &str, text: &str) -> Vec<(String, Vec<Bytes<usize>>, String)> {
    let mut found = Vec::new();
    if path.ends_with(".yaml") || path.ends_with(".yml") {
        for key in index::yaml_keys(text).into_iter().filter(|key| !key.value.is_empty()) {
            if text.get(key.offset..key.offset + key.key.len()) != Some(key.key.as_str()) {
                continue;
            }
            let parent = key.parents.join(".");
            let full = if parent.is_empty() { key.key.clone() } else { format!("{parent}.{}", key.key) };
            found.push((full, vec![key.offset..key.offset + key.key.len()], parent));
        }
        return found;
    }
    for (tag_name, key_attribute, close) in [("trans-unit", "resname", "</trans-unit>"), ("unit", "name", "</unit>")] {
        for (tag, at) in index::xml_tags(text, tag_name) {
            let after = at + tag.len();
            let body_end = text[after..].find(close).map_or(text.len(), |i| after + i);
            let source = text[after..body_end]
                .find("<source>")
                .map(|i| after + i + "<source>".len())
                .and_then(|start| Some(start..start + text[start..body_end].find("</source>")?));
            let named = index::xml_attribute(tag, key_attribute).map(|(value, offset)| (value.to_owned(), at + offset..at + offset + value.len()));
            let Some(key) = named.as_ref().map(|(value, _)| value.clone()).or_else(|| source.clone().map(|range| text[range].to_owned())) else { continue };
            let mut ranges: Vec<_> = named.into_iter().map(|(_, range)| range).collect();
            if let Some((id, offset)) = index::xml_attribute(tag, "id").filter(|(id, _)| *id == key) {
                ranges.push(at + offset..at + offset + id.len());
            }
            if let Some(range) = source.filter(|range| text[range.clone()] == key) {
                ranges.push(range);
            }
            ranges.sort_by_key(|range| range.start);
            ranges.dedup();
            found.push((key, ranges, String::new()));
        }
    }
    found
}

fn template_names<'a>(index: &'a Index, file: &str) -> Vec<&'a String> {
    index.templates.iter().filter(|(_, entry)| entry.path == file).map(|(name, _)| name).collect()
}

/// The proven name under the caret, or the open template itself.
fn target_at(index: &Index, doc: &Document, at: usize) -> Option<Target> {
    if doc.text.len() > ownership::MAX_TEXT {
        return None;
    }
    if is_translation_file(&doc.path) && (doc.path.ends_with(".yaml") || doc.path.ends_with(".yml") || doc.path.ends_with(".xlf") || doc.path.ends_with(".xliff")) {
        let domain = index::translation_domain(&doc.path).to_owned();
        let (name, _, _) = translation_keys(&doc.path, &doc.text).into_iter().find(|(_, ranges, _)| ranges.iter().any(|r| r.start <= at && at <= r.end))?;
        return translation_target(index, name, domain);
    }
    let spans = [
        ownership::reference(doc, at).map(|r| r.start..r.end),
        refs::literal_at(&doc.language, &doc.text, at).filter(|(_, _, closed)| *closed).map(|(s, e, _)| s + usize::from(doc.text[s..e].starts_with('@')) .. e),
        (doc.language == "yaml").then(|| index::yaml_keys(&doc.text).into_iter().find_map(|key| {
            let value = yaml_value(&doc.text, &key).filter(|r| r.start <= at && at <= r.end);
            let name = (key.offset <= at && at <= key.offset + key.key.len()).then(|| key.offset..key.offset + key.key.len());
            value.or(name)
        })).flatten(),
    ];
    for span in spans.into_iter().flatten() {
        let Some(proof) = classify(doc, span.start, span.end) else { continue };
        let name = doc.text[span].to_owned();
        return match proof.what {
            What::Template => template_target(index, &name),
            What::Translation => translation_target(index, name, proof.domain?),
            What::Service => service_target(index, name),
        };
    }
    if doc.language == "twig" {
        if let [name] = template_names(index, &doc.path).as_slice() {
            return template_target(index, name);
        }
    }
    None
}

fn template_target(index: &Index, name: &str) -> Option<Target> {
    if index.ambiguous_templates.contains(name) {
        return None;
    }
    let file = &index.templates.get(name)?.path;
    let single = template_names(index, file).len() == 1;
    (single && !file.contains("/templates/bundles/") && file.ends_with(&format!("/{}", local_template(name).1)))
        .then(|| Target { what: What::Template, name: name.to_owned(), domain: String::new(), file: file.clone() })
}

fn translation_target(index: &Index, name: String, domain: String) -> Option<Target> {
    index.translations.get(&domain)?.get(&name)?;
    Some(Target { what: What::Translation, name, domain, file: String::new() })
}

fn service_target(index: &Index, name: String) -> Option<Target> {
    let entry = index.services.get(&name)?;
    let config = entry.path.contains("/config/") && matches!(language(&entry.path), Some("yaml" | "xml"));
    (config && !name.contains('\\') && !name.starts_with('.')).then(|| Target { what: What::Service, name, domain: String::new(), file: String::new() })
}

/// `@Admin/` and the name inside its root.
fn local_template(name: &str) -> (&str, &str) {
    match name.strip_prefix('@').and_then(|rest| rest.split_once('/')) {
        Some((namespace, local)) => (&name[..namespace.len() + 2], local),
        None => ("", name),
    }
}

fn kind_label(target: &Target) -> String {
    match target.what {
        What::Template => "template".into(),
        What::Translation => format!("translation key ({} domain)", target.domain),
        What::Service => "service".into(),
    }
}

pub(crate) fn actions(owner: &Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let run = || -> Option<FileAction> {
        let project = host::project_of(&doc.path)?;
        let root = project.root().ok()?;
        let index = owner.indexes.get(&root).filter(|index| !index.partial && index.settings_current())?;
        let target = target_at(index, doc, text::offset(&doc.text, selection.start)?)?;
        let title = match target.what {
            What::Template => format!("Rename or move template {}…", target.name),
            What::Translation => format!("Rename translation key {} ({})…", target.name, target.domain),
            What::Service => format!("Rename service {}…", target.name),
        };
        let request = Request { root, path: doc.path.clone(), version: doc.version, selection, target };
        Some(FileAction { title, kind: Some("refactor.rename".into()), data: Some(format!("{PREFIX}{}", serde_json::to_string(&request).ok()?)), ..FileAction::default() })
    };
    run().into_iter().collect()
}

pub(crate) fn handles(action: &FileAction) -> bool {
    action.data.as_deref().is_some_and(|data| data.starts_with(PREFIX))
}

fn relative<'a>(root: &str, path: &'a str) -> &'a str {
    path.strip_prefix(root).and_then(|rest| rest.strip_prefix('/')).unwrap_or(path)
}

fn refuse(message: String) -> Error {
    Error::Invalid(message)
}

/// Every proven spelling of the target in the project, or why the rename is unsafe.
fn scan(project: &Project, root: &str, doc: &Document, index: &Index, target: &Target) -> Result<Vec<Change>, Error> {
    let name = target.name.as_str();
    let mut globs = vec!["**/*.php", "**/*.twig", "config/**/*.yaml", "config/**/*.yml", "config/**/*.xml"];
    if target.what == What::Translation {
        globs.push("translations/**/*");
    }
    let nested = crate::nested_roots(root);
    let mut paths = BTreeSet::new();
    for glob in globs {
        let found = project.find_files(glob, (MAX_FILES + 1) as u32)?;
        if found.len() > MAX_FILES {
            return Err(refuse(format!("The project has more than {MAX_FILES} files matching {glob}, so not every use of {name} can be checked. Rename it by hand.")));
        }
        paths.extend(found);
    }
    paths.insert(doc.path.clone());
    let strict = name.contains(['.', '/', '\\', '@', ':']);
    let mut total = 0usize;
    let mut changes = Vec::new();
    for path in paths {
        let rel = relative(root, &path);
        if !under(&path, root)
            || rel.starts_with("var/")
            || rel.split('/').any(|part| matches!(part, "vendor" | "node_modules" | ".git"))
            || nested.iter().any(|inner| under(&path, inner))
        {
            continue;
        }
        let translations = target.what == What::Translation && rel.starts_with("translations/");
        if translations && index::translation_domain(&path) != target.domain {
            continue;
        }
        let Some(lang) = language(&path).or(translations.then_some("translation")) else { continue };
        let text = if path == doc.path { doc.text.clone() } else { project.read_file(&path)? };
        total += text.len();
        if total > MAX_TOTAL {
            return Err(refuse(format!("The project sources are over 32 MiB, so not every use of {name} can be checked. Rename it by hand.")));
        }
        if translations {
            if !matches!(path.rsplit('.').next(), Some("yaml" | "yml" | "xlf" | "xliff")) {
                return Err(refuse(format!("{rel} is a {} translation file this rename can't edit. Rename {name} by hand.", target.domain)));
            }
            let found: Vec<_> = translation_keys(&path, &text).into_iter().filter(|(key, _, _)| key == name).collect();
            if found.len() > 1 {
                return Err(refuse(format!("{rel} defines {name} more than once. Remove the duplicate, then try again.")));
            }
            let spots: Vec<_> = found.into_iter().flat_map(|(_, ranges, parent)| ranges.into_iter().map(move |range| Spot { range, parent: parent.clone() })).collect();
            if !spots.is_empty() {
                changes.push(Change { path, text, spots });
            }
            continue;
        }
        if !text.contains(name) {
            continue;
        }
        if text.len() > ownership::MAX_TEXT {
            return Err(refuse(format!("{rel} is over 128 KiB and mentions {name}, so its uses can't be checked. Rename it by hand.")));
        }
        let source = Document { path: path.clone(), language: lang.into(), text, version: 0 };
        let mut spots = Vec::new();
        for (s, _) in source.text.match_indices(name) {
            let e = s + name.len();
            let bytes = source.text.as_bytes();
            if s > 0 && name_byte(bytes[s - 1]) || bytes.get(e).is_some_and(|b| name_byte(*b)) {
                continue;
            }
            match classify(&source, s, e).filter(|proof| proof.what == target.what) {
                Some(proof) if target.what == What::Translation => match proof.domain {
                    Some(domain) if domain == target.domain => spots.push(Spot { range: s..e, parent: String::new() }),
                    Some(_) => {}
                    None => {
                        return Err(refuse(format!("The domain of {name} at {rel}:{} can't be read from the source. Rename it by hand.", line_number(&source.text, s))));
                    }
                },
                Some(_) => spots.push(Spot { range: s..e, parent: String::new() }),
                None => {
                    let counted = strict || quoted(&source.text, s, e).is_some() || (lang == "yaml" && s > 0 && bytes[s - 1] == b'@');
                    if counted {
                        return Err(refuse(format!(
                            "{name} is also written at {rel}:{}, where it isn't a plain {} reference. Change that line or rename by hand.",
                            line_number(&source.text, s),
                            match target.what {
                                What::Template => "template",
                                What::Translation => "translation key",
                                What::Service => "service",
                            }
                        )));
                    }
                }
            }
        }
        if !spots.is_empty() {
            changes.push(Change { path, text: source.text, spots });
        }
    }
    let definition = match target.what {
        What::Template => Some(target.file.clone()),
        What::Translation => index.translations.get(&target.domain).and_then(|keys| keys.get(name)).map(|entry| entry.path.clone()),
        What::Service => index.services.get(name).map(|entry| entry.path.clone()),
    };
    let defined = target.what == What::Template || definition.as_ref().is_some_and(|path| changes.iter().any(|change| &change.path == path));
    if !defined {
        return Err(refuse(format!("The definition of {name} wasn't found in the project files. Run Symfony: Reindex Project, then try again.")));
    }
    Ok(changes)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// The full new template name and its file.
fn new_template(target: &Target, input: &str) -> Result<(String, String), Error> {
    let (namespace, local) = local_template(&target.name);
    let input = input.trim();
    let new_local = if namespace.is_empty() { input } else { input.strip_prefix(namespace).unwrap_or(input) };
    let ok = !new_local.starts_with('@')
        && new_local.ends_with(".twig")
        && new_local.len() <= 512
        && new_local.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && part.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)));
    if !ok {
        return Err(refuse(format!("Use a relative name ending in .twig with letters, digits, dots, dashes, underscores and slashes{}.", if namespace.is_empty() { String::new() } else { format!(", in {namespace}") })));
    }
    let base = target.file.strip_suffix(local).ok_or(Error::Stale)?;
    Ok((format!("{namespace}{new_local}"), format!("{base}{new_local}")))
}

fn check_new(index: &Index, project: &Project, root: &str, target: &Target, new: &str) -> Result<Option<String>, Error> {
    match target.what {
        What::Template => {
            let (name, file) = new_template(target, new)?;
            if name == target.name {
                return Err(Error::Cancelled);
            }
            if index.templates.contains_key(&name) || project.exists(&file)? {
                return Err(refuse(format!("{name} already exists. Choose another name.")));
            }
            let parent = file.rsplit_once('/').map_or("", |(parent, _)| parent);
            if !project.stat(parent).is_ok_and(|stat| stat.kind == openidea_plugin::FileKind::Directory) {
                return Err(refuse(format!("The folder {} doesn't exist. Create it first, then try again.", relative(root, parent))));
            }
            Ok(Some(file))
        }
        What::Translation | What::Service => {
            if new == target.name {
                return Err(Error::Cancelled);
            }
            if !valid_name(new) {
                return Err(refuse("Use letters, digits, dots, dashes and underscores, without a leading, trailing or double dot.".into()));
            }
            let taken = match target.what {
                What::Translation => index.translations.get(&target.domain).is_some_and(|keys| keys.contains_key(new)),
                _ => index.services.contains_key(new) || index.autowired.contains_key(new),
            };
            if taken {
                return Err(refuse(format!("{new} already exists. Choose another name.")));
            }
            Ok(None)
        }
    }
}

/// The replacement text of each spot, or why a nested YAML key can't take the new name.
fn replacements(root: &str, changes: &[Change], new: &str) -> Result<Vec<Vec<(Bytes<usize>, String)>>, Error> {
    changes
        .iter()
        .map(|change| {
            change
                .spots
                .iter()
                .map(|spot| {
                    if spot.parent.is_empty() {
                        return Ok((spot.range.clone(), new.to_owned()));
                    }
                    let rest = new.strip_prefix(&format!("{}.", spot.parent)).filter(|rest| !rest.is_empty()).ok_or_else(|| {
                        refuse(format!("{} nests this key under {}. Keep the {}. prefix in the new key.", relative(root, &change.path), spot.parent, spot.parent))
                    })?;
                    Ok((spot.range.clone(), rest.to_owned()))
                })
                .collect()
        })
        .collect()
}

pub(crate) fn resolve(owner: &Main, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
    let data = action.data.as_deref().and_then(|data| data.strip_prefix(PREFIX)).ok_or(Error::Stale)?;
    let request: Request = serde_json::from_str(data).map_err(|_| Error::Stale)?;
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let current = |owner: &Main| -> Result<(), Error> {
        let index = owner.indexes.get(&request.root).filter(|index| !index.partial && index.settings_current()).ok_or(Error::Stale)?;
        let at = text::offset(&doc.text, request.selection.start).ok_or(Error::Stale)?;
        let same = doc.path == request.path
            && doc.version == request.version
            && project.root()? == request.root
            && project.read_file(&doc.path)? == doc.text
            && target_at(index, doc, at).as_ref() == Some(&request.target);
        if same { Ok(()) } else { Err(Error::Stale) }
    };
    current(owner)?;
    let index = owner.indexes.get(&request.root).ok_or(Error::Stale)?;
    let target = &request.target;
    let prompt = match target.what {
        What::Template => "New template name, such as blog/show.html.twig".to_owned(),
        What::Translation => format!("New translation key in the {} domain", target.domain),
        What::Service => "New service id".to_owned(),
    };
    let new = host::input(&prompt, Some(&target.name))?.ok_or(Error::Cancelled)?;
    let new = new.trim().to_owned();
    let moved = check_new(index, &project, &request.root, target, &new)?;
    let new_name = match &moved {
        Some(_) => new_template(target, &new)?.0,
        None => new.clone(),
    };
    let changes = scan(&project, &request.root, doc, index, target)?;
    let replaced = replacements(&request.root, &changes, &new_name)?;
    let root = &request.root;
    let count: usize = replaced.iter().map(Vec::len).sum();
    let mut preview = format!("Renames {} {} → {new_name}.\n", kind_label(target), target.name);
    if let Some(file) = &moved {
        preview.push_str(&format!("Moves {} → {}.\n", relative(root, &target.file), relative(root, file)));
    }
    preview.push_str(&format!("Changes {count} places in {} files. Undo reverts all of it.\n\n", changes.len()));
    let mut edits = Vec::new();
    for (change, replaced) in changes.iter().zip(&replaced) {
        let mut after = change.text.clone();
        for (range, text) in replaced.iter().rev() {
            after.replace_range(range.clone(), text);
        }
        let lines: Vec<&str> = after.lines().collect();
        for (range, _) in replaced {
            let number = line_number(&change.text, range.start);
            let line: String = lines.get(number - 1).unwrap_or(&"").trim().chars().take(160).collect();
            preview.push_str(&format!("{}:{number}  {line}\n", relative(root, &change.path)));
        }
        let path = match &moved {
            Some(file) if change.path == target.file => file.clone(),
            _ => change.path.clone(),
        };
        let text_edits = replaced.iter().map(|(range, text)| TextEdit { range: range_of(&change.text, range.start, range.end), text: text.clone() }).collect();
        edits.push((path, text_edits));
    }
    if !ui_kit::prompts::confirm_exact(&format!("Rename {} {}?", kind_label(target), target.name), &preview)? {
        return Err(Error::Cancelled);
    }
    current(owner)?;
    if scan(&project, &request.root, doc, index, target)? != changes {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: format!("Rename {} {} → {new_name}", kind_label(target), target.name),
        kind: Some("refactor.rename".into()),
        operations: moved.iter().map(|file| FileOperation::Rename { from: target.file.clone(), to: file.clone() }).collect(),
        edits,
        open: moved,
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::file_actions::FileActions;
    use openidea_plugin::mock::MockHost;

    fn setup(files: &[(&str, &str)]) -> (MockHost, Main) {
        let host = MockHost::install();
        host.add_file("composer.json", r#"{"require":{"symfony/framework-bundle":"^7.0"},"autoload":{"psr-4":{"App\\":"src/"}}}"#);
        for (path, text) in files {
            host.add_file(path, text);
        }
        let mut owner = Main::default();
        owner.activate(&host.context("aritti.symfony")).unwrap();
        (host, owner)
    }

    fn open(host: &MockHost, path: &str) -> Document {
        let text = host.invocation().project().unwrap().read_file(path).unwrap();
        let language = match path.rsplit('.').next().unwrap() {
            "yaml" => "yaml",
            "xml" | "xlf" => "xml",
            other => other,
        };
        Document { path: format!("/project/{path}"), language: language.into(), text, version: 1 }
    }

    fn caret(doc: &Document, needle: &str) -> Range {
        let at = text::position(&doc.text, doc.text.find(needle).unwrap() + 1);
        Range::new(at, at)
    }

    fn offered(owner: &mut Main, doc: &Document, needle: &str) -> Option<FileAction> {
        owner.file_actions(doc, caret(doc, needle)).into_iter().find(handles)
    }

    fn rename(host: &MockHost, owner: &mut Main, doc: &Document, needle: &str, new: &str) -> Result<FileAction, Error> {
        let action = offered(owner, doc, needle).expect("rename offered");
        host.answer_next_prompt(Some(&[new]));
        host.answer_next_prompt(Some(&["yes"]));
        owner.resolve_file_action(doc, action)
    }

    /// A rename expected to fail before the confirmation.
    fn attempt(host: &MockHost, owner: &mut Main, doc: &Document, needle: &str, new: &str) -> Result<FileAction, Error> {
        let action = offered(owner, doc, needle).expect("rename offered");
        host.answer_next_prompt(Some(&[new]));
        owner.resolve_file_action(doc, action)
    }

    /// Each edit replaces a nonempty range holding exactly `old` (UTF-16 positions).
    fn assert_exact(host: &MockHost, result: &FileAction, moved: Option<(&str, &str)>, old: &[&str]) {
        let project = host.invocation().project().unwrap();
        for (path, edits) in &result.edits {
            let source = moved.filter(|(_, to)| to == path).map_or(path.as_str(), |(from, _)| from);
            let text = project.read_file(source).unwrap();
            for edit in edits {
                assert_ne!(edit.range.start, edit.range.end, "{path}: zero-width edit");
                let covered = &text[text::offset(&text, edit.range.start).unwrap()..text::offset(&text, edit.range.end).unwrap()];
                assert!(old.contains(&covered), "{path}: {covered}");
            }
        }
    }

    const CONTROLLER: &str = "<?php\nclass BlogController\n{\n    #[Template('blog/show.html.twig')]\n    public function show() { $x = 'é😀'; return $this->render('blog/show.html.twig', []); }\n    public function block() { return $this->renderBlock('blog/show.html.twig', 'body'); }\n}\n";

    fn template_project() -> (MockHost, Main) {
        let (host, owner) = setup(&[
            ("templates/blog/show.html.twig", "<p>😀 show</p>"),
            ("templates/base.html.twig", "{% extends 'blog/show.html.twig' %}{% include 'blog/show.html.twig' -%}{{ include('blog/show.html.twig') }}{{ source('blog/show.html.twig') }}{% form_theme form 'blog/show.html.twig' %}{% use \"blog/show.html.twig\" %}{% import 'blog/show.html.twig' as m %}{% from 'blog/show.html.twig' import x %}{% embed 'blog/show.html.twig' %}{% endembed %}"),
            ("src/Controller/BlogController.php", CONTROLLER),
            ("config/routes.yaml", "about:\n    path: /about\n    controller: Symfony\\Bundle\\FrameworkBundle\\Controller\\TemplateController\n    defaults:\n        template: blog/show.html.twig\n"),
        ]);
        host.add_dir("templates/post");
        (host, owner)
    }

    #[test]
    fn template_rename_moves_the_file_and_updates_every_exact_reference() {
        let (host, mut owner) = template_project();
        let doc = open(&host, "src/Controller/BlogController.php");
        let result = rename(&host, &mut owner, &doc, "blog/show", "post/show.html.twig").unwrap();
        assert_eq!(result.operations, [FileOperation::Rename { from: "/project/templates/blog/show.html.twig".into(), to: "/project/templates/post/show.html.twig".into() }]);
        assert_eq!(result.open.as_deref(), Some("/project/templates/post/show.html.twig"));
        let count = |suffix: &str| result.edits.iter().find(|(path, _)| path.ends_with(suffix)).map_or(0, |(_, edits)| edits.len());
        assert_eq!(count("BlogController.php"), 3);
        assert_eq!(count("base.html.twig"), 9);
        assert_eq!(count("routes.yaml"), 1);
        assert!(result.edits.iter().flat_map(|(_, edits)| edits).all(|edit| edit.text == "post/show.html.twig"));
        assert_exact(&host, &result, None, &["blog/show.html.twig"]);
        let php = &result.edits.iter().find(|(path, _)| path.ends_with("BlogController.php")).unwrap().1;
        assert_eq!((php[1].range.start.line, php[1].range.start.character, php[1].range.end.character), (4, 63, 82), "UTF-16 columns after é😀");
        assert!(host.invocation().project().unwrap().exists("templates/blog/show.html.twig").unwrap(), "nothing is written before the host applies it");
    }

    #[test]
    fn template_rename_is_offered_in_the_template_itself_and_edits_land_on_the_moved_file() {
        let (host, mut owner) = setup(&[("templates/blog/show.html.twig", "{# self #}{{ include('blog/show.html.twig') }}")]);
        host.add_dir("templates/post");
        let doc = open(&host, "templates/blog/show.html.twig");
        let result = rename(&host, &mut owner, &doc, "self", "post/show.html.twig").unwrap();
        assert_eq!(result.edits[0].0, "/project/templates/post/show.html.twig");
        assert_exact(&host, &result, Some(("templates/blog/show.html.twig", "/project/templates/post/show.html.twig")), &["blog/show.html.twig"]);
    }

    #[test]
    fn dynamic_or_unknown_template_uses_withhold_the_rename() {
        for other in [
            "<?php $name = 'blog/show.html.twig'; $this->render($name);",
            "<?php $this->render('blog/show.html.twig' . $suffix);",
            "<?php // renders blog/show.html.twig\n",
        ] {
            let (host, mut owner) = template_project();
            host.add_file("src/Other.php", other);
            let doc = open(&host, "src/Controller/BlogController.php");
            let error = attempt(&host, &mut owner, &doc, "blog/show", "post/show.html.twig").unwrap_err();
            assert!(matches!(&error, Error::Invalid(message) if message.contains("src/Other.php:")), "{other}: {error:?}");
        }
        let (host, mut owner) = template_project();
        host.add_file("templates/list.html.twig", "{% include ['blog/show.html.twig', 'x.html.twig'] %}");
        let doc = open(&host, "src/Controller/BlogController.php");
        assert!(attempt(&host, &mut owner, &doc, "blog/show", "post/show.html.twig").is_err());
    }

    #[test]
    fn template_targets_keep_their_namespace_and_refuse_collisions_and_missing_folders() {
        let (host, mut owner) = setup(&[
            ("config/packages/twig.yaml", "twig:\n    paths:\n        'admin/views': Admin\n"),
            ("admin/views/dash.html.twig", "dash"),
            ("admin/views/other.html.twig", "other"),
            ("src/C.php", "<?php $this->render('@Admin/dash.html.twig');"),
        ]);
        host.add_dir("admin/views");
        let doc = open(&host, "src/C.php");
        let result = rename(&host, &mut owner, &doc, "@Admin", "board.html.twig").unwrap();
        assert_eq!(result.edits[0].1[0].text, "@Admin/board.html.twig");
        assert!(matches!(&result.operations[0], FileOperation::Rename { to, .. } if to == "/project/admin/views/board.html.twig"));
        for bad in ["@Other/board.html.twig", "../x.html.twig", "board.txt", "other.html.twig", "missing/board.html.twig"] {
            assert!(attempt(&host, &mut owner, &doc, "@Admin", bad).is_err(), "{bad}");
        }
    }

    const FORM: &str = "<?php\nclass PostType extends AbstractType\n{\n    function build($b) { $b->add('title', null, ['label' => 'post.title']); }\n}\n";

    fn translation_project() -> (MockHost, Main) {
        setup(&[
            ("translations/messages.en.yaml", "post:\n    title: Title\n"),
            ("translations/messages.fr.yaml", "post.title: Titre\n"),
            ("translations/messages.de.xlf", "<xliff><file><body><trans-unit id=\"post.title\" resname=\"post.title\"><source>post.title</source><target>Titel</target></trans-unit></body></file></xliff>"),
            ("translations/admin.en.yaml", "post.title: Admin title\n"),
            ("templates/a.html.twig", "{{ 'post.title'|trans }} {{ 'post.title'|trans({}, 'admin') }}"),
            ("src/Form/PostType.php", FORM),
            ("src/Use.php", "<?php\n$t->trans('post.title'); new TranslatableMessage('post.title'); t('post.title', [], 'admin');\n"),
        ])
    }

    #[test]
    fn translation_rename_edits_one_domain_its_definitions_and_usages() {
        let (host, mut owner) = translation_project();
        let doc = open(&host, "src/Use.php");
        let result = rename(&host, &mut owner, &doc, "post.title", "post.heading").unwrap();
        assert!(result.operations.is_empty());
        let edits = |suffix: &str| result.edits.iter().find(|(path, _)| path.ends_with(suffix)).map(|(_, edits)| edits.iter().map(|e| e.text.as_str()).collect::<Vec<_>>());
        assert_eq!(edits("messages.en.yaml"), Some(vec!["heading"]), "nested key keeps its parent");
        assert_eq!(edits("messages.fr.yaml"), Some(vec!["post.heading"]));
        assert_eq!(edits("messages.de.xlf"), Some(vec!["post.heading"; 3]), "id, resname and source");
        assert_eq!(edits("a.html.twig"), Some(vec!["post.heading"]), "the admin domain use stays");
        assert_eq!(edits("Use.php"), Some(vec!["post.heading"; 2]));
        assert_eq!(edits("PostType.php"), Some(vec!["post.heading"]), "form label");
        assert_eq!(edits("admin.en.yaml"), None);
        assert_exact(&host, &result, None, &["post.title", "title"]);
    }

    #[test]
    fn translation_rename_is_offered_on_the_definition_and_guards_nested_prefixes() {
        let (host, mut owner) = translation_project();
        let doc = open(&host, "translations/messages.en.yaml");
        assert!(offered(&mut owner, &doc, "title").unwrap().title.contains("post.title (messages)"));
        let error = attempt(&host, &mut owner, &doc, "title", "article.title").unwrap_err();
        assert!(matches!(error, Error::Invalid(message) if message.contains("Keep the post. prefix")));
        for bad in ["post title", "post..title", "post.title'", "", "post.title"] {
            assert!(attempt(&host, &mut owner, &doc, "title", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn dynamic_translation_uses_or_unknown_domains_withhold_the_key() {
        for (path, text) in [
            ("templates/b.html.twig", "{{ ('post.title' ~ suffix)|trans }}"),
            ("templates/b.html.twig", "{% trans %}post.title{% endtrans %}"),
            ("src/B.php", "<?php $t->trans('post.title', [], $domain);"),
            ("translations/messages.es.php", "<?php return ['post.title' => 'Título'];"),
        ] {
            let (host, mut owner) = translation_project();
            host.add_file(path, text);
            let doc = open(&host, "src/Use.php");
            assert!(attempt(&host, &mut owner, &doc, "post.title", "post.heading").is_err(), "{path}: {text}");
        }
    }

    const SERVICES: &str = "services:\n    app.mailer:\n        class: App\\Mailer\n    app.sender:\n        arguments: ['@app.mailer', !service_closure '@app.mailer']\n    app.alias:\n        alias: app.mailer\n    App\\Mailer: ~\n";

    fn service_project() -> (MockHost, Main) {
        setup(&[
            ("config/services.yaml", SERVICES),
            ("config/services.xml", "<container><services><service id=\"app.x\" class=\"X\"><argument type=\"service\" id=\"app.mailer\"/></service><service id=\"app.y\" alias=\"app.mailer\"/></services></container>"),
            ("src/Use.php", "<?php\nclass Use { function __construct(#[Autowire(service: 'app.mailer')] $m) { $this->container->get('app.mailer'); } }\n"),
            ("config/services.php", "<?php return function ($c) { $c->services()->set('app.z')->args([service('app.mailer')]); };"),
        ])
    }

    #[test]
    fn service_rename_updates_definition_and_every_reference_form() {
        let (host, mut owner) = service_project();
        let doc = open(&host, "config/services.yaml");
        let result = rename(&host, &mut owner, &doc, "app.mailer:", "app.notifier").unwrap();
        let count = |suffix: &str| result.edits.iter().find(|(path, _)| path.ends_with(suffix)).map_or(0, |(_, edits)| edits.len());
        assert_eq!((count("services.yaml"), count("services.xml"), count("Use.php"), count("services.php")), (4, 2, 2, 1));
        assert_exact(&host, &result, None, &["app.mailer"]);
        let first = &result.edits.iter().find(|(path, _)| path.ends_with("services.yaml")).unwrap().1[0];
        assert_eq!((first.range.start.line, first.range.start.character, first.range.end.character), (1, 4, 14));
    }

    #[test]
    fn class_ids_unknown_uses_and_collisions_withhold_service_rename() {
        let (host, mut owner) = service_project();
        let doc = open(&host, "config/services.yaml");
        assert!(offered(&mut owner, &doc, "App\\Mailer: ~").is_none(), "class ids are left to the PHP rename");
        assert!(attempt(&host, &mut owner, &doc, "app.mailer:", "app.sender").is_err());
        host.add_file("src/Other.php", "<?php $id = 'app.mailer';");
        assert!(attempt(&host, &mut owner, &doc, "app.mailer:", "app.notifier").is_err());
    }

    #[test]
    fn cancelled_or_changed_sources_return_no_edits() {
        let (host, mut owner) = service_project();
        let doc = open(&host, "config/services.yaml");
        let action = offered(&mut owner, &doc, "app.mailer:").unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(owner.resolve_file_action(&doc, action), Err(Error::Cancelled)));
        let action = offered(&mut owner, &doc, "app.mailer:").unwrap();
        host.answer_next_prompt(Some(&["app.notifier"]));
        host.answer_next_prompt(None);
        assert!(matches!(owner.resolve_file_action(&doc, action), Err(Error::Cancelled)));
        let action = offered(&mut owner, &doc, "app.mailer:").unwrap();
        host.add_file("config/services.yaml", "services:\n");
        assert!(matches!(owner.resolve_file_action(&doc, action), Err(Error::Stale)));
    }

    #[test]
    fn registered_indexer_publishes_no_template_translation_or_service_anchors() {
        use openidea_plugin::indexing::{Document as Indexed, DocumentIndexer};
        let (host, mut owner) = service_project();
        for path in ["config/services.yaml", "src/Use.php"] {
            let doc = open(&host, path);
            let edges = owner.index(&Indexed { path: doc.path.clone(), text: doc.text.clone(), symbols: Vec::new() });
            for anchor in edges.definitions.iter().chain(&edges.references) {
                assert!(anchor.key.starts_with("route#"), "{path}: {}", anchor.key);
                assert_ne!(anchor.range.start, anchor.range.end);
            }
        }
    }
}
