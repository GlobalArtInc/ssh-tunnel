use crate::config::Namespace;
use crate::shutdown::Shutdown;
use anyhow::Result;
use std::cmp::min;
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct StatusEvent {
    pub namespace: String,
    pub status: ConnectionStatus,
    pub attempt: u32,
    pub message: Option<String>,
    pub forwards: Vec<String>,
}

impl StatusEvent {
    fn new(
        namespace: String,
        status: ConnectionStatus,
        attempt: u32,
        message: Option<String>,
        forwards: &[String],
    ) -> Self {
        Self {
            namespace,
            status,
            attempt,
            message,
            forwards: forwards.to_vec(),
        }
    }
}

pub async fn run_namespace(
    name: String,
    spec: Namespace,
    sender: Sender<StatusEvent>,
    shutdown: Shutdown,
) -> Result<()> {
    let mut attempt: u32 = 0;
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);
    let forwards_view: Vec<String> = spec
        .forwards
        .iter()
        .map(|forward| format!("{} -> {}:{}", forward.local, spec.target_ip, forward.remote))
        .collect();
    loop {
        if shutdown.is_triggered() {
            let _ = sender.send(StatusEvent::new(
                name.clone(),
                ConnectionStatus::Stopped,
                attempt,
                None,
                &forwards_view,
            ));
            return Ok(());
        }
        attempt = attempt.saturating_add(1);
        let _ = sender.send(StatusEvent::new(
            name.clone(),
            ConnectionStatus::Connecting,
            attempt,
            None,
            &forwards_view,
        ));
        let mut cmd = Command::new("ssh");
        cmd.arg("-N");
        cmd.arg("-o").arg("ExitOnForwardFailure=yes");
        cmd.arg("-o").arg("ServerAliveInterval=30");
        cmd.arg("-o").arg("ServerAliveCountMax=3");
        cmd.arg("-o").arg("BatchMode=yes");
        for forward in &spec.forwards {
            let binding = format!("{}:{}:{}", forward.local, spec.target_ip, forward.remote);
            cmd.arg("-L").arg(binding);
        }
        cmd.arg("-p").arg(spec.ssh_port.to_string());
        cmd.arg(format!("{}@{}", spec.ssh_user, spec.ssh_host));
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        match cmd.spawn() {
            Ok(mut child) => {
                let _ = sender.send(StatusEvent::new(
                    name.clone(),
                    ConnectionStatus::Connected,
                    attempt,
                    None,
                    &forwards_view,
                ));
                backoff = Duration::from_secs(1);
                let status = tokio::select! {
                    res = child.wait() => res,
                    _ = shutdown.notified() => {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        let _ = sender.send(StatusEvent::new(name.clone(), ConnectionStatus::Stopped, attempt, None, &forwards_view));
                        return Ok(());
                    }
                };
                match status {
                    Ok(exit_status) => {
                        if shutdown.is_triggered() {
                            let _ = sender.send(StatusEvent::new(
                                name.clone(),
                                ConnectionStatus::Stopped,
                                attempt,
                                None,
                                &forwards_view,
                            ));
                            return Ok(());
                        }
                        let message = match exit_status.code() {
                            Some(code) => format!("ssh exited with status {}", code),
                            None => String::from("ssh terminated by signal"),
                        };
                        let _ = sender.send(StatusEvent::new(
                            name.clone(),
                            ConnectionStatus::Failed,
                            attempt,
                            Some(message),
                            &forwards_view,
                        ));
                    }
                    Err(err) => {
                        let _ = sender.send(StatusEvent::new(
                            name.clone(),
                            ConnectionStatus::Failed,
                            attempt,
                            Some(err.to_string()),
                            &forwards_view,
                        ));
                    }
                }
            }
            Err(err) => {
                let _ = sender.send(StatusEvent::new(
                    name.clone(),
                    ConnectionStatus::Failed,
                    attempt,
                    Some(err.to_string()),
                    &forwards_view,
                ));
            }
        }
        if shutdown.is_triggered() {
            let _ = sender.send(StatusEvent::new(
                name.clone(),
                ConnectionStatus::Stopped,
                attempt,
                None,
                &forwards_view,
            ));
            return Ok(());
        }
        let message = format!("retry in {} seconds", backoff.as_secs());
        let _ = sender.send(StatusEvent::new(
            name.clone(),
            ConnectionStatus::Reconnecting,
            attempt,
            Some(message),
            &forwards_view,
        ));
        sleep(backoff).await;
        backoff = min(backoff * 2, max_backoff);
    }
}
