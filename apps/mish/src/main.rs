use anyhow::Result;
use clap::{Parser, Subcommand};
use mish::{
    appliance::{EventStore, now},
    coordinator::Coordinator,
    domain::*,
    storage::{self, MishStore},
    validation,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
#[derive(Parser)]
#[command(version, about = "Offline event coordination for staff and operators")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Commission {
        #[arg(long)]
        name: String,
    },
    Enroll {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "staff")]
        role: Role,
    },
    Revoke {
        #[arg(long)]
        id: String,
    },
    Message {
        #[arg(long)]
        text: String,
        #[arg(long)]
        author: String,
    },
    Alert {
        #[arg(long)]
        text: String,
        #[arg(long)]
        author: String,
        #[arg(long)]
        responder: String,
        #[arg(long)]
        expires: f64,
        #[arg(long)]
        urgent: bool,
    },
    Ack {
        #[arg(long)]
        id: String,
        #[arg(long)]
        token: String,
    },
    Announce {
        #[arg(long)]
        text: String,
    },
    Backup {
        #[arg(long)]
        output: PathBuf,
    },
    Restore {
        #[arg(long)]
        input: PathBuf,
    },
    Messages,
    Prune,
    Wipe {
        #[arg(long,value_parser=["WIPE-EVENT"])]
        confirm: String,
    },
    Status,
    Serve {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    Waypoint {
        #[command(subcommand)]
        command: Waypoint,
    },
    Geopack {
        #[command(subcommand)]
        command: Geopack,
    },
    Identity {
        #[command(subcommand)]
        command: Identity,
    },
    #[command(hide = true)]
    InternalCheckSchemas {
        #[arg(long)]
        schemas: PathBuf,
        #[arg(long)]
        examples: PathBuf,
        #[arg(long)]
        catalog: PathBuf,
    },
    #[command(hide = true)]
    Stub {
        #[arg(value_parser=["matrix","openziti"])]
        service: String,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        require_mfa: bool,
    },
}
#[derive(Subcommand)]
enum Waypoint {
    AddSimulated {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lon: f64,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long, default_value = "private")]
        visibility: String,
    },
    List,
}
#[derive(Subcommand)]
enum Geopack {
    Create {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: String,
        #[arg(long, required = true)]
        waypoint: Vec<String>,
        #[arg(long, default_value = "private")]
        visibility: String,
    },
    Publish {
        #[arg(long)]
        pack: String,
        #[arg(long)]
        relay: Vec<String>,
    },
    List,
}
#[derive(Subcommand)]
enum Identity {
    BootstrapOperator,
}
fn print(value: impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("mish: {error:#}");
        std::process::exit(2);
    }
}
async fn run(cli: Cli) -> Result<()> {
    let config = storage::load_config(cli.config.as_deref(), cli.state_dir.as_deref())?;
    let state = PathBuf::from(
        config["stateDir"]
            .as_str()
            .ok_or_else(|| mish::invalid("stateDir required"))?,
    );
    let queue_limit = config["queueLimit"].as_i64().unwrap_or(1000);
    let retention = config["retentionDays"].as_i64().unwrap_or(7);
    match cli.command {
        Command::Restore { input } => {
            return print(mish::recovery::restore_snapshot(&input, &state)?);
        }
        Command::Status => return print(storage::status_payload(&config).await),
        Command::InternalCheckSchemas {
            schemas,
            examples,
            catalog,
        } => {
            for name in [
                "waypoint.schema.json",
                "geopack-manifest.schema.json",
                "catalog.schema.json",
            ] {
                storage::read_json(&schemas.join(name))?;
            }
            for (directory, validate) in [
                (
                    "waypoints",
                    validation::waypoint as fn(&Value) -> Result<()>,
                ),
                ("geopacks", validation::geopack),
            ] {
                for path in fs::read_dir(examples.join(directory))? {
                    let path = path?.path();
                    if path.extension().is_some_and(|v| v == "json") {
                        validate(&storage::read_json(&path)?)?;
                    }
                }
            }
            validation::catalog(&storage::read_json(&catalog)?)?;
            return print(json!({"ok":true,"schemas":schemas,"examples":examples}));
        }
        Command::Serve { host, port } => {
            let events = EventStore::new(&state, queue_limit, true)?;
            let coordinator = Coordinator::start(events, retention);
            let radio = mish::radio::start(coordinator.clone(), &config)?;
            let retention_coordinator = coordinator.clone();
            let pruning = tokio::spawn(async move {
                loop {
                    let _ = retention_coordinator.prune().await;
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            });
            let app = mish::server::AppState {
                coordinator,
                coordinator_id: config["coordinatorId"].as_str().unwrap_or("mish").into(),
                radio_status: radio.status.clone(),
                adapter: radio.adapter.clone(),
                legacy: MishStore::new(&state, config.clone())?,
                legacy_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(4)),
            };
            let result = mish::server::serve(
                host.as_deref()
                    .unwrap_or_else(|| config["api"]["bindHost"].as_str().unwrap_or("127.0.0.1")),
                port.unwrap_or(config["api"]["port"].as_u64().unwrap_or(8787).try_into()?),
                app,
            )
            .await;
            pruning.abort();
            radio.stop().await;
            return result;
        }
        Command::Waypoint { command } => {
            let store = MishStore::new(&state, config)?;
            return match command {
                Waypoint::List => print(json!({"waypoints":store.list_waypoints()?})),
                Waypoint::AddSimulated {
                    id,
                    lat,
                    lon,
                    label,
                    body,
                    visibility,
                } => {
                    let (waypoint, created) = store.add_simulated_waypoint(
                        lat,
                        lon,
                        label.as_deref(),
                        body.as_deref(),
                        &visibility,
                        id.as_deref(),
                        json!({"cli":"mish waypoint add-simulated"}),
                    )?;
                    print(json!({"created":created,"waypoint":waypoint}))
                }
            };
        }
        Command::Geopack { command } => {
            let store = MishStore::new(&state, config)?;
            return match command {
                Geopack::List => print(json!({"geopacks":store.list_geopacks()?})),
                Geopack::Publish { pack, relay } => print(store.publish_geopack(&pack, relay)?),
                Geopack::Create {
                    id,
                    name,
                    waypoint,
                    visibility,
                } => {
                    let (pack, created) =
                        store.create_geopack(&name, waypoint, &visibility, id.as_deref())?;
                    print(json!({"created":created,"pack":pack}))
                }
            };
        }
        Command::Identity { .. } => {
            let (enrollment, created) = MishStore::new(&state, config)?.bootstrap_operator()?;
            return print(json!({"created":created,"operatorEnrollment":enrollment}));
        }
        Command::Stub {
            service,
            host,
            port,
            provider,
            domain,
            role,
            require_mfa,
        } => {
            let payload = json!({"ok":true,"service":service,"provider":provider,"domain":domain,"role":role,"privilegedMfaRequired":require_mfa,"stub":true});
            let app = axum::Router::new().route(
                "/health",
                axum::routing::get(move || {
                    let value = payload.clone();
                    async move { axum::Json(value) }
                }),
            );
            axum::serve(
                tokio::net::TcpListener::bind((host.as_str(), port)).await?,
                app,
            )
            .await?;
            return Ok(());
        }
        Command::Wipe { .. } => {
            let state = fs::canonicalize(state)?;
            if ["/", "/var", "/var/lib", "/home", "/tmp"]
                .iter()
                .any(|v| state == std::path::Path::new(v))
                || !state.join("events.sqlite").is_file()
            {
                return Err(mish::invalid(
                    "refusing unsafe or uncommissioned state directory",
                ));
            }
            for child in fs::read_dir(&state)? {
                let child = child?;
                if child.file_type()?.is_dir() {
                    fs::remove_dir_all(child.path())?;
                } else {
                    fs::remove_file(child.path())?;
                }
            }
            return print(json!({"wiped":state}));
        }
        command => {
            let events = EventStore::new(&state, queue_limit, false)?;
            match command {
                Command::Commission { name } => {
                    print(events.enroll(&name, Role::Operator, true)?)?
                }
                Command::Enroll { name, role } => print(events.enroll(&name, role, false)?)?,
                Command::Revoke { id } => {
                    events.revoke(&id)?;
                    print(json!({"revoked":id}))?;
                }
                Command::Message { text, author } => {
                    print(events.create(NewRecord::message(MessageText::new(text)?, author)?)?)?
                }
                Command::Alert {
                    text,
                    author,
                    responder,
                    expires,
                    urgent,
                } => print(events.create(NewRecord::alert(
                    MessageText::new(text)?,
                    author,
                    StaffName::new(responder)?,
                    expires,
                    urgent,
                    now(),
                )?)?)?,
                Command::Ack { id, token } => {
                    let actor = events
                        .authenticate(&token)?
                        .ok_or_else(|| mish::invalid("invalid staff token"))?;
                    print(events.acknowledge(
                        &id,
                        &actor.id,
                        AcknowledgementSource::Browser,
                        None,
                        None,
                    )?)?;
                }
                Command::Announce { text } => print(events.publish(&text)?)?,
                Command::Backup { output } => {
                    events.backup(&output)?;
                    print(json!({"backup":output}))?;
                }
                Command::Messages => print(json!({"messages":events.records()?}))?,
                Command::Prune => {
                    events.prune(retention)?;
                }
                _ => unreachable!("handled command"),
            }
        }
    }
    Ok(())
}
