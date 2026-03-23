use anyhow::{Context, Result};
use indexmap::IndexMap;
use std::path::{Path, PathBuf};
use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Skills,
    Agents,
    Instructions,
    Hooks,
    Workflows,
}

impl ResourceType {
    pub fn all() -> &'static [ResourceType] {
        &[
            ResourceType::Skills,
            ResourceType::Agents,
            ResourceType::Instructions,
            ResourceType::Hooks,
            ResourceType::Workflows,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ResourceType::Skills => "skills",
            ResourceType::Agents => "agents",
            ResourceType::Instructions => "instructions",
            ResourceType::Hooks => "hooks",
            ResourceType::Workflows => "workflows",
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, ResourceType::Skills | ResourceType::Hooks)
    }

    pub fn file_suffix(&self) -> Option<&'static str> {
        match self {
            ResourceType::Agents => Some(".agent.md"),
            ResourceType::Instructions => Some(".instructions.md"),
            ResourceType::Workflows => Some(".md"),
            _ => None,
        }
    }

    pub fn config_map<'a>(&self, config: &'a Config) -> &'a IndexMap<String, String> {
        match self {
            ResourceType::Skills => &config.skills,
            ResourceType::Agents => &config.agents,
            ResourceType::Instructions => &config.instructions,
            ResourceType::Hooks => &config.hooks,
            ResourceType::Workflows => &config.workflows,
        }
    }

    pub fn config_map_mut<'a>(&self, config: &'a mut Config) -> &'a mut IndexMap<String, String> {
        match self {
            ResourceType::Skills => &mut config.skills,
            ResourceType::Agents => &mut config.agents,
            ResourceType::Instructions => &mut config.instructions,
            ResourceType::Hooks => &mut config.hooks,
            ResourceType::Workflows => &mut config.workflows,
        }
    }
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceType {
    Repo,
    User,
}

#[derive(Debug, Clone)]
pub struct LinkSource {
    pub source_type: SourceType,
    pub source_name: String,
}

impl LinkSource {
    pub fn parse(value: &str) -> Result<(Self, String)> {
        let parts: Vec<&str> = value.splitn(3, ':').collect();
        if parts.len() != 3 {
            anyhow::bail!("Invalid source value '{}': expected 'type:name:relpath'", value);
        }
        let source_type = match parts[0] {
            "repo" => SourceType::Repo,
            "user" => SourceType::User,
            other => anyhow::bail!("Unknown source type '{}'", other),
        };
        Ok((
            LinkSource { source_type, source_name: parts[1].to_string() },
            parts[2].to_string(),
        ))
    }

    pub fn resolve_path(&self, root: &Path, relpath: &str) -> Option<PathBuf> {
        match self.source_type {
            SourceType::Repo => {
                let p = root.join("repo").join(&self.source_name).join(relpath);
                p.exists().then_some(p)
            }
            SourceType::User => {
                let local = root.join("user.local").join(&self.source_name).join(relpath);
                if local.exists() { return Some(local); }
                let user = root.join("user").join(&self.source_name).join(relpath);
                user.exists().then_some(user)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AvailableItem {
    pub suggested_key: String,
    pub source_value: String,
    pub display_source: String,
}

#[derive(Debug, Clone)]
pub enum OpType {
    Add,
    Remove,
    RepoAdd,
}

#[derive(Debug, Clone)]
pub struct OpDesc {
    pub op: OpType,
    pub resource_type: Option<ResourceType>,
    pub key: String,
}

impl OpDesc {
    pub fn to_message(&self) -> String {
        let op_str = match self.op {
            OpType::Add => "add",
            OpType::Remove => "remove",
            OpType::RepoAdd => "add repo",
        };
        if let Some(rt) = &self.resource_type {
            format!("{} {} {}", op_str, rt.name(), self.key)
        } else {
            format!("{} {}", op_str, self.key)
        }
    }
}

pub fn list_available(root: &Path, config: &Config, rtype: ResourceType) -> Result<Vec<AvailableItem>> {
    let installed = rtype.config_map(config);
    let installed_values: std::collections::HashSet<&str> = installed.values().map(|s| s.as_str()).collect();
    let mut items = Vec::new();

    // Scan repos
    for repo in &config.repos {
        let scan_paths = match rtype {
            ResourceType::Skills => repo.skills.as_deref(),
            ResourceType::Agents => repo.agents.as_deref(),
            ResourceType::Instructions => repo.instructions.as_deref(),
            ResourceType::Hooks => repo.hooks.as_deref(),
            ResourceType::Workflows => repo.workflows.as_deref(),
        };
        let scan_paths = match scan_paths {
            Some(p) => p,
            None => continue,
        };

        for scan_path in scan_paths {
            let base = if scan_path.is_empty() || scan_path == "." {
                root.join("repo").join(&repo.name)
            } else {
                root.join("repo").join(&repo.name).join(scan_path)
            };

            if !base.exists() { continue; }

            if rtype.is_dir() {
                let entries = match std::fs::read_dir(&base) { Ok(e) => e, Err(_) => continue };
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                    let dirname = entry.file_name().to_string_lossy().to_string();
                    let relpath = if scan_path.is_empty() || scan_path == "." {
                        dirname.clone()
                    } else {
                        format!("{}/{}", scan_path, dirname)
                    };
                    let source_value = format!("repo:{}:{}", repo.name, relpath);
                    if installed_values.contains(source_value.as_str()) { continue; }
                    items.push(AvailableItem {
                        suggested_key: format!("{}-{}", repo.name, dirname),
                        source_value,
                        display_source: format!("repo:{}", repo.name),
                    });
                }
            } else {
                let suffix = rtype.file_suffix().unwrap_or(".md");
                let entries = match std::fs::read_dir(&base) { Ok(e) => e, Err(_) => continue };
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                    let filename = entry.file_name().to_string_lossy().to_string();
                    if !filename.ends_with(suffix) { continue; }
                    let relpath = if scan_path.is_empty() || scan_path == "." {
                        filename.clone()
                    } else {
                        format!("{}/{}", scan_path, filename)
                    };
                    let source_value = format!("repo:{}:{}", repo.name, relpath);
                    if installed_values.contains(source_value.as_str()) { continue; }
                    let stem = filename.strip_suffix(suffix).unwrap_or(&filename);
                    items.push(AvailableItem {
                        suggested_key: format!("{}-{}", repo.name, stem),
                        source_value,
                        display_source: format!("repo:{}", repo.name),
                    });
                }
            }
        }
    }

    // Scan user directories
    let user_dir = root.join("user");
    if user_dir.exists() {
        if let Ok(groups) = std::fs::read_dir(&user_dir) {
            for group_entry in groups.flatten() {
                if !group_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                let group_name = group_entry.file_name().to_string_lossy().to_string();
                let type_dir = group_entry.path().join(rtype.name());
                if !type_dir.exists() { continue; }

                if rtype.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&type_dir) {
                        for entry in entries.flatten() {
                            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
                            let dirname = entry.file_name().to_string_lossy().to_string();
                            let relpath = format!("{}/{}", rtype.name(), dirname);
                            let source_value = format!("user:{}:{}", group_name, relpath);
                            if installed_values.contains(source_value.as_str()) { continue; }
                            items.push(AvailableItem {
                                suggested_key: dirname.clone(),
                                source_value,
                                display_source: format!("user:{}", group_name),
                            });
                        }
                    }
                } else {
                    let suffix = rtype.file_suffix().unwrap_or(".md");
                    if let Ok(entries) = std::fs::read_dir(&type_dir) {
                        for entry in entries.flatten() {
                            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                            let filename = entry.file_name().to_string_lossy().to_string();
                            if !filename.ends_with(suffix) { continue; }
                            let relpath = format!("{}/{}", rtype.name(), filename);
                            let source_value = format!("user:{}:{}", group_name, relpath);
                            if installed_values.contains(source_value.as_str()) { continue; }
                            let stem = filename.strip_suffix(suffix).unwrap_or(&filename);
                            items.push(AvailableItem {
                                suggested_key: stem.to_string(),
                                source_value,
                                display_source: format!("user:{}", group_name),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(items)
}

pub fn list_installed(root: &Path, config: &Config, rtype: ResourceType) -> Vec<(String, String, bool)> {
    rtype.config_map(config).iter().map(|(key, value)| {
        let dst = target_path(root, rtype, key);
        let link_exists = dst.exists() || is_junction_or_link(&dst);
        (key.clone(), value.clone(), link_exists)
    }).collect()
}

pub fn target_path(root: &Path, rtype: ResourceType, key: &str) -> PathBuf {
    match rtype {
        ResourceType::Skills => root.join("skills").join(key),
        ResourceType::Agents => root.join("agents").join(format!("{}.agent.md", key)),
        ResourceType::Instructions => root.join("instructions").join(format!("{}.instructions.md", key)),
        ResourceType::Hooks => root.join("hooks").join(key),
        ResourceType::Workflows => root.join("workflows").join(format!("{}.md", key)),
    }
}

pub fn link(src: &Path, dst: &Path, rtype: ResourceType) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }
    if dst.exists() || is_junction_or_link(dst) {
        unlink(dst, rtype)?;
    }
    if rtype.is_dir() {
        junction::create(src, dst)
            .with_context(|| format!("creating junction {} -> {}", dst.display(), src.display()))
    } else {
        std::fs::hard_link(src, dst)
            .with_context(|| format!("creating hard link {} -> {}", dst.display(), src.display()))
    }
}

pub fn unlink(dst: &Path, rtype: ResourceType) -> Result<()> {
    if rtype.is_dir() {
        if dst.exists() || is_junction_or_link(dst) {
            std::fs::remove_dir(dst)
                .with_context(|| format!("removing junction {}", dst.display()))?;
        }
    } else {
        if dst.exists() {
            std::fs::remove_file(dst)
                .with_context(|| format!("removing file/hardlink {}", dst.display()))?;
        }
    }
    Ok(())
}

fn is_junction_or_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn add_resource(
    root: &Path,
    shared: &mut Config,
    local: &mut Config,
    rtype: ResourceType,
    source_value: &str,
    key: &str,
    is_local: bool,
) -> Result<OpDesc> {
    let (link_source, relpath) = LinkSource::parse(source_value)?;
    let src = link_source.resolve_path(root, &relpath)
        .with_context(|| format!("source path not found: {}", source_value))?;
    let dst = target_path(root, rtype, key);
    link(&src, &dst, rtype)?;
    let config = if is_local { local } else { shared };
    rtype.config_map_mut(config).insert(key.to_string(), source_value.to_string());
    Ok(OpDesc { op: OpType::Add, resource_type: Some(rtype), key: key.to_string() })
}

pub fn remove_resource(
    root: &Path,
    shared: &mut Config,
    local: &mut Config,
    rtype: ResourceType,
    key: &str,
) -> Result<OpDesc> {
    let dst = target_path(root, rtype, key);
    if dst.exists() || is_junction_or_link(&dst) {
        unlink(&dst, rtype)?;
    }
    rtype.config_map_mut(shared).shift_remove(key);
    rtype.config_map_mut(local).shift_remove(key);
    Ok(OpDesc { op: OpType::Remove, resource_type: Some(rtype), key: key.to_string() })
}
