//! Symfony: Add Database to IDE… and Symfony: Manage Database Connections…: the app's Doctrine connections
//! as data sources of the IDE's Database tool window (`database.sources`). The password, URL and
//! driver options never leave the app; with a Docker target, a container host becomes its
//! published port on this machine.
use crate::docker_target::{self, Target};
use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, PluginDataSource, Project, host};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::net::IpAddr;

const FEATURE: &str = "Adding databases to the IDE";
const PREFIX: &str = "symfony-";
const INSPECT_FORMAT: &str = r#"{"Id":{{json .Id}},"Running":{{json .State.Running}},"Project":{{json (index .Config.Labels "com.docker.compose.project")}},"Networks":{{json .NetworkSettings.Networks}},"Ports":{{json .NetworkSettings.Ports}}}"#;

type Docker<'a> = &'a mut dyn FnMut(&Project, &[String]) -> Result<String, Error>;

fn hash(text: &str) -> String {
    format!("{:016x}", text.bytes().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)))
}

fn source_id(root: &str, connection: &str) -> String {
    format!("{PREFIX}{}", hash(&format!("{root}\0{connection}")))
}

fn ours(id: &str) -> bool {
    id.strip_prefix(PREFIX).is_some_and(|hash| hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn denied(error: Error) -> Error {
    ui_kit::denied::map(error, FEATURE, "database.sources", "Symfony")
}

/// Doctrine's driver names as the IDE's.
fn driver(doctrine: &str) -> Option<&'static str> {
    Some(match doctrine {
        "pdo_mysql" | "mysqli" => "mysql",
        "pdo_pgsql" | "pgsql" => "postgres",
        "pdo_sqlite" | "sqlite3" => "sqlite",
        "pdo_sqlsrv" | "sqlsrv" => "sqlserver",
        _ => return None,
    })
}

fn plain(value: &Value, key: &str) -> Option<String> {
    value[key].as_str().filter(|text| !text.is_empty() && text.len() <= 512 && !text.chars().any(char::is_control)).map(str::to_owned)
}

fn loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn default_port(driver: &str) -> Option<u16> {
    match driver {
        "mysql" => Some(3306),
        "postgres" => Some(5432),
        _ => None,
    }
}

/// The published `host:port` on this machine for `host:port` as the app container sees it: the app
/// container itself for a loopback host, else the one Compose service of the same project that answers
/// to that name on a shared network. MySQL and PostgreSQL only; ambiguous answers are refused.
fn published(project: &Project, target: &Target, driver: &str, host: &str, port: Option<u16>, docker: Docker) -> Result<(String, u16), Error> {
    let Some(port) = port.or_else(|| default_port(driver)) else {
        return Err(Error::Invalid("With a Docker target, only MySQL and PostgreSQL databases can be added. Add this one in the Database tool window yourself.".into()));
    };
    let inspect = |id: &str| ["--host", target.socket(), "inspect", "--type", "container", "--format", INSPECT_FORMAT, id].map(str::to_owned).to_vec();
    let app: Value = serde_json::from_str(&docker(project, &inspect(target.container()))?).map_err(|_| Error::Failed("Docker returned an unreadable container description.".into()))?;
    let mut candidates = Vec::new();
    if loopback(host) {
        candidates.push(app);
    } else {
        let compose = plain(&app, "Project").ok_or_else(|| Error::Invalid(format!("The app container is not part of a Compose project, so the database host {host} cannot be found. Add the database in the Database tool window yourself.")))?;
        let args = ["--host", target.socket(), "ps", "--no-trunc", "--filter", &format!("label=com.docker.compose.project={compose}"), "--format", "{{.ID}}"].map(str::to_owned).to_vec();
        let listed = docker(project, &args)?;
        let ids: Vec<&str> = listed.lines().map(str::trim).filter(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())).take(64).collect();
        let networks = |container: &Value| container["Networks"].as_object().map(|map| map.values().cloned().collect::<Vec<_>>()).unwrap_or_default();
        let shared: Vec<Value> = networks(&app).iter().map(|network| network["NetworkID"].clone()).collect();
        for id in ids {
            let peer: Value = serde_json::from_str(&docker(project, &inspect(id))?).unwrap_or_default();
            let answers = networks(&peer).iter().any(|network| {
                shared.contains(&network["NetworkID"])
                    && (network["IPAddress"] == host || network["Aliases"].as_array().into_iter().flatten().any(|alias| alias.as_str().is_some_and(|alias| alias.eq_ignore_ascii_case(host))))
            });
            if answers && peer["Running"] == true {
                candidates.push(peer);
            }
        }
    }
    let [container] = candidates.as_slice() else {
        return Err(Error::Invalid(format!("No single running container of this Compose project answers to {host}. Add the database in the Database tool window yourself.")));
    };
    let mut endpoints: Vec<(String, u16)> = container["Ports"][format!("{port}/tcp")]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|binding| {
            let address: IpAddr = binding["HostIp"].as_str()?.parse().ok()?;
            let port: u16 = binding["HostPort"].as_str()?.parse().ok().filter(|port| *port > 0)?;
            let host = match address {
                IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_owned(),
                IpAddr::V6(ip) if ip.is_unspecified() => "::1".to_owned(),
                ip if ip.is_loopback() => ip.to_string(),
                _ => return None,
            };
            Some((host, port))
        })
        .collect();
    endpoints.sort_by_key(|(host, _)| host.contains(':'));
    endpoints.dedup();
    let Some(first) = endpoints.first().cloned() else {
        return Err(Error::Invalid(format!("Port {port} of the database container is not published on this machine. Publish it in compose.yaml (ports: [\"{port}\"]), then try again.")));
    };
    if endpoints.iter().any(|(_, other)| *other != first.1) {
        return Err(Error::Invalid(format!("Port {port} of the database container is published on several ports. Add the database in the Database tool window yourself.")));
    }
    Ok(first)
}

/// The data source for one connection of the helper's `sources` answer; `note` says how a container
/// host was mapped. A SQLite file must exist inside the project.
fn source(project: &Project, root: &str, connection: &Value, target: Option<&Target>, docker: Docker) -> Result<(PluginDataSource, Option<String>), Error> {
    let name = plain(connection, "name").ok_or(Error::Stale)?;
    if let Some(error) = plain(connection, "error") {
        return Err(Error::Invalid(format!("Connection {name} could not be read ({error}).")));
    }
    let declared = plain(connection, "driver").unwrap_or_default();
    let Some(driver) = driver(&declared) else {
        return Err(Error::Invalid(format!("Connection {name} uses the {declared} driver, which the IDE's Database tool window does not support.")));
    };
    let mut definition = PluginDataSource {
        id: source_id(root, &name),
        name: format!("Symfony · {name}"),
        driver: driver.into(),
        host: None,
        port: None,
        database: None,
        user: None,
        url: None,
        options: Vec::new(),
    };
    if driver == "sqlite" {
        let path = plain(connection, "path").unwrap_or_default();
        let path = match target {
            Some(target) => target.host_path(root, &path).unwrap_or_default(),
            None => path,
        };
        let inside = path.strip_prefix(&format!("{root}/")).is_some_and(|relative| relative.split('/').all(|part| !matches!(part, "" | "." | "..")));
        if !inside || !project.exists(&path).unwrap_or(false) {
            return Err(Error::Invalid(format!("Connection {name} is a SQLite database outside this project or not created yet. Create it (bin/console doctrine:database:create), then try again.")));
        }
        definition.database = Some(path);
        return Ok((definition, None));
    }
    let host = plain(connection, "host").filter(|host| !host.contains(['/', '@', '?', '#', ' '])).ok_or_else(|| Error::Invalid(format!("Connection {name} has no database host.")))?;
    let port = match plain(connection, "port") {
        Some(port) => Some(port.parse::<u16>().ok().filter(|port| *port > 0).ok_or_else(|| Error::Invalid(format!("Connection {name} has an invalid port {port}.")))?),
        None => None,
    };
    let mut note = None;
    match target {
        Some(target) => {
            let (mapped_host, mapped_port) = published(project, target, driver, &host, port, docker)?;
            let original = format!("{host}:{}", port.or_else(|| default_port(driver)).unwrap_or_default());
            note = Some(format!("The app reaches it in Docker as {original}; the IDE uses the published port {mapped_host}:{mapped_port}."));
            definition.host = Some(mapped_host);
            definition.port = Some(mapped_port);
        }
        None => {
            definition.host = Some(host);
            definition.port = port;
        }
    }
    definition.database = plain(connection, "dbname");
    definition.user = plain(connection, "user");
    Ok((definition, note))
}

fn names_key(root: &str) -> String {
    format!("symfony-database-sources-v1:{}", hash(root))
}

/// Source names by id, kept so pickers show names; empty without the storage permission.
fn saved_names(root: &str) -> BTreeMap<String, String> {
    host::storage::get(&names_key(root)).ok().flatten().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save_names(root: &str, names: &BTreeMap<String, String>) {
    let _ = host::storage::set(&names_key(root), &json!(names).to_string());
}

fn project_of(ctx: &Invocation) -> Result<(Project, String), Error> {
    let project = ctx.project().ok_or("Open a Symfony project first.")?;
    let root = project.root()?;
    Ok((project, root))
}

/// Symfony: Add Database to IDE….
pub(crate) fn add(ctx: &Invocation) -> Result<(), Error> {
    add_with(ctx, &mut |project| crate::database::probe(project, "sources", &json!({})), &mut docker_target::run)
}

fn add_with(ctx: &Invocation, probe: &mut dyn FnMut(&Project) -> Result<Value, Error>, docker: Docker) -> Result<(), Error> {
    let (project, root) = project_of(ctx)?;
    project.data_sources().map_err(denied)?;
    // One task covers the container check, the probe and the published-port lookup, whose
    // `docker inspect` calls run after the probe's own task ends.
    let task = host::begin_task("Reading the app's database connections", false);
    let target = docker_target::verify(&project)?;
    let answer = probe(&project)?;
    let connections: Vec<Value> = answer["connections"].as_array().cloned().unwrap_or_default().into_iter().take(32).collect();
    let connection = match connections.as_slice() {
        [] => return Err(Error::Invalid("The app declares no Doctrine connection. Configure doctrine.dbal in config/packages/doctrine.yaml, then try again.".into())),
        [only] => only.clone(),
        several => {
            let items: Vec<PickItem> = several
                .iter()
                .filter_map(|connection| {
                    let name = plain(connection, "name")?;
                    let label = if connection["default"] == true { format!("{name} (default)") } else { name.clone() };
                    Some(PickItem::new(&name, &label).description(&plain(connection, "driver").unwrap_or_default()))
                })
                .collect();
            let Some(name) = host::pick("Add which Doctrine connection to the IDE?", &items)? else { return Ok(()) };
            several.iter().find(|connection| plain(connection, "name").as_deref() == Some(name.as_str())).cloned().ok_or(Error::Stale)?
        }
    };
    let (definition, note) = source(&project, &root, &connection, target.as_ref(), docker)?;
    drop(task);
    let mut lines = vec![format!("Project: {root}"), format!("Driver: {}", definition.driver)];
    if let Some(host) = &definition.host {
        lines.push(format!("Host: {host}{}", definition.port.map(|port| format!(":{port}")).unwrap_or_default()));
    }
    lines.extend(definition.database.iter().map(|database| format!("Database: {database}")));
    lines.extend(definition.user.iter().map(|user| format!("User: {user}")));
    lines.extend(note);
    lines.push("The password is not copied: enter it when you connect.".into());
    if !ui_kit::prompts::confirm_exact(&format!("Add {} to the IDE?", definition.name), &lines.join("\n"))? {
        return Ok(());
    }
    project.upsert_data_source(&definition).map_err(denied)?;
    let mut names = saved_names(&root);
    names.insert(definition.id.clone(), definition.name.clone());
    save_names(&root, &names);
    host::notify(
        NotificationLevel::Info,
        "Symfony",
        &format!("Added {} to the Database tool window. Connect with Symfony: Manage Database Connections… and enter the password.", definition.name),
    );
    Ok(())
}

/// Symfony: Manage Database Connections…: pick one of this project's sources, then Connect or Remove.
pub(crate) fn manage(ctx: &Invocation) -> Result<(), Error> {
    let (project, root) = project_of(ctx)?;
    let mut ids: Vec<String> = project.data_sources().map_err(denied)?.into_iter().filter(|id| ours(id)).collect();
    ids.sort();
    if ids.is_empty() {
        host::notify(NotificationLevel::Info, "Symfony", "This project has no Symfony database in the IDE yet. Run Symfony: Add Database to IDE… first.");
        return Ok(());
    }
    let names = saved_names(&root);
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let items: Vec<PickItem> = ids.iter().map(|id| PickItem::new(id, &name(id))).collect();
    let Some(id) = host::pick("Symfony database connections", &items)? else { return Ok(()) };
    if !ids.contains(&id) {
        return Err(Error::Stale);
    }
    let actions = [PickItem::new("connect", "Connect…"), PickItem::new("remove", "Remove from the IDE…")];
    let Some(action) = host::pick(&name(&id), &actions)? else { return Ok(()) };
    if action == "connect" {
        return project.connect_data_source(&id).map_err(denied);
    }
    let body = format!("Project: {root}\nRemoves {} from the Database tool window, with its saved password.\nThe database and its data are not changed.", name(&id));
    if !ui_kit::prompts::confirm_exact(&format!("Remove {} from the IDE?", name(&id)), &body)? {
        return Ok(());
    }
    let removed = name(&id);
    project.remove_data_source(&id).map_err(denied)?;
    let mut names = names;
    names.remove(&id);
    save_names(&root, &names);
    host::notify(NotificationLevel::Info, "Symfony", &format!("Removed {removed} from the Database tool window."));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker_target::tests::{ID, choose_first, raw};
    use openidea_plugin::mock::MockHost;

    fn host() -> MockHost {
        MockHost::install().with_capabilities(&["project.read", "storage", "database.sources", "process:docker"])
    }

    fn connection(driver: &str) -> Value {
        json!({"name":"default","default":true,"driver":driver,"host":"127.0.0.1","port":"3307","dbname":"shop","user":"app"})
    }

    fn no_docker(_: &Project, _: &[String]) -> Result<String, Error> {
        panic!("no Docker call expected")
    }

    fn mapped(host: &MockHost, connection: Value) -> Result<PluginDataSource, Error> {
        let project = host.invocation().project().unwrap();
        source(&project, "/project", &connection, None, &mut no_docker).map(|(definition, _)| definition)
    }

    #[test]
    fn each_doctrine_driver_maps_to_the_ide_driver() {
        let host = host();
        for (doctrine, ide) in [("pdo_mysql", "mysql"), ("mysqli", "mysql"), ("pdo_pgsql", "postgres"), ("pdo_sqlsrv", "sqlserver"), ("sqlsrv", "sqlserver")] {
            let definition = mapped(&host, connection(doctrine)).unwrap();
            assert_eq!(definition.driver, ide);
            assert_eq!((definition.host.as_deref(), definition.port, definition.database.as_deref(), definition.user.as_deref()), (Some("127.0.0.1"), Some(3307), Some("shop"), Some("app")));
            assert!(definition.url.is_none() && definition.options.is_empty());
            assert_eq!(definition.name, "Symfony · default");
            assert!(ours(&definition.id));
        }
        assert!(mapped(&host, connection("ibm_db2")).unwrap_err().to_string().contains("does not support"));
        assert_ne!(source_id("/a", "default"), source_id("/b", "default"));
    }

    #[test]
    fn sqlite_must_be_an_existing_file_inside_the_project() {
        let host = host();
        host.add_file("var/data.db", "");
        let sqlite = |path: &str| json!({"name":"default","driver":"pdo_sqlite","path":path});
        assert_eq!(mapped(&host, sqlite("/project/var/data.db")).unwrap().database.as_deref(), Some("/project/var/data.db"));
        for outside in ["/etc/data.db", "/project/../etc/data.db", "/project/var/missing.db", ":memory:", ""] {
            assert!(mapped(&host, sqlite(outside)).is_err(), "{outside}");
        }
        assert_eq!(mapped(&host, json!({"name":"x","driver":"sqlite3","path":"/project/var/data.db"})).unwrap().driver, "sqlite");
    }

    #[test]
    fn adding_confirms_without_the_password_and_cancel_adds_nothing() {
        let host = host();
        let mut probe = |_: &Project| Ok(json!({"connections":[connection("pdo_mysql")]}));
        add_with(&host.invocation(), &mut probe, &mut no_docker).unwrap();
        let project = host.invocation().project().unwrap();
        assert!(project.data_sources().unwrap().is_empty(), "a dismissed confirm adds nothing");
        host.answer_next_prompt(Some(&["ok"]));
        add_with(&host.invocation(), &mut probe, &mut no_docker).unwrap();
        assert_eq!(project.data_sources().unwrap(), [source_id("/project", "default")]);
        assert!(host.notifications().iter().any(|n| n.body.starts_with("Added Symfony · default")));
    }

    #[test]
    fn without_the_permission_nothing_runs() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let mut probe = |_: &Project| -> Result<Value, Error> { panic!("the app must not boot") };
        let error = add_with(&host.invocation(), &mut probe, &mut no_docker).unwrap_err().to_string();
        assert!(error.contains("database.sources permission"), "{error}");
    }

    #[test]
    fn remove_is_confirmed_and_connect_uses_the_ide() {
        let host = host();
        host.answer_next_prompt(Some(&["ok"]));
        add_with(&host.invocation(), &mut |_| Ok(json!({"connections":[connection("pdo_pgsql")]})), &mut no_docker).unwrap();
        let id = source_id("/project", "default");
        let project = host.invocation().project().unwrap();
        host.answer_next_prompt(Some(&[&id]));
        host.answer_next_prompt(Some(&["remove"]));
        host.answer_next_prompt(None);
        manage(&host.invocation()).unwrap();
        assert_eq!(project.data_sources().unwrap(), [id.clone()], "a cancelled remove keeps it");
        host.answer_next_prompt(Some(&[&id]));
        host.answer_next_prompt(Some(&["connect"]));
        manage(&host.invocation()).unwrap();
        host.answer_next_prompt(Some(&[&id]));
        host.answer_next_prompt(Some(&["remove"]));
        host.answer_next_prompt(Some(&["ok"]));
        manage(&host.invocation()).unwrap();
        assert!(project.data_sources().unwrap().is_empty());
    }

    fn container(id: &str, running: bool, network: &str, alias: &str, ports: Value) -> String {
        json!({"Id":id,"Running":running,"Project":"shop","Networks":{"shop_default":{"NetworkID":network,"Aliases":[alias],"IPAddress":"172.18.0.3"}},"Ports":ports}).to_string()
    }

    #[test]
    fn a_docker_target_maps_the_database_service_to_its_published_port() {
        let host = host();
        host.on_process("docker", |args, _| Ok(openidea_plugin::ProcessOutput { status: Some(0), stdout: if args[2] == "inspect" { raw().to_string() } else { String::new() }, stderr: String::new() }));
        choose_first(&host, raw());
        let project = host.invocation().project().unwrap();
        let target = docker_target::verify(&project).unwrap().unwrap();
        let peer = "b".repeat(64);
        let answer = move |args: &[String], ports: Value| -> String {
            match args[2].as_str() {
                "ps" => format!("{peer}\n"),
                _ if args.last().unwrap() == ID => container(ID, true, "net1", "php", json!({})),
                _ => container(&peer, true, "net1", "database", ports),
            }
        };
        let remote = json!({"name":"default","driver":"pdo_pgsql","host":"database","dbname":"app","user":"app"});
        let published_port = json!({"5432/tcp":[{"HostIp":"0.0.0.0","HostPort":"54321"},{"HostIp":"::","HostPort":"54321"}]});
        let (definition, note) = source(&project, "/project", &remote, Some(&target), &mut |_, args| Ok(answer(args, published_port.clone()))).unwrap();
        assert_eq!((definition.host.as_deref(), definition.port), (Some("127.0.0.1"), Some(54321)));
        assert!(note.unwrap().contains("database:5432"));
        let ambiguous = json!({"5432/tcp":[{"HostIp":"0.0.0.0","HostPort":"1"},{"HostIp":"127.0.0.1","HostPort":"2"}]});
        assert!(source(&project, "/project", &remote, Some(&target), &mut |_, args| Ok(answer(args, ambiguous.clone()))).unwrap_err().to_string().contains("several ports"));
        assert!(source(&project, "/project", &remote, Some(&target), &mut |_, args| Ok(answer(args, json!({})))).unwrap_err().to_string().contains("not published"));
        let sqlite = json!({"name":"default","driver":"pdo_sqlite","path":"/app/var/data.db"});
        host.add_file("var/data.db", "");
        assert_eq!(source(&project, "/project", &sqlite, Some(&target), &mut no_docker).unwrap().0.database.as_deref(), Some("/project/var/data.db"));
    }
}
