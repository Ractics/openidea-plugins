//! Runs the console command the open PHP class defines, or one picked from the index.

use openidea_plugin::{Error, Invocation, PickItem, Position, host};

use crate::Main;

/// Names of the console commands `text` defines: `#[AsCommand('x')]`, `$defaultName = 'x'` and
/// `->setName('x')`, without `|alias` parts.
pub(crate) fn command_names(text: &str) -> Vec<String> {
    let mut found: Vec<(String, Position)> = Vec::new();
    crate::index::php_commands("", text, &mut found);
    let set = text.match_indices("->setName(").filter_map(|(at, call)| {
        let rest = text[at + call.len()..].trim_start();
        let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
        rest[1..].split(quote).next().map(str::to_owned)
    });

    let mut names: Vec<String> = Vec::new();
    for name in found.into_iter().map(|(name, _)| name).chain(set) {
        let name = name.trim_start_matches('|').split('|').next().unwrap_or_default().to_owned();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

pub(crate) fn run(main: &Main, ctx: &Invocation) -> Result<Option<String>, Error> {
    let project = ctx.project().ok_or("Open a project first.")?;
    let in_editor = ctx
        .editor()
        .filter(|editor| editor.language() == "php")
        .and_then(|editor| editor.document().ok())
        .and_then(|document| command_names(&document.text).into_iter().next());

    let name = match in_editor {
        Some(name) => name,
        None => {
            let index = main.view_index(ctx).ok_or("Symfony is still indexing. Try again in a moment.")?;
            if index.commands.is_empty() {
                return Err("No console command class found in the project. Use Symfony: Run Console Command… to pick from bin/console.".into());
            }
            let items: Vec<PickItem> = index
                .commands
                .iter()
                .map(|(name, entry)| PickItem::new(name, name).description(entry.path.rsplit('/').next().unwrap_or_default()))
                .collect();
            let Some(name) = host::pick("Run console command", &items)? else { return Ok(None) };
            name
        }
    };

    let Some(arguments) = host::input(&format!("Arguments for bin/console {name}"), Some(""))? else {
        return Ok(None);
    };
    let mut args = vec!["bin/console".to_owned(), name.clone()];
    args.extend(shell_words::split(&arguments).map_err(|_| Error::Invalid("Unbalanced quote in the arguments.".into()))?);
    crate::console_runner::open_terminal(&project, &format!("console {name}"), &args).map(|()| None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_the_open_command_class_or_a_picked_one_with_arguments() {
        use openidea_plugin::Plugin;
        let host = openidea_plugin::mock::MockHost::install().with_capabilities(&["project.read", "editor.read", "process:php"]);
        let mut plugin = crate::tests::activated(&host);
        assert!(plugin.run_command("run-command-from-class", &host.invocation(), None).unwrap_err().to_string().contains("Run Console Command…"));

        let source = "<?php\n#[AsCommand(name: 'app:import')]\nclass ImportCommand {}\n";
        host.add_file("src/Command/ImportCommand.php", source);
        host.open_editor("/project/src/Command/ImportCommand.php", "php", source, &[(2, 0, 2, 0)]);
        host.answer_next_prompt(Some(&["--force 'a b'"]));
        plugin.run_command("run-command-from-class", &host.invocation(), None).unwrap();
        assert_eq!(host.terminals()[0].args, ["bin/console", "app:import", "--force", "a b"]);

        plugin.index_all();
        host.open_editor("/project/src/Mailer.php", "php", "<?php\nclass Mailer {}\n", &[(1, 0, 1, 0)]);
        host.answer_next_prompt(Some(&["app:import"]));
        host.answer_next_prompt(Some(&[""]));
        plugin.run_command("run-command-from-class", &host.invocation(), None).unwrap();
        assert_eq!(host.terminals()[1].args, ["bin/console", "app:import"]);
    }

    #[test]
    fn reads_the_command_name_of_each_declaration_style() {
        assert_eq!(command_names("<?php\n#[AsCommand(name: 'app:a|alias', description: 'x')]\nclass A {}\n"), ["app:a"]);
        assert_eq!(command_names("<?php\n#[AsCommand('app:b')]\nclass B {}\n"), ["app:b"]);
        assert_eq!(command_names("<?php\nclass C { protected static $defaultName = 'app:c'; }\n"), ["app:c"]);
        assert_eq!(command_names("<?php\nclass D { function configure() { $this->setName(\"app:d\")->setDescription('x'); } }\n"), ["app:d"]);
        assert!(command_names("<?php\nclass E {}\n").is_empty());
    }
}
