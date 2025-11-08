use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    namespaces: HashMap<String, Namespace>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Namespace {
    pub ssh_user: String,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub target_ip: String,
    pub forwards: Vec<Forward>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Forward {
    pub local: u16,
    pub remote: u16,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = resolve_config_path(path)?;
        let raw = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config {}", config_path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;
        Ok(config)
    }

    pub fn select_namespaces(&self, requested: &[String]) -> Result<Vec<(String, Namespace)>> {
        if requested.is_empty() {
            return Err(anyhow!("at least one namespace is required"));
        }
        let mut result = Vec::with_capacity(requested.len());
        for name in requested {
            let namespace = self
                .namespaces
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow!("namespace {} not found", name))?;
            result.push((name.clone(), namespace));
        }
        Ok(result)
    }
}

fn resolve_config_path(path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(explicit) = path {
        return Ok(explicit);
    }
    let base =
        dirs::config_dir().ok_or_else(|| anyhow!("failed to locate configuration directory"))?;
    let dir = base.join("ssht");
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create directory {}", dir.display()))?;
    }
    Ok(dir.join("config.toml"))
}
