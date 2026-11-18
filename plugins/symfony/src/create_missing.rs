//! The class named at the caret that doesn't exist yet, and the MakerBundle command that creates it.

use openidea_plugin::{Error, Project, TextEditor, host};

use crate::index::Index;
use crate::refs::{is_identifier, literal_at, word_at};

/// Class-name suffix, maker, and the namespace under `App\` that maker writes to.
const MAKERS: &[(&str, &str, &str)] = &[
    ("Controller", "controller", "Controller\\"),
    ("Command", "command", "Command\\"),
    ("Type", "form", "Form\\"),
    ("Voter", "voter", "Security\\Voter\\"),
    ("Subscriber", "subscriber", "EventSubscriber\\"),
    ("Listener", "listener", "EventListener\\"),
    ("Handler", "message", "MessageHandler\\"),
    ("Message", "message", "Message\\"),
    ("Validator", "validator", "Validator\\"),
    ("Constraint", "validator", "Validator\\"),
    ("Fixtures", "fixtures", "DataFixtures\\"),
    ("Test", "test", "Tests\\"),
];

fn is_class_byte(byte: u8) -> bool {
    is_identifier(byte) || byte == b'\\'
}

/// The fully qualified class written at byte `at`: `Foo::class`, `new Foo`, a type hint, or a
/// namespaced name in a string or a YAML/XML value (`App\Controller\FooController::index`).
pub(crate) fn class_at(language: &str, text: &str, at: usize) -> Option<String> {
    let (mut start, mut end) = word_at(text, at.min(text.len()), is_class_byte);
    if text[..start].ends_with("::") {
        (start, end) = word_at(text, start - 2, is_class_byte);
    }

    let written = text[start..end].replace("\\\\", "\\");
    let name = written.trim_start_matches('\\');
    let short = name.rsplit('\\').next().unwrap_or_default();
    let valid = name.split('\\').all(|part| !part.is_empty() && !part.as_bytes()[0].is_ascii_digit())
        && short.starts_with(|c: char| c.is_ascii_uppercase())
        && short.contains(|c: char| c.is_ascii_lowercase());
    if !valid {
        return None;
    }

    let before = text[..start].trim_end();
    if before.ends_with('$') || before.ends_with("->") {
        return None;
    }

    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let line = text[line_start..].trim_start();
    let code = language == "php" && literal_at(language, text, at).is_none() && !line.starts_with("use ");
    if line.starts_with("namespace ") {
        return None;
    }

    if code {
        return Some(crate::php_text::resolver(text)(&written));
    }

    name.contains('\\').then(|| name.to_owned())
}

/// The `make:*` command and its name argument for a class MakerBundle can create.
pub(crate) fn maker_for(fqcn: &str) -> Option<(&'static str, Option<String>)> {
    let relative = fqcn.strip_prefix("App\\")?;
    let short = relative.rsplit('\\').next().unwrap_or_default();
    let (suffix, maker, namespace) = MAKERS
        .iter()
        .copied()
        .find(|(suffix, _, _)| short.len() > suffix.len() && short.ends_with(suffix))
        .or_else(|| relative.starts_with("Entity\\").then_some(("", "entity", "Entity\\")))?;

    let name = match relative.strip_prefix(namespace) {
        Some(inside) => inside.to_owned(),
        None => format!("\\{fqcn}"),
    };

    Some(match maker {
        "command" => (maker, Some(format!("app:{}", kebab(&short[..short.len() - suffix.len()])))),
        "test" => (maker, None),
        "message" | "validator" if suffix == "Handler" || suffix == "Validator" => (maker, Some(name[..name.len() - suffix.len()].to_owned())),
        _ => (maker, Some(name)),
    })
}

fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// The source file Composer's default `App\` → `src/` (and `App\Tests\` → `tests/`) mapping gives the class.
fn default_path(fqcn: &str) -> Option<String> {
    let relative = fqcn.strip_prefix("App\\")?;
    Some(match relative.strip_prefix("Tests\\") {
        Some(test) => format!("tests/{}.php", test.replace('\\', "/")),
        None => format!("src/{}.php", relative.replace('\\', "/")),
    })
}

/// Runs the maker for the missing class at the caret in a terminal, after a confirm.
/// `false` when the caret names no class MakerBundle can create, or the class exists.
pub(crate) fn make_class(index: Option<&Index>, project: &Project, editor: &TextEditor) -> Result<bool, Error> {
    let document = editor.document()?;
    let caret = editor.selections()?.first().map(|selection| selection.start).unwrap_or_default();
    let Some(at) = openidea_plugin::text::offset(&document.text, caret) else { return Ok(false) };
    let Some(fqcn) = class_at(&editor.language(), &document.text, at) else { return Ok(false) };
    let Some((maker, name)) = maker_for(&fqcn) else { return Ok(false) };

    let indexed = index.is_some_and(|index| index.classes.contains_key(&fqcn));
    let on_disk = default_path(&fqcn).is_some_and(|path| project.exists(&path).unwrap_or(false));
    if indexed || on_disk {
        return Ok(false);
    }

    let mut args = vec!["bin/console".to_owned(), format!("make:{maker}")];
    args.extend(name);
    let short = fqcn.rsplit('\\').next().unwrap_or(&fqcn);
    let body = format!("Runs {} in a terminal.\nMakerBundle may ask more questions there.", args.join(" "));
    if !host::confirm(&format!("Create {short}?"), &body)? {
        return Ok(true);
    }

    crate::console_runner::open_terminal(project, &format!("make:{maker}"), &args)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(language: &str, marked: &str) -> Option<String> {
        let at = marked.find('|').unwrap();
        class_at(language, &marked.replace('|', ""), at)
    }

    #[test]
    fn finds_class_references_in_php_code_and_strings() {
        let header = "<?php\nnamespace App\\Controller;\nuse App\\Form\\PostType;\n";
        assert_eq!(class("php", &format!("{header}$f = $this->createForm(Post|Type::class);")).as_deref(), Some("App\\Form\\PostType"));
        assert_eq!(class("php", &format!("{header}new Admin\\Us|erController();")).as_deref(), Some("App\\Controller\\Admin\\UserController"));
        assert_eq!(class("php", &format!("{header}function a(\\App\\Message\\Sen|dMail $m) {{}}")).as_deref(), Some("App\\Message\\SendMail"));
        assert_eq!(class("php", &format!("{header}$x = 'App\\Controller\\FooController::b|ar';")).as_deref(), Some("App\\Controller\\FooController"));
        assert_eq!(class("php", &format!("{header}$x = 'Pla|in';")), None);
        assert_eq!(class("php", &format!("{header}$this->ren|der();")), None);
        assert_eq!(class("php", &format!("{header}$x = FOO_B|AR;")), None);
    }

    #[test]
    fn finds_controllers_in_route_and_service_files() {
        assert_eq!(class("yaml", "blog:\n    controller: App\\Controller\\Blog|Controller::index\n").as_deref(), Some("App\\Controller\\BlogController"));
        assert_eq!(class("yaml", "x:\n    defaults: { _controller: \"App\\\\Controller\\\\Blog|Controller::index\" }\n").as_deref(), Some("App\\Controller\\BlogController"));
        assert_eq!(class("xml", "<route id=\"a\" controller=\"App\\Controller\\A|Controller::x\"/>").as_deref(), Some("App\\Controller\\AController"));
        assert_eq!(class("xml", "<default key=\"_controller\">App\\Controller\\A|Controller::x</default>").as_deref(), Some("App\\Controller\\AController"));
        assert_eq!(class("yaml", "framework:\n    Sec|ret: x\n"), None);
        assert_eq!(class("php", "<?php\nuse App\\Entity\\Pro|duct;\n").as_deref(), Some("App\\Entity\\Product"));
    }

    #[test]
    fn command_runs_the_maker_for_a_missing_class_and_falls_back_to_the_name_prompt() {
        use openidea_plugin::Plugin;
        let host = openidea_plugin::mock::MockHost::install().with_capabilities(&["project.read", "editor.read", "process:php"]);
        let mut plugin = crate::tests::activated(&host);
        let source = "<?php\nnamespace App\\Controller;\nclass A { function a() { return new Admin\\FooController(); } }\n";
        host.open_editor("/project/src/Controller/A.php", "php", source, &[(2, 50, 2, 50)]);
        host.answer_next_prompt(Some(&["yes"]));
        plugin.run_command("create-template", &host.invocation(), None).unwrap();
        assert_eq!(host.terminals()[0].args, ["bin/console", "make:controller", "Admin\\FooController"]);

        let existing = "<?php\nnamespace App\\Controller;\nclass A { function a() { return new BlogController(); } }\n";
        host.open_editor("/project/src/Controller/A.php", "php", existing, &[(2, 40, 2, 40)]);
        host.answer_next_prompt(None);
        plugin.run_command("create-template", &host.invocation(), None).unwrap();
        assert_eq!(host.terminals().len(), 1);
        assert!(host.notifications().iter().any(|n| n.body == "No missing template or class at the caret."));
    }

    #[test]
    fn picks_the_maker_by_suffix_with_a_namespace_relative_name() {
        let made = |fqcn: &str| maker_for(fqcn).map(|(maker, name)| (maker, name.unwrap_or_default()));
        assert_eq!(made("App\\Controller\\Admin\\FooController"), Some(("controller", "Admin\\FooController".into())));
        assert_eq!(made("App\\Command\\SendWeeklyReportCommand"), Some(("command", "app:send-weekly-report".into())));
        assert_eq!(made("App\\Form\\PostType"), Some(("form", "PostType".into())));
        assert_eq!(made("App\\Security\\Voter\\PostVoter"), Some(("voter", "PostVoter".into())));
        assert_eq!(made("App\\MessageHandler\\SendMailHandler"), Some(("message", "SendMail".into())));
        assert_eq!(made("App\\Validator\\UniqueEmailValidator"), Some(("validator", "UniqueEmail".into())));
        assert_eq!(made("App\\DataFixtures\\AppFixtures"), Some(("fixtures", "AppFixtures".into())));
        assert_eq!(made("App\\Tests\\BlogTest"), Some(("test", String::new())));
        assert_eq!(made("App\\Entity\\Product"), Some(("entity", "Product".into())));
        assert_eq!(made("App\\Other\\FooController"), Some(("controller", "\\App\\Other\\FooController".into())));
        assert_eq!(made("App\\Service\\Mailer"), None);
        assert_eq!(made("Vendor\\FooController"), None);
    }
}
