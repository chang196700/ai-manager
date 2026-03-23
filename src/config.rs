use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflows: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
    #[serde(default)]
    pub skills: IndexMap<String, String>,
    #[serde(default)]
    pub agents: IndexMap<String, String>,
    #[serde(default)]
    pub instructions: IndexMap<String, String>,
    #[serde(default)]
    pub hooks: IndexMap<String, String>,
    #[serde(default)]
    pub workflows: IndexMap<String, String>,
}

pub fn load_shared(root: &Path) -> Result<Config> {
    let path = root.join("config.toml");
    if !path.exists() { return Ok(Config::default()); }
    let content = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

pub fn load_local(root: &Path) -> Result<Config> {
    let path = root.join("config.local.toml");
    if !path.exists() { return Ok(Config::default()); }
    let content = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

pub fn merged(root: &Path) -> Result<Config> {
    let shared = load_shared(root)?;
    let local = load_local(root)?;
    Ok(merge(shared, local))
}

pub fn merge(mut base: Config, overlay: Config) -> Config {
    let mut repo_map: IndexMap<String, RepoConfig> = IndexMap::new();
    for repo in base.repos { repo_map.insert(repo.name.clone(), repo); }
    for repo in overlay.repos { repo_map.insert(repo.name.clone(), repo); }
    base.repos = repo_map.into_values().collect();
    merge_map(&mut base.skills, overlay.skills);
    merge_map(&mut base.agents, overlay.agents);
    merge_map(&mut base.instructions, overlay.instructions);
    merge_map(&mut base.hooks, overlay.hooks);
    merge_map(&mut base.workflows, overlay.workflows);
    base
}

fn merge_map(base: &mut IndexMap<String, String>, overlay: IndexMap<String, String>) {
    for (k, v) in overlay { base.insert(k, v); }
}

pub fn save_shared(root: &Path, config: &Config) -> Result<()> {
    let path = root.join("config.toml");
    let content = toml::to_string_pretty(config).context("serializing config")?;
    std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}

pub fn save_local(root: &Path, config: &Config) -> Result<()> {
    let path = root.join("config.local.toml");
    let content = toml::to_string_pretty(config).context("serializing config")?;
    std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}

pub fn find_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir().context("getting current dir")?;
    loop {
        if current.join("config.toml").exists() && current.join("manager").is_dir() {
            return Ok(current);
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => anyhow::bail!("Could not find .ai workspace root (looking for config.toml + manager/ dir)"),
        }
    }
}
