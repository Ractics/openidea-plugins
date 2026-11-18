//! Symfony: New Symfony Project… creates a project with the symfony CLI, or Composer without it,
//! in a terminal, then offers to open it.

use openidea_plugin::{Error, Event, EventKind, NotificationLevel, PickItem, host};

use crate::SOURCE;

/// Terminal keys are `<prefix><next step>:<destination>`, so the step survives a reclaimed instance.
// ponytail: the host caps keys at 128 bytes, which limits the destination to about 100 characters.
const KEY_PREFIX: &str = "symfony-new:";
const MAX_KEY: usize = 128;

fn valid_name(name: &str) -> bool {
    name.len() <= 80
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// `(program, args, next step)` that creates `destination`: `open` when it is done, `webapp` when
/// Composer still has to add the web app pack.
pub(crate) fn plan(symfony_cli: bool, webapp: bool, destination: &str) -> (&'static str, Vec<String>, &'static str) {
    let destination = destination.to_owned();
    match (symfony_cli, webapp) {
        (true, true) => ("symfony", vec!["new".into(), destination, "--webapp".into()], "open"),
        (true, false) => ("symfony", vec!["new".into(), destination], "open"),
        (false, webapp) => ("composer", vec!["create-project".into(), "symfony/skeleton".into(), destination], if webapp { "webapp" } else { "open" }),
    }
}

fn symfony_cli_available() -> bool {
    host::run_process("symfony", &["version".to_owned(), "--no-ansi".to_owned()], None).is_ok_and(|output| output.status == Some(0))
}

fn launch(step: &str, destination: &str, title: &str, program: &str, args: &[String]) -> Result<(), Error> {
    let key = format!("{KEY_PREFIX}{step}:{destination}");
    let command = host::Command { program: program.to_owned(), args: args.to_vec(), env: Vec::new() };
    host::open_terminal_session(&key, title, &command)
        .map(|_| ())
        .map_err(|error| ui_kit::denied::map(error, "Creating a Symfony project", &format!("process:{program}"), "Symfony"))
}

pub(crate) fn run() -> Result<Option<String>, Error> {
    let Some(parent) = host::pick_directory("New Symfony project: choose the parent folder", None)? else { return Ok(None) };
    let Some(name) = host::input("Project name (letters, numbers, - and _)", Some("symfony-app"))? else { return Ok(None) };
    let name = name.trim();
    if !valid_name(name) {
        return Err("Use a project name of letters, numbers, dashes or underscores that starts with a letter or number.".into());
    }

    let destination = format!("{}/{name}", parent.trim_end_matches(['/', '\\']));
    if KEY_PREFIX.len() + "webapp:".len() + destination.len() > MAX_KEY {
        return Err("The project folder path is too long (over 100 characters). Choose a parent folder with a shorter path.".into());
    }

    let kinds = [
        PickItem::new("webapp", "Web application").description("Twig, Doctrine, forms and security"),
        PickItem::new("skeleton", "Microservice, console or API").description("The minimal skeleton"),
    ];
    let Some(kind) = host::pick("Symfony project type", &kinds)? else { return Ok(None) };
    let webapp = kind == "webapp";

    let (program, args, step) = plan(symfony_cli_available(), webapp, &destination);
    let then = if step == "webapp" { format!("\nThen runs composer require webapp in {destination}.") } else { String::new() };
    let body = format!("Runs {program} {} in a terminal.{then}\nYou can open the project when it finishes.", args.join(" "));
    if !host::confirm(&format!("Create Symfony project {name}?"), &body)? {
        return Ok(None);
    }

    let _ = host::subscribe(EventKind::TerminalExited);
    launch(step, &destination, &format!("New Symfony project: {name}"), program, &args)?;
    Ok(None)
}

/// Runs the next step of a project creation whose terminal ended.
pub(crate) fn handle_events(events: &[Event]) {
    for exit in events.iter().filter_map(Event::terminal_exited) {
        let Some((step, destination)) = exit.key.strip_prefix(KEY_PREFIX).and_then(|rest| rest.split_once(':')) else { continue };
        if exit.closed {
            continue;
        }
        if exit.status != Some(0) {
            host::notify(NotificationLevel::Warning, SOURCE, &format!("Creating the Symfony project in {destination} failed. Check the terminal output."));
            continue;
        }

        let name = destination.rsplit(['/', '\\']).next().unwrap_or(destination);
        let result = match step {
            "webapp" => {
                let args = ["require".to_owned(), "webapp".to_owned(), format!("--working-dir={destination}")];
                launch("open", destination, &format!("New Symfony project: {name}"), "composer", &args)
            }
            _ => host::confirm(&format!("Open {name}?"), &format!("The Symfony project is ready in {destination}."))
                .and_then(|open| if open { host::open_project(destination, true) } else { Ok(()) }),
        };
        if let Err(error) = result {
            host::notify(NotificationLevel::Warning, SOURCE, &format!("The Symfony project is in {destination}, but the next step failed: {error}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::ProcessOutput;
    use openidea_plugin::mock::MockHost;

    fn answers(host: &MockHost, name: &str, kind: &str) {
        host.answer_next_prompt(Some(&["/work"]));
        host.answer_next_prompt(Some(&[name]));
        host.answer_next_prompt(Some(&[kind]));
        host.answer_next_prompt(Some(&["yes"]));
    }

    #[test]
    fn symfony_cli_creates_a_web_app_and_offers_to_open_it() {
        let host = MockHost::install().with_capabilities(&["process:symfony"]);
        host.on_process("symfony", |_, _| Ok(ProcessOutput { status: Some(0), stdout: "Symfony CLI 5".into(), stderr: String::new() }));
        answers(&host, "shop", "webapp");
        run().unwrap();

        let exit = host.finish_terminal("symfony-new:open:/work/shop").unwrap();
        host.answer_next_prompt(Some(&["yes"]));
        handle_events(&[exit]);
        assert_eq!(host.opened_projects(), [("/work/shop".to_owned(), true)]);
    }

    #[test]
    fn composer_creates_the_skeleton_then_adds_the_web_app_pack() {
        let host = MockHost::install().with_capabilities(&["process:composer"]);
        answers(&host, "shop", "webapp");
        run().unwrap();

        let exit = host.finish_terminal("symfony-new:webapp:/work/shop").unwrap();
        handle_events(&[exit]);
        assert!(host.opened_projects().is_empty());
        let next = host.finish_terminal("symfony-new:open:/work/shop").unwrap();
        host.answer_next_prompt(None);
        handle_events(&[next]);
        assert!(host.opened_projects().is_empty());
    }

    #[test]
    fn invalid_names_and_cancelled_confirms_start_nothing() {
        let host = MockHost::install().with_capabilities(&["process:composer"]);
        host.answer_next_prompt(Some(&["/work"]));
        host.answer_next_prompt(Some(&["../x"]));
        assert!(run().unwrap_err().to_string().contains("project name"));

        host.answer_next_prompt(Some(&["/work"]));
        host.answer_next_prompt(Some(&["api"]));
        host.answer_next_prompt(Some(&["skeleton"]));
        host.answer_next_prompt(None);
        run().unwrap();
        assert!(host.finish_terminal("symfony-new:open:/work/api").is_none());
    }

    #[test]
    fn plans_for_each_tool_and_type() {
        assert_eq!(plan(true, false, "/w/a"), ("symfony", vec!["new".into(), "/w/a".into()], "open"));
        assert_eq!(plan(false, false, "/w/a").2, "open");
        assert_eq!(plan(false, true, "/w/a").1, ["create-project", "symfony/skeleton", "/w/a"]);
    }

    #[test]
    fn a_denied_installer_names_the_permission() {
        let host = MockHost::install().with_capabilities(&[]);
        answers(&host, "shop", "skeleton");
        let error = run().unwrap_err().to_string();
        assert!(error.contains("process:composer permission"), "{error}");
    }
}
