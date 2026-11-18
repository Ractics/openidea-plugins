//! **Extract to Twig template…**: the selected markup of a template moves to a new template and is
//! replaced with `{{ include('name') }}`, passing the plain variables it prints. Selections that
//! open or close a Twig block or an HTML element without the other half are refused.
use crate::*;
use openidea_plugin::file_actions::{FileAction, FileOperation};

const PREFIX: &str = "twig-extract:";
const MAX_SOURCE: usize = 128 << 10;
const BLOCK_TAGS: [&str; 14] = ["if", "for", "block", "embed", "macro", "with", "apply", "autoescape", "spaceless", "verbatim", "raw", "filter", "sandbox", "cache"];
const VOID_ELEMENTS: [&str; 14] = ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];
const IMPLICIT_VARIABLES: [&str; 5] = ["loop", "app", "_self", "_context", "_charset"];

/// The selection without surrounding whitespace, if it is not empty.
fn trimmed(text: &str, start: usize, end: usize) -> Option<std::ops::Range<usize>> {
    let slice = text.get(start..end)?;
    let from = start + slice.len() - slice.trim_start().len();
    let to = from + slice.trim().len();
    (to > from).then_some(from..to)
}

pub(crate) fn actions(listing: &crate::file_actions::Listing, doc: &Document, selection: Range) -> Vec<FileAction> {
    if doc.language != "twig" || !doc.path.ends_with(".twig") || doc.text.len() > MAX_SOURCE {
        return Vec::new();
    }
    let (Some(start), Some(end)) = (text::offset(&doc.text, selection.start), text::offset(&doc.text, selection.end)) else {
        return Vec::new();
    };
    let Some(range) = trimmed(&doc.text, start, end) else {
        return Vec::new();
    };
    if listing.index.and_then(|index| index.template_of(&doc.path)).is_none() {
        return Vec::new();
    }
    vec![FileAction {
        title: "Extract to Twig template…".into(),
        kind: Some("refactor.extract".into()),
        data: Some(format!("{PREFIX}{}:{}:{}", doc.version, range.start, range.end)),
        ..FileAction::default()
    }]
}

pub(crate) fn handles(action: &FileAction) -> bool {
    action.data.as_deref().is_some_and(|data| data.starts_with(PREFIX))
}

/// The first tag of `{% tag … %}` and whether it assigns (`set x = …`).
fn tag_name(body: &str) -> (&str, bool) {
    let body = body.trim_matches(['-', '~', ' ', '\t', '\r', '\n']);
    let end = body.find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_').unwrap_or(body.len());
    (&body[..end], body.contains('='))
}

/// Why the selection can't stand alone as a template, if it can't.
pub(crate) fn unbalanced(text: &str, range: std::ops::Range<usize>) -> Option<String> {
    let before = &text[..range.start];
    let unclosed = |open: &str, close: &str| before.rfind(open).is_some_and(|opened| before.rfind(close).is_none_or(|closed| closed < opened));
    if unclosed("{{", "}}") || unclosed("{%", "%}") || unclosed("{#", "#}") || before.rfind('<') > before.rfind('>') {
        return Some("The selection starts inside a tag. Select whole tags and elements.".into());
    }

    let selected = &text[range];
    let cut = || Some("The selection ends inside a tag. Select whole tags and elements.".to_owned());
    let mut blocks: Vec<&str> = Vec::new();
    let mut elements: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(found) = selected[at..].find(['{', '<']) {
        let open = at + found;
        let rest = &selected[open..];
        if rest.starts_with("{#") {
            let Some(close) = rest.find("#}") else { return cut() };
            at = open + close + 2;
        } else if rest.starts_with("{{") {
            let Some(close) = file_actions::tag_end(selected, open + 2, "}}") else { return cut() };
            at = close + 2;
        } else if rest.starts_with("{%") {
            let Some(close) = file_actions::tag_end(selected, open + 2, "%}") else { return cut() };
            let (name, assigns) = tag_name(&selected[open + 2..close]);
            at = close + 2;
            if BLOCK_TAGS.contains(&name) || (name == "set" && !assigns) {
                blocks.push(name);
            } else if let Some(ended) = name.strip_prefix("end") {
                if blocks.pop() != Some(ended) {
                    return Some(format!("{{% {name} %}} closes a block opened outside the selection. Select the whole block."));
                }
            } else if matches!(name, "else" | "elseif") && blocks.is_empty() {
                return Some(format!("{{% {name} %}} belongs to a block outside the selection. Select the whole block."));
            }
        } else if rest.starts_with("<!--") {
            let Some(close) = rest.find("-->") else { return cut() };
            at = open + close + 3;
        } else if rest.starts_with('<') {
            let closing = rest.starts_with("</");
            let name: String = rest[if closing { 2 } else { 1 }..].chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-').collect::<String>().to_ascii_lowercase();
            if name.is_empty() {
                at = open + 1;
                continue;
            }
            let Some(close) = file_actions::tag_end(selected, open + 1, ">") else { return cut() };
            let self_closing = selected[..close].ends_with('/');
            at = close + 1;
            if closing {
                if elements.pop().as_deref() != Some(name.as_str()) {
                    return Some(format!("</{name}> closes an element opened outside the selection. Select the whole element."));
                }
            } else if !self_closing && !VOID_ELEMENTS.contains(&name.as_str()) {
                elements.push(name);
            }
        } else {
            at = open + 1;
        }
    }
    if let Some(name) = blocks.last() {
        return Some(format!("{{% {name} %}} has no {{% end{name} %}} in the selection. Select the whole block."));
    }
    elements.last().map(|name| format!("<{name}> isn't closed in the selection. Select the whole element."))
}

/// Root names of the plain `{{ var }}` / `{{ var.prop }}` prints, minus names the selection binds itself.
pub(crate) fn variables(selected: &str, globals: &Names) -> Vec<String> {
    let mut bound: Vec<String> = Vec::new();
    let mut found: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(open) = selected[at..].find(['{']).map(|found| at + found) {
        let tag = selected[open..].starts_with("{%");
        if !tag && !selected[open..].starts_with("{{") {
            at = open + 1;
            continue;
        }
        let Some(close) = file_actions::tag_end(selected, open + 2, if tag { "%}" } else { "}}" }) else { break };
        let body = selected[open + 2..close].trim_matches(['-', '~', ' ', '\t', '\r', '\n']);
        at = close + 2;
        let read = if tag {
            let (name, _) = tag_name(body);
            let rest = &body[name.len()..];
            let (names, read) = match name {
                "for" => rest.split_once(" in ").unwrap_or((rest, "")),
                "set" => (rest.split('=').next().unwrap_or(rest), ""),
                _ => continue,
            };
            bound.extend(names.split([',', ' ']).filter(|word| !word.is_empty()).map(str::to_owned));
            read.trim()
        } else {
            body
        };
        let plain = !read.is_empty() && read.split('.').all(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_') && part.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'));
        let root = read.split('.').next().unwrap_or_default().to_owned();
        if plain && !found.contains(&root) {
            found.push(root);
        }
    }
    found.retain(|name| !bound.contains(name) && !IMPLICIT_VARIABLES.contains(&name.as_str()) && !globals.contains_key(name));
    found
}

/// `blog/_recent_posts.html.twig` for a selection reading "Recent posts" in `blog/index.html.twig`.
fn suggested_name(index: &Index, template: &str, selected: &str) -> String {
    let mut words = String::new();
    let mut depth = 0i32;
    for ch in selected.chars() {
        match ch {
            '<' | '{' => depth += 1,
            '>' | '}' => depth -= 1,
            _ if depth <= 0 => words.push(ch),
            _ => {}
        }
    }
    let slug: Vec<String> = words.split(|ch: char| !ch.is_ascii_alphanumeric()).filter(|word| !word.is_empty()).take(3).map(str::to_lowercase).collect();
    let slug = if slug.is_empty() { "partial".to_owned() } else { slug.join("_") };
    let folder = template.rsplit_once('/').map_or(String::new(), |(folder, _)| format!("{folder}/"));
    let extension = if template.ends_with(".html.twig") { ".html.twig" } else { ".twig" };
    let stem = format!("{folder}_{slug}");
    std::iter::once(format!("{stem}{extension}"))
        .chain((2..20).map(|n| format!("{stem}_{n}{extension}")))
        .find(|name| !index.templates.contains_key(name))
        .unwrap_or(format!("{stem}{extension}"))
}

/// The selection with the indentation of its first line removed from every line.
fn dedented(text: &str, range: std::ops::Range<usize>) -> String {
    let indent = &text[intentions::line_start(text, range.start)..range.start];
    let selected = &text[range.clone()];
    if !indent.trim().is_empty() || indent.is_empty() {
        return selected.to_owned();
    }
    selected.split('\n').map(|line| line.strip_prefix(indent).unwrap_or(line)).collect::<Vec<_>>().join("\n")
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
    if start > end || end > doc.text.len() || !doc.text.is_char_boundary(start) || !doc.text.is_char_boundary(end) {
        return Err(Error::Stale);
    }
    let index = owner.index_for(&doc.path).ok_or(Error::Stale)?;
    let template = index.template_of(&doc.path).ok_or(Error::Stale)?;
    if let Some(reason) = unbalanced(&doc.text, start..end) {
        return Err(reason.into());
    }
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let root = project.root()?;
    let templates_root = doc.path.strip_suffix(template).ok_or(Error::Stale)?;

    let suggestion = suggested_name(index, template, &doc.text[start..end]);
    let Some(name) = host::input("New template name", Some(&suggestion))? else {
        return Err(Error::Cancelled);
    };
    let name = name.trim().to_owned();
    if !file_actions::valid_name(&name) {
        return Err("Use a relative template name ending in .twig, such as blog/_post.html.twig.".into());
    }
    if index.templates.contains_key(&name) {
        return Err(format!("The template {name} already exists. Choose another name.").into());
    }
    let target = format!("{templates_root}{name}");
    file_actions::vacant(&project, &target, &root)?;

    let passed = variables(&doc.text[start..end], &index.twig_globals);
    let include = if passed.is_empty() {
        format!("{{{{ include('{name}') }}}}")
    } else {
        let pairs: Vec<String> = passed.iter().map(|name| format!("{name}: {name}")).collect();
        format!("{{{{ include('{name}', {{{}}}) }}}}", pairs.join(", "))
    };
    let eol = intentions::eol_of(&doc.text);
    let content = format!("{}{eol}", dedented(&doc.text, start..end));
    let preview = format!("Creates {target} with the selection and replaces the selection with\n{include}\nUndo in the editor.\n\n{content}");
    if !ui_kit::prompts::confirm_exact(&format!("Extract to {name}?"), &preview)? {
        return Err(Error::Cancelled);
    }
    Ok(FileAction {
        title: format!("Extract to {name}"),
        kind: Some("refactor.extract".into()),
        operations: vec![FileOperation::Create { path: target, text: content, overwrite: false }],
        edits: vec![(doc.path.clone(), vec![TextEdit { range: range_of(&doc.text, start, end), text: include }])],
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{activated, doc};
    use openidea_plugin::file_actions::FileActions;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/templates/blog/index.html.twig";
    const TEXT: &str = "{% extends 'base.html.twig' %}\n{% block body %}\n    <ul class=\"posts\">\n        {% for post in posts %}\n            <li>{{ post.title }} by {{ author.name }}</li>\n        {% endfor %}\n    </ul>\n{% endblock %}\n";

    fn select(plugin: &mut Main, from: &str, to: &str) -> Option<FileAction> {
        let start = TEXT.find(from).unwrap();
        let end = TEXT.find(to).unwrap() + to.len();
        let selection = Range::new(text::position(TEXT, start), text::position(TEXT, end));
        plugin.file_actions(&doc(PATH, "twig", TEXT), selection).into_iter().find(handles)
    }

    #[test]
    fn extracts_the_selection_into_a_sibling_template_passing_its_variables() {
        let host = MockHost::install();
        host.add_file("templates/blog/index.html.twig", TEXT);
        let mut plugin = activated(&host);
        let action = select(&mut plugin, "<ul", "</ul>").unwrap();
        assert_eq!(action.title, "Extract to Twig template…");

        host.answer_next_prompt(Some(&["blog/_list.html.twig"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc(PATH, "twig", TEXT), action).unwrap();
        let [FileOperation::Create { path, text, overwrite: false }] = resolved.operations.as_slice() else { panic!() };
        assert_eq!(path, "/project/templates/blog/_list.html.twig");
        assert_eq!(text, "<ul class=\"posts\">\n    {% for post in posts %}\n        <li>{{ post.title }} by {{ author.name }}</li>\n    {% endfor %}\n</ul>\n");
        let [(file, edits)] = resolved.edits.as_slice() else { panic!() };
        assert_eq!(file, PATH);
        assert_eq!(edits[0].text, "{{ include('blog/_list.html.twig', {posts: posts, author: author}) }}");
    }

    #[test]
    fn suggests_a_name_from_the_words_and_refuses_unbalanced_selections() {
        let host = MockHost::install();
        host.add_file("templates/blog/index.html.twig", TEXT);
        let mut plugin = activated(&host);
        let index = plugin.index_for(PATH).unwrap();
        assert_eq!(suggested_name(index, "blog/index.html.twig", "<h1>Recent posts here</h1>"), "blog/_recent_posts_here.html.twig");
        assert_eq!(suggested_name(index, "index.twig", "{{ x }}"), "_partial.twig");

        for (from, to, reason) in [
            ("{% for", "</li>", "has no {% endfor %}"),
            ("<li>", "{% endfor %}", "closes a block opened outside"),
            ("<ul", "{% endfor %}", "<ul> isn't closed"),
            ("{% for", "</ul>", "</ul> closes an element"),
            ("post.title", "</li>", "starts inside a tag"),
        ] {
            let action = select(&mut plugin, from, to).unwrap();
            let error = plugin.resolve_file_action(&doc(PATH, "twig", TEXT), action).unwrap_err().to_string();
            assert!(error.contains(reason), "{from}..{to}: {error}");
        }
        assert!(plugin.file_actions(&doc("/project/notes/x.twig", "twig", TEXT), Range::new(Position::new(2, 4), Position::new(2, 9))).into_iter().all(|action| !handles(&action)));
    }

    #[test]
    fn existing_name_and_cancelled_prompts_refuse() {
        let host = MockHost::install();
        host.add_file("templates/blog/index.html.twig", TEXT);
        let mut plugin = activated(&host);
        let action = select(&mut plugin, "<li>", "</li>").unwrap();
        host.answer_next_prompt(Some(&["base.html.twig"]));
        let error = plugin.resolve_file_action(&doc(PATH, "twig", TEXT), action.clone()).unwrap_err().to_string();
        assert!(error.contains("already exists"), "{error}");
        host.answer_next_prompt(None);
        assert!(matches!(plugin.resolve_file_action(&doc(PATH, "twig", TEXT), action), Err(Error::Cancelled)));
    }
}
