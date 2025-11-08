mod config;
mod connection;
mod shutdown;
mod tui;

use anyhow::{anyhow, Result};
use connection::{run_namespace, StatusEvent};
use shutdown::Shutdown;
use std::env;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = parse_args()?;
    let config = config::Config::load(cli.config_path)?;
    let namespaces = config.select_namespaces(&cli.namespaces)?;
    let (event_tx, event_rx) = std::sync::mpsc::channel::<StatusEvent>();
    let shutdown = Shutdown::new();
    let ui_shutdown = shutdown.clone();
    let ui_handle = std::thread::spawn(move || tui::run(event_rx, ui_shutdown));
    let mut tasks = Vec::new();
    for (name, namespace) in namespaces {
        let sender = event_tx.clone();
        let shutdown_token = shutdown.clone();
        tasks.push(tokio::spawn(async move {
            spawn_namespace(name, namespace, sender, shutdown_token).await
        }));
    }
    drop(event_tx);
    let ctrl_c_shutdown = shutdown.clone();
    let ctrl_c_task = tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        ctrl_c_shutdown.trigger();
    });
    for task in tasks {
        match task.await {
            Ok(result) => {
                if let Err(err) = result {
                    return Err(err);
                }
            }
            Err(err) => {
                return Err(anyhow!(err));
            }
        }
    }
    if !shutdown.is_triggered() {
        shutdown.trigger();
    }
    if !ctrl_c_task.is_finished() {
        ctrl_c_task.abort();
    }
    match ctrl_c_task.await {
        Ok(_) => {}
        Err(err) if err.is_cancelled() => {}
        Err(err) => return Err(anyhow!(err)),
    }
    match ui_handle.join() {
        Ok(result) => result?,
        Err(_) => return Err(anyhow!("TUI panicked")),
    }
    Ok(())
}

async fn spawn_namespace(
    name: String,
    namespace: config::Namespace,
    sender: Sender<StatusEvent>,
    shutdown: Shutdown,
) -> Result<()> {
    run_namespace(name.clone(), namespace, sender.clone(), shutdown).await
}

struct CliArgs {
    config_path: Option<PathBuf>,
    namespaces: Vec<String>,
}

fn parse_args() -> Result<CliArgs> {
    let mut iter = env::args().skip(1);
    let mut config_path = None;
    let mut namespaces = Vec::new();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => {
                let next = iter
                    .next()
                    .ok_or_else(|| anyhow!("--config requires a path argument"))?;
                config_path = Some(PathBuf::from(next));
            }
            value => namespaces.push(value.to_string()),
        }
    }
    if namespaces.is_empty() {
        return Err(anyhow!("at least one namespace is required"));
    }
    Ok(CliArgs {
        config_path,
        namespaces,
    })
}
