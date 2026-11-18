//! Static links from a hovered Symfony name to its symfony.com documentation page.
//! No project facts: the link depends only on the text under the cursor.
use crate::*;
use openidea_plugin::hover::Hover;

const DOCS: &str = "https://symfony.com/doc/current/";
const TWIG_REFERENCE: &str = "reference/twig_reference.html";
const TWIG_COMPONENTS: &str = "https://symfony.com/bundles/ux-twig-component/current/index.html";

pub(crate) struct Link {
    title: &'static str,
    url: String,
}

impl Link {
    fn page(title: &'static str, page: &str) -> Self {
        let url = if page.starts_with("https://") { page.to_owned() } else { format!("{DOCS}{page}") };
        Link { title, url }
    }

    /// The trailing markdown line, placed last like the Laravel hover's documentation link.
    pub(crate) fn markdown(&self) -> String {
        format!("\n\n[Symfony docs: {}]({})", self.title, self.url)
    }
}

pub(crate) fn for_kind(kind: Kind, encore: bool) -> Option<Link> {
    Some(match kind {
        Kind::Route => Link::page("Routing", "routing.html"),
        Kind::Template => Link::page("Templates", "templates.html"),
        Kind::Service => Link::page("Service container", "service_container.html"),
        Kind::Translation => Link::page("Translation", "translation.html"),
        Kind::Role => Link::page("Security", "security.html"),
        Kind::Component => Link::page("Twig components", TWIG_COMPONENTS),
        Kind::Asset => Link::page("AssetMapper", "frontend/asset_mapper.html"),
        Kind::Entry if encore => Link::page("Webpack Encore", "frontend/encore/index.html"),
        Kind::Entry => Link::page("AssetMapper", "frontend/asset_mapper.html"),
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum TwigName {
    Function,
    Filter,
    Tag,
}

const TWIG_FUNCTIONS: &[&str] = &[
    "absolute_url", "access_decision", "access_decision_for_user", "asset", "asset_version", "controller", "csrf_token",
    "dns_prefetch", "dump", "expression", "fragment_uri", "impersonation_exit_path", "impersonation_exit_url",
    "impersonation_path", "impersonation_url", "importmap", "is_granted", "is_granted_for_user", "link", "logout_path",
    "logout_url", "path", "preconnect", "prefetch", "preload", "prerender", "relative_path", "render", "render_esi",
    "render_ssi", "t", "url", "workflow_can", "workflow_has_marked_place", "workflow_marked_places", "workflow_metadata",
    "workflow_transition", "workflow_transition_blockers", "workflow_transitions",
];
const TWIG_FILTERS: &[&str] = &[
    "abbr_class", "abbr_method", "emojify", "file_excerpt", "file_link", "file_relative", "format_args",
    "format_args_as_text", "format_file", "format_file_from_text", "humanize", "trans", "sanitize_html", "serialize",
    "yaml_dump", "yaml_encode",
];
/// Tags whose anchor differs from the same-named function or filter.
const TWIG_TAGS: &[(&str, &str)] =
    &[("dump", "dump-1"), ("form_theme", "form-theme"), ("stopwatch", "stopwatch"), ("trans", "trans-1"), ("trans_default_domain", "trans-default-domain")];

fn twig_link(name: &str, kind: TwigName) -> Option<Link> {
    let anchor = |anchor: &str| Some(Link::page("Twig extensions", &format!("{TWIG_REFERENCE}#{anchor}")));
    match (kind, name) {
        (TwigName::Function | TwigName::Tag, "include") => Some(Link::page("Templates", "templates.html#including-templates")),
        (TwigName::Tag, "extends") | (TwigName::Function | TwigName::Tag, "block") => {
            Some(Link::page("Templates", "templates.html#template-inheritance-and-layouts"))
        }
        (TwigName::Function | TwigName::Tag, "component") => Some(Link::page("Twig components", TWIG_COMPONENTS)),
        (TwigName::Function, "encore_entry_link_tags" | "encore_entry_script_tags") => {
            Some(Link::page("Webpack Encore", "frontend/encore/index.html"))
        }
        (TwigName::Function, form) if form == "form" || form.starts_with("form_") || form.starts_with("field_") => {
            anchor("form-related-functions")
        }
        (TwigName::Function, _) if TWIG_FUNCTIONS.contains(&name) => anchor(&name.replace('_', "-")),
        (TwigName::Filter, _) if TWIG_FILTERS.contains(&name) => anchor(&name.replace('_', "-")),
        (TwigName::Tag, _) => TWIG_TAGS.iter().find(|(tag, _)| *tag == name).and_then(|(_, id)| anchor(id)),
        _ => None,
    }
}

/// Twig function, filter or tag name under the cursor, in Twig code outside strings and comments.
fn twig_hover(doc: &Document, at: usize) -> Option<(usize, usize, String, Link)> {
    let text = &doc.text;
    let (start, end) = refs::word_at(text, at, refs::is_identifier);
    if start == end || at >= end {
        return None;
    }
    let code = refs::twig_code_start(text, start)?;
    if !twig::code_regions(text).iter().any(|region| region.start <= code && end <= region.end)
        || ownership::twig_strings(text)?.iter().any(|literal| literal.start <= start && end <= literal.end)
    {
        return None;
    }
    let name = &text[start..end];
    let before = text[code + 2..start].trim_end_matches([' ', '\t']);
    let kind = if before.trim_start_matches(['-', '~', ' ', '\t']).is_empty() && text[code..].starts_with("{%") {
        TwigName::Tag
    } else if before.ends_with('|') {
        TwigName::Filter
    } else if text[end..].trim_start_matches([' ', '\t']).starts_with('(') && !before.ends_with('.') {
        TwigName::Function
    } else {
        return None;
    };
    let label = match kind {
        TwigName::Function => "Twig function",
        TwigName::Filter => "Twig filter",
        TwigName::Tag => "Twig tag",
    };
    Some((start, end, format!("**{label}** {name}"), twig_link(name, kind)?))
}

/// Pages under `reference/configuration/`, plus bundles documented elsewhere.
fn config_link(key: &str) -> Option<Link> {
    Some(match key {
        "framework" => Link::page("FrameworkBundle configuration", "reference/configuration/framework.html"),
        "doctrine" => Link::page("DoctrineBundle configuration", "reference/configuration/doctrine.html"),
        "security" => Link::page("SecurityBundle configuration", "reference/configuration/security.html"),
        "twig" => Link::page("TwigBundle configuration", "reference/configuration/twig.html"),
        "monolog" => Link::page("MonologBundle configuration", "reference/configuration/monolog.html"),
        "web_profiler" => Link::page("WebProfilerBundle configuration", "reference/configuration/web_profiler.html"),
        "debug" => Link::page("DebugBundle configuration", "reference/configuration/debug.html"),
        "twig_component" => Link::page("Twig components", TWIG_COMPONENTS),
        "webpack_encore" => Link::page("Webpack Encore", "frontend/encore/index.html"),
        _ => return None,
    })
}

fn yaml_hover(doc: &Document, at: usize) -> Option<(usize, usize, String, Link)> {
    let text = &doc.text;
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..text[at..].find('\n').map_or(text.len(), |i| at + i)];
    let column = at - line_start;
    if doc.path.contains("/config/packages/") {
        let (start, end) = refs::word_at(line, column, refs::is_identifier);
        if start == 0 && end > column && line[end..].starts_with(':') {
            let link = config_link(&line[..end])?;
            return Some((line_start, line_start + end, format!("**Configuration** {}", &line[..end]), link));
        }
    }
    if !doc.path.contains("/config/") || line.trim_start().starts_with('#') {
        return None;
    }
    let mut from = 0;
    while let Some(found) = line[from..].find("%env(") {
        let start = from + found;
        let end = start + line[start..].find(")%")? + 2;
        if (start..end).contains(&column) {
            let inner = &line[start + 5..end - 2];
            let name = inner.rsplit(':').next().unwrap_or(inner);
            if line[..start].contains(" #") || name.is_empty() || !name.bytes().all(refs::is_identifier) {
                return None;
            }
            let link = Link::page("Environment variable processors", "configuration/env_var_processors.html");
            return Some((line_start + start, line_start + end, format!("**Environment variable** {name}"), link));
        }
        from = end;
    }
    None
}

/// Namespaces of PHP classes and attributes, longest prefixes first where they overlap.
const PHP_NAMESPACES: &[(&str, &str, &str)] = &[
    ("Symfony\\Component\\HttpKernel\\Event\\", "Events and event listeners", "event_dispatcher.html"),
    ("Symfony\\Component\\HttpKernel\\KernelEvents", "Events and event listeners", "event_dispatcher.html"),
    ("Symfony\\Component\\EventDispatcher\\", "Events and event listeners", "event_dispatcher.html"),
    ("Symfony\\Contracts\\EventDispatcher\\", "Events and event listeners", "event_dispatcher.html"),
    ("Symfony\\Component\\Routing\\", "Routing", "routing.html"),
    ("Symfony\\Bridge\\Twig\\Attribute\\Template", "Templates", "templates.html"),
    ("Symfony\\Component\\DependencyInjection\\", "Service container", "service_container.html"),
    ("Symfony\\Component\\Translation\\", "Translation", "translation.html"),
    ("Symfony\\Contracts\\Translation\\", "Translation", "translation.html"),
    ("Symfony\\Component\\Form\\", "Forms", "forms.html"),
    ("Symfony\\Component\\Console\\", "Console commands", "console.html"),
    ("Symfony\\Component\\Security\\", "Security", "security.html"),
    ("Symfony\\Bundle\\SecurityBundle\\", "Security", "security.html"),
    ("Symfony\\Component\\Messenger\\", "Messenger", "messenger.html"),
    ("Symfony\\Component\\AssetMapper\\", "AssetMapper", "frontend/asset_mapper.html"),
    ("Symfony\\WebpackEncoreBundle\\", "Webpack Encore", "frontend/encore/index.html"),
    ("Symfony\\UX\\TwigComponent\\", "Twig components", TWIG_COMPONENTS),
    ("Doctrine\\ORM\\", "Databases and Doctrine ORM", "doctrine.html"),
    ("Doctrine\\Persistence\\", "Databases and Doctrine ORM", "doctrine.html"),
    ("Doctrine\\Bundle\\DoctrineBundle\\", "Databases and Doctrine ORM", "doctrine.html"),
];

fn php_link(fqcn: &str) -> Option<Link> {
    PHP_NAMESPACES.iter().find(|(prefix, _, _)| fqcn.starts_with(prefix)).map(|(_, title, page)| Link::page(title, page))
}

/// A class or attribute name in PHP code, resolved through the file's `use` imports.
fn php_hover(doc: &Document, at: usize) -> Option<(usize, usize, String, Link)> {
    let masked = php_text::mask(&doc.text);
    let (start, end) = refs::word_at(&masked, at, |b| refs::is_identifier(b) || b == b'\\');
    if start == end || at >= end {
        return None;
    }
    let before = masked[..start].trim_end();
    if before.ends_with(['$', '>', ':']) || masked.as_bytes()[start].is_ascii_digit() {
        return None;
    }
    let name = &masked[start..end];
    let line_start = masked[..start].rfind('\n').map_or(0, |i| i + 1);
    let fqcn = if masked[line_start..start].trim_start().starts_with("use ") {
        name.trim_start_matches('\\').to_owned()
    } else {
        php_text::resolver(&doc.text)(name)
    };
    let link = php_link(&fqcn)?;
    Some((start, end, format!("**{}**", fqcn.replace('\\', "\\\\")), link))
}

/// A documentation-only hover for names that have no cached declaration hover.
pub(crate) fn hover(doc: &Document, at: usize) -> Option<Hover> {
    if doc.text.len() > ownership::MAX_TEXT || !doc.text.is_char_boundary(at) {
        return None;
    }
    if let Some(r) = ownership::reference(doc, at) {
        let name = &doc.text[r.start..r.end];
        if is_dynamic(name) || matches!(r.kind, Kind::Route | Kind::Service | Kind::Template | Kind::Translation) {
            return None;
        }
        let line_start = doc.text[..r.start].rfind('\n').map_or(0, |i| i + 1);
        let link = for_kind(r.kind, doc.text[line_start..r.start].contains("encore_entry_"))?;
        let label = match r.kind {
            Kind::Role => "Role",
            Kind::Component => "Twig component",
            Kind::Asset => "Asset",
            _ => "Entry point",
        };
        let body = format!("**{label}** {}{}", hover_escaped(name), link.markdown());
        return Some(Hover::new(body).range(range_of(&doc.text, r.start, r.end)));
    }
    let (start, end, header, link) = match doc.language.as_str() {
        "twig" => twig_hover(doc, at)?,
        "yaml" => yaml_hover(doc, at)?,
        "php" => php_hover(doc, at)?,
        _ => return None,
    };
    Some(Hover::new(format!("{header}{}", link.markdown())).range(range_of(&doc.text, start, end)))
}

fn hover_escaped(value: &str) -> String {
    value
        .chars()
        .take(512)
        .flat_map(|c| if "\\`*_{}[]<>#|!".contains(c) { vec!['\\', c] } else if c.is_control() { vec![' '] } else { vec![c] })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover_at(marked: &str, path: &str, language: &str) -> Option<String> {
        let at = marked.find('^').unwrap();
        let text = marked.replacen('^', "", 1);
        let doc = Document { path: path.into(), language: language.into(), version: 1, text };
        hover(&doc, at).map(|h| h.markdown)
    }

    fn url_of(markdown: &str) -> &str {
        let start = markdown.rfind("](").unwrap() + 2;
        &markdown[start..markdown.len() - 1]
    }

    #[test]
    fn declaration_kinds_map_to_their_guides() {
        for (kind, url) in [
            (Kind::Route, "https://symfony.com/doc/current/routing.html"),
            (Kind::Template, "https://symfony.com/doc/current/templates.html"),
            (Kind::Service, "https://symfony.com/doc/current/service_container.html"),
            (Kind::Translation, "https://symfony.com/doc/current/translation.html"),
        ] {
            assert_eq!(for_kind(kind, false).unwrap().url, url);
        }
        assert_eq!(for_kind(Kind::Route, false).unwrap().markdown(), "\n\n[Symfony docs: Routing](https://symfony.com/doc/current/routing.html)");
        assert!(for_kind(Kind::Block, false).is_none() && for_kind(Kind::Class, false).is_none());
    }

    #[test]
    fn twig_literal_references_without_declaration_hover_link_their_guides() {
        for (marked, url) in [
            ("{{ is_granted('ROLE_AD^MIN') }}", "https://symfony.com/doc/current/security.html"),
            ("{{ asset('img/lo^go.png') }}", "https://symfony.com/doc/current/frontend/asset_mapper.html"),
            ("{{ encore_entry_link_tags('a^pp') }}", "https://symfony.com/doc/current/frontend/encore/index.html"),
            ("{{ importmap('a^pp') }}", "https://symfony.com/doc/current/frontend/asset_mapper.html"),
            ("{{ component('Al^ert') }}", TWIG_COMPONENTS),
        ] {
            let markdown = hover_at(marked, "/p/templates/a.html.twig", "twig").unwrap_or_else(|| panic!("{marked}"));
            assert_eq!(url_of(&markdown), url, "{marked}");
        }
        assert!(hover_at("{{ path('ho^me') }}", "/p/templates/a.html.twig", "twig").is_none());
    }

    #[test]
    fn twig_builtins_link_the_twig_reference_anchor() {
        let reference = "https://symfony.com/doc/current/reference/twig_reference.html";
        for (marked, url) in [
            ("{{ pa^th('home') }}", format!("{reference}#path")),
            ("{{ u^rl('home') }}", format!("{reference}#url")),
            ("{{ ass^et('a.png') }}", format!("{reference}#asset")),
            ("{{ csrf_to^ken('x') }}", format!("{reference}#csrf-token")),
            ("{% if is_gran^ted('ROLE_A') %}", format!("{reference}#is-granted")),
            ("{{ ren^der(controller('A::b')) }}", format!("{reference}#render")),
            ("{{ render(contr^oller('A::b')) }}", format!("{reference}#controller")),
            ("{{ du^mp(x) }}", format!("{reference}#dump")),
            ("{% du^mp x %}", format!("{reference}#dump-1")),
            ("{{ 'k'|tr^ans }}", format!("{reference}#trans")),
            ("{% tr^ans %}k{% endtrans %}", format!("{reference}#trans-1")),
            ("{%- trans_default_dom^ain 'admin' %}", format!("{reference}#trans-default-domain")),
            ("{{ form_wid^get(form.name) }}", format!("{reference}#form-related-functions")),
            ("{{ inclu^de('a.twig') }}", "https://symfony.com/doc/current/templates.html#including-templates".into()),
            ("{% exte^nds 'base.twig' %}", "https://symfony.com/doc/current/templates.html#template-inheritance-and-layouts".into()),
            ("{{ encore_entry_script_t^ags('app') }}", "https://symfony.com/doc/current/frontend/encore/index.html".into()),
        ] {
            let markdown = hover_at(marked, "/p/templates/a.html.twig", "twig").unwrap_or_else(|| panic!("{marked}"));
            assert_eq!(url_of(&markdown), url, "{marked}");
        }
    }

    #[test]
    fn unknown_or_quiet_twig_names_get_no_link() {
        for marked in [
            "{{ my_fun^ction() }}",
            "{{ 'k'|my_fil^ter }}",
            "{{ obj.pa^th('home') }}",
            "{{ pa^th }}",
            "{# {{ pa^th('home') }} #}",
            "{% verbatim %}{{ pa^th('home') }}{% endverbatim %}",
            "<p>pa^th('home')</p>",
            "{{ \"pa^th('home')\" }}",
            "{% fo^r x in y %}",
        ] {
            assert!(hover_at(marked, "/p/templates/a.html.twig", "twig").is_none(), "{marked}");
        }
    }

    #[test]
    fn top_level_bundle_config_keys_link_their_reference() {
        for (key, url) in [
            ("framework", "https://symfony.com/doc/current/reference/configuration/framework.html"),
            ("doctrine", "https://symfony.com/doc/current/reference/configuration/doctrine.html"),
            ("security", "https://symfony.com/doc/current/reference/configuration/security.html"),
            ("twig", "https://symfony.com/doc/current/reference/configuration/twig.html"),
            ("monolog", "https://symfony.com/doc/current/reference/configuration/monolog.html"),
        ] {
            let marked = format!("{}^{}:\n    key: value\n", &key[..1], &key[1..]);
            let markdown = hover_at(&marked, "/p/config/packages/prod/x.yaml", "yaml").unwrap_or_else(|| panic!("{key}"));
            assert_eq!(url_of(&markdown), url);
        }
        for (marked, path) in [
            ("fra^mework:\n", "/p/config/services.yaml"),
            ("framework:\n    sec^ret: x\n", "/p/config/packages/framework.yaml"),
            ("doctrine_mig^rations:\n", "/p/config/packages/doctrine_migrations.yaml"),
            ("when@p^rod:\n", "/p/config/packages/framework.yaml"),
        ] {
            assert!(hover_at(marked, path, "yaml").is_none(), "{marked}");
        }
    }

    #[test]
    fn env_placeholders_link_the_env_var_processors() {
        let url = "https://symfony.com/doc/current/configuration/env_var_processors.html";
        let markdown = hover_at("doctrine:\n    dbal:\n        url: '%env(resolve:DATABASE^_URL)%'\n", "/p/config/packages/doctrine.yaml", "yaml").unwrap();
        assert!(markdown.starts_with("**Environment variable** DATABASE_URL"));
        assert_eq!(url_of(&markdown), url);
        assert!(hover_at("# url: '%env(DATA^BASE_URL)%'\n", "/p/config/packages/doctrine.yaml", "yaml").is_none());
        assert!(hover_at("url: '%env(DATA^BASE_URL)%'\n", "/p/other/doctrine.yaml", "yaml").is_none());
        assert!(hover_at("url: '%env(%DATA^BASE)%'\n", "/p/config/x.yaml", "yaml").is_none());
    }

    #[test]
    fn imported_symfony_and_doctrine_classes_link_their_guides() {
        let source = "<?php\nnamespace App\\Controller;\nuse Symfony\\Component\\Routing\\Attribute\\Route;\nuse Symfony\\Component\\Console\\Attribute\\AsCommand;\nuse Symfony\\Component\\Messenger\\Attribute\\AsMessageHandler;\nuse Symfony\\Component\\EventDispatcher\\Attribute\\AsEventListener;\nuse Symfony\\Component\\Form\\AbstractType;\nuse Symfony\\Component\\Security\\Http\\Attribute\\IsGranted;\nuse Symfony\\Component\\DependencyInjection\\Attribute\\Autowire;\nuse Doctrine\\ORM\\Mapping as ORM;\nuse Doctrine\\Bundle\\DoctrineBundle\\Repository\\ServiceEntityRepository;\nuse Symfony\\UX\\TwigComponent\\Attribute\\AsTwigComponent;\n";
        for (code, url) in [
            ("#[Ro^ute('/')]", "routing.html"),
            ("#[AsComm^and('app:x')]", "console.html"),
            ("#[AsMessage^Handler]", "messenger.html"),
            ("#[AsEvent^Listener]", "event_dispatcher.html"),
            ("class A extends Abstract^Type {}", "forms.html"),
            ("#[IsGr^anted('ROLE_A')]", "security.html"),
            ("function f(#[Auto^wire('@x')] $x) {}", "service_container.html"),
            ("#[ORM\\Ent^ity]", "doctrine.html"),
            ("class R extends ServiceEntity^Repository {}", "doctrine.html"),
            ("#[AsTwig^Component]", TWIG_COMPONENTS),
        ] {
            let markdown = hover_at(&format!("{source}{code}"), "/p/src/A.php", "php").unwrap_or_else(|| panic!("{code}"));
            assert!(url_of(&markdown).ends_with(url), "{code}: {markdown}");
        }
        let markdown = hover_at("<?php\nuse Symfony\\Component\\Routing\\Attri^bute\\Route;", "/p/src/A.php", "php").unwrap();
        assert!(markdown.starts_with("**Symfony\\\\Component\\\\Routing\\\\Attribute\\\\Route**"));
    }

    #[test]
    fn project_classes_members_and_php_strings_get_no_link() {
        let source = "<?php\nnamespace App;\nuse Symfony\\Component\\Routing\\Attribute\\Route;\n";
        for code in ["new Contr^oller();", "$this->ro^ute();", "Route::FO^O;", "$ro^ute;", "// Ro^ute", "'Ro^ute';"] {
            assert!(hover_at(&format!("{source}{code}"), "/p/src/A.php", "php").is_none(), "{code}");
        }
    }
}
