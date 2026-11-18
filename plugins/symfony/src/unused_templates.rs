//! Twig templates of `templates/` that no PHP, Twig or config file names.

use std::collections::BTreeSet;

use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Project, host};

use crate::refs::{self, Kind, is_identifier};
use crate::{Main, SOURCE};

/// Controller and mailer calls whose first argument is a template name.
const RENDER_CALLS: &[&str] = &["->render(", "->renderView(", "->renderBlock(", "->renderBlockView(", "->htmlTemplate(", "->textTemplate("];

/// Bundle overrides, mails, form themes and UX components are named outside the code this reads.
const SKIPPED_DIRECTORIES: &[&str] = &["bundles/", "email/", "emails/", "form/", "components/"];
const SOURCES: &[&str] = &["src/**/*.php", "templates/**/*.twig", "config/**/*.yaml", "config/**/*.yml", "config/**/*.xml", "config/**/*.php"];
/// Files read per pattern.
const MAX_SOURCES: usize = 5000;
const TWIG_TEMPLATE_TAGS: &[&str] = &["extends", "include", "embed", "import", "from", "use", "form_theme"];

pub(crate) struct Unused {
    pub name: String,
    pub maybe: bool,
}

/// Every `….twig` token of `text`, quoted or not.
fn twig_names(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || "_./@!-+".contains(c))).filter(|token| token.len() > 5 && token.ends_with(".twig"))
}

fn starts_with_literal(rest: &str) -> bool {
    let rest = rest.trim_start();
    rest.starts_with(['\'', '"', '[', ')']) || rest.starts_with("_self")
}

/// The source names templates at run time: a render call or Twig include whose name is not a string.
fn builds_names(language: &str, text: &str) -> bool {
    if language == "php" {
        return RENDER_CALLS.iter().any(|call| text.match_indices(call).any(|(at, _)| !starts_with_literal(&text[at + call.len()..])));
    }

    if language != "twig" {
        return false;
    }

    let function = ["include(", "source("].iter().any(|call| {
        text.match_indices(call).any(|(at, _)| {
            let named = text[..at].bytes().last().is_some_and(|byte| is_identifier(byte) || byte == b'.');
            !named && !starts_with_literal(&text[at + call.len()..])
        })
    });
    function
        || refs::twig_tags(text).any(|(tag, rest, _)| {
            let rest = if tag == "form_theme" { rest.trim_start().split_once(char::is_whitespace).map_or("", |(_, theme)| theme) } else { rest };
            TWIG_TEMPLATE_TAGS.contains(&tag) && !rest.trim().is_empty() && !starts_with_literal(rest)
        })
}

/// What the sources name: exact template names, prefixes of names built at run time, and
/// whether any source builds names at run time.
#[derive(Default)]
struct Usage {
    used: BTreeSet<String>,
    prefixes: BTreeSet<String>,
    dynamic: bool,
}

impl Usage {
    /// Folds one source in, so its text can be dropped.
    fn add(&mut self, language: &str, text: &str) {
        self.used.extend(twig_names(text).map(str::to_owned));
        self.dynamic |= builds_names(language, text);
        if !matches!(language, "php" | "twig") {
            return;
        }

        for reference in refs::references(language, text).into_iter().filter(|reference| reference.kind == Kind::Template) {
            let name = &text[reference.start..reference.end];
            if name.contains(['$', '{', '#']) || !name.ends_with(".twig") {
                self.prefixes.insert(name.split(['$', '{', '#']).next().unwrap_or_default().to_owned());
                self.dynamic = true;
            }
        }
    }

    /// Templates of `templates` no source names. `maybe` is set when some source builds
    /// template names at run time.
    fn unused(&self, templates: &BTreeSet<&str>) -> Vec<Unused> {
        templates
            .iter()
            .filter(|name| !self.used.contains(**name))
            .filter(|name| !SKIPPED_DIRECTORIES.iter().any(|directory| name.starts_with(directory)))
            .filter(|name| {
                let file = name.rsplit('/').next().unwrap_or_default();
                file != "base.html.twig" && !file.starts_with("error") && !(self.dynamic && file.starts_with('_'))
            })
            .filter(|name| !self.prefixes.iter().any(|prefix| !prefix.is_empty() && name.starts_with(prefix.as_str())))
            .map(|name| Unused { name: (*name).to_owned(), maybe: self.dynamic })
            .collect()
    }
}

/// Reads every source once. Refuses when a pattern matches more than `limit` files: templates
/// named only in the unread files would show as unused.
fn read_sources(project: &Project, limit: usize) -> Result<Usage, Error> {
    let mut usage = Usage::default();
    for glob in SOURCES {
        let language = glob.rsplit('.').next().unwrap_or_default();
        let language = if language == "yml" { "yaml" } else { language };
        let paths = project.find_files(glob, (limit + 1) as u32)?;
        if paths.len() > limit {
            return Err(format!("More than {limit} files match {glob}, so some template uses can't be read. Nothing was listed.").as_str().into());
        }
        for path in paths {
            if let Ok(text) = project.read_file(&path) {
                usage.add(language, &text);
            }
        }
    }
    Ok(usage)
}

pub(crate) fn run(main: &Main, ctx: &Invocation) -> Result<Option<String>, Error> {
    let project = ctx.project().ok_or("Open a project first.")?;
    let index = main.view_index(ctx).ok_or("Symfony is still indexing. Try again in a moment.")?;
    let _task = host::begin_task("Finding unused templates", false);
    let usage = read_sources(&project, MAX_SOURCES)?;

    let own = format!("{}/templates/", project.root()?.trim_end_matches('/'));
    let templates: BTreeSet<&str> =
        index.templates.iter().filter(|(name, entry)| !name.starts_with('@') && entry.path.starts_with(&own)).map(|(name, _)| name.as_str()).collect();
    let unused = usage.unused(&templates);
    if unused.is_empty() {
        host::notify(NotificationLevel::Info, SOURCE, "Every Twig template is used.");
        return Ok(None);
    }

    let choices: Vec<PickItem> = unused
        .iter()
        .map(|template| {
            let item = PickItem::new(&template.name, &template.name);
            if template.maybe { item.description("maybe used: the project builds template names at run time") } else { item }
        })
        .collect();
    let Some(name) = host::pick(&format!("{} unused templates (choose one to open)", unused.len()), &choices)? else {
        return Ok(None);
    };

    let entry = index.templates.get(&name).ok_or_else(|| format!("{name} is no longer indexed. Run Symfony: Reindex Project."))?;
    host::open_location(&entry.path, 0, 0)?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(templates: &[&str], sources: &[(&str, &str)]) -> Vec<(String, bool)> {
        let templates: BTreeSet<&str> = templates.iter().copied().collect();
        let mut usage = Usage::default();
        for (language, text) in sources {
            usage.add(language, text);
        }
        usage.unused(&templates).into_iter().map(|unused| (unused.name, unused.maybe)).collect()
    }

    #[test]
    fn command_opens_the_chosen_template_or_says_all_are_used() {
        use openidea_plugin::Plugin;
        let host = openidea_plugin::mock::MockHost::install();
        let mut plugin = crate::tests::activated(&host);
        plugin.run_command("unused-templates", &host.invocation(), None).unwrap();
        assert!(host.notifications().iter().any(|n| n.body == "Every Twig template is used."));

        host.add_file("templates/orphan.html.twig", "<p>x</p>\n");
        plugin.index_all();
        host.answer_next_prompt(Some(&["orphan.html.twig"]));
        plugin.run_command("unused-templates", &host.invocation(), None).unwrap();
        assert_eq!(host.opened_locations(), [("/project/templates/orphan.html.twig".to_owned(), 0, 0)]);
    }

    #[test]
    fn a_pattern_over_the_file_limit_refuses_instead_of_listing_false_unused() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file("templates/a.html.twig", "");
        host.add_file("templates/b.html.twig", "");
        host.add_file("templates/c.html.twig", "");
        let project = host.invocation().project().unwrap();
        let error = read_sources(&project, 2).err().expect("refused");
        assert!(error.to_string().contains("More than 2 files match templates/**/*.twig"), "{error}");
        assert!(read_sources(&project, 3).is_ok());
    }

    #[test]
    fn templates_nobody_names_are_listed() {
        let found = names(
            &["blog/index.html.twig", "layout.html.twig", "part/nav.html.twig", "theme/fields.html.twig", "static/about.html.twig", "old.html.twig"],
            &[
                ("php", "<?php\nreturn $this->render('blog/index.html.twig');\n"),
                ("twig", "{% extends 'layout.html.twig' %}\n{{ include(\"part/nav.html.twig\") }}\n{% form_theme form 'theme/fields.html.twig' %}\n"),
                ("yaml", "about:\n    path: /about\n    controller: Symfony\\Bundle\\FrameworkBundle\\Controller\\TemplateController\n    defaults:\n        template: static/about.html.twig\n"),
            ],
        );
        assert_eq!(found, [("old.html.twig".to_owned(), false)]);
    }

    #[test]
    fn framework_templates_are_never_listed() {
        let found = names(
            &["base.html.twig", "bundles/TwigBundle/Exception/error404.html.twig", "emails/welcome.html.twig", "form/theme.html.twig", "error.html.twig", "components/Alert.html.twig", "_partial.html.twig"],
            &[],
        );
        assert_eq!(found, [("_partial.html.twig".to_owned(), false)]);
    }

    #[test]
    fn dynamic_names_make_the_result_a_maybe_and_cover_their_prefix() {
        let found = names(
            &["reports/daily.html.twig", "other.html.twig", "_row.html.twig"],
            &[("php", "<?php\nreturn $this->render('reports/' . $type . '.html.twig');\n")],
        );
        assert_eq!(found, [("other.html.twig".to_owned(), true)]);

        let twig = names(&["a.html.twig"], &[("twig", "{{ include(template) }}\n")]);
        assert_eq!(twig, [("a.html.twig".to_owned(), true)]);
        let tag = names(&["a.html.twig"], &[("twig", "{% include name %}\n")]);
        assert_eq!(tag, [("a.html.twig".to_owned(), true)]);
        let php = names(&["a.html.twig"], &[("php", "<?php\n$this->render($view);\n")]);
        assert_eq!(php, [("a.html.twig".to_owned(), true)]);
    }
}
