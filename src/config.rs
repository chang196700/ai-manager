use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
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

/// Returns the default .ai home directory.
///
/// Resolution order:
/// 1. `AI_HOME` environment variable (if set)
/// 2. `~/.ai` — `%USERPROFILE%\.ai` on Windows, `$HOME/.ai` on Unix/macOS
pub fn default_ai_home() -> Result<PathBuf> {
    if let Ok(val) = std::env::var("AI_HOME") {
        return Ok(PathBuf::from(val));
    }
    let home = home_dir().context("Could not determine home directory. Set AI_HOME to override.")?;
    Ok(home.join(".ai"))
}

/// Returns the path to the user's home directory in a cross-platform way.
fn home_dir() -> Option<PathBuf> {
    // Try USERPROFILE (Windows), then HOME (Unix/macOS)
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub fn find_root() -> Result<PathBuf> {
    let root = default_ai_home()?;
    if !root.exists() {
        anyhow::bail!(
            "Workspace not found at '{}'. Run `ai-manager init` to create it, or set AI_HOME to point to an existing workspace.",
            root.display()
        );
    }
    Ok(root)
}
