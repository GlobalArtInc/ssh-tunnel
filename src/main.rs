use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Deserialize)]
struct Config {
    namespaces: HashMap<String, Namespace>,
}

#[derive(Deserialize)]
struct Namespace {
    ssh_user: String,
    ssh_host: String,
    ssh_port: u16,
    target_ip: String,
    forwards: Vec<Forward>,
}

#[derive(Deserialize)]
struct Forward {
    local: u16,
    remote: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err("at least one namespace must be specified".into());
    }
    let path = resolve_config_path()?;
    let raw = fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&raw)?;
    for ns in &args[1..] {
        let data = config
            .namespaces
            .get(ns)
            .ok_or_else(|| format!("no such namespace: {}", ns))?;
        let mut cmd = Command::new("ssh");
        cmd.arg("-N");
        for f in &data.forwards {
            let binding = format!("{}:{}:{}", f.local, data.target_ip, f.remote);
            cmd.arg("-L").arg(binding);
        }
        cmd.arg("-p")
            .arg(data.ssh_port.to_string())
            .arg(format!("{}@{}", data.ssh_user, data.ssh_host));
        println!("tunnels for namespace {} are configuring", ns);
        io::stdout().flush()?;
        let status = cmd.status()?;
        if !status.success() {
            return Err(format!("error while starting namespace: {}", ns).into());
        }
        println!("tunnels for namespace {} are active", ns);
    }
    Ok(())
}

fn resolve_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| -> Box<dyn std::error::Error> { "cannot determine home directory".into() })?;
    let dir = home.join(".config/ssht");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    let path = dir.join("config.toml");

    Ok(path)
}
