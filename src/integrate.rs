use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use serde_json::Value;
use crate::resource::ResourceType;

// ── Integrate mode types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IntegrateMode {
    #[default]
    Config,
    Link,
}

impl std::fmt::Display for IntegrateMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntegrateMode::Config => write!(f, "config"),
            IntegrateMode::Link => write!(f, "link"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolResourceIntegration {
    #[serde(default)]
    pub mode: IntegrateMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked: Vec<String>,
}

/// Top-level structure of integrate.local.toml
/// Keys are "tool.resource" e.g. "copilot-cli.skills", "vscode.agents"
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrateLocalConfig {
    #[serde(default, rename = "copilot-cli")]
    pub copilot_cli: IndexMap<String, ToolResourceIntegration>,
    #[serde(default)]
    pub vscode: IndexMap<String, ToolResourceIntegration>,
    #[serde(default, rename = "claude-code")]
    pub claude_code: IndexMap<String, ToolResourceIntegration>,
}

impl IntegrateLocalConfig {
    pub fn get(&self, tool: &str, resource: &str) -> Option<&ToolResourceIntegration> {
        match tool {
            "copilot-cli" => self.copilot_cli.get(resource),
            "vscode" => self.vscode.get(resource),
            "claude-code" => self.claude_code.get(resource),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, tool: &str, resource: &str) -> &mut ToolResourceIntegration {
        let map = match tool {
            "copilot-cli" => &mut self.copilot_cli,
            "vscode" => &mut self.vscode,
            "claude-code" => &mut self.claude_code,
            _ => &mut self.copilot_cli, // fallback
        };
        map.entry(resource.to_string()).or_default()
    }

    pub fn set_mode(&mut self, tool: &str, resource: &str, mode: IntegrateMode) {
        self.get_mut(tool, resource).mode = mode;
    }
}

pub fn load_integrate_local(root: &Path) -> Result<IntegrateLocalConfig> {
    let path = root.join("integrate.local.toml");
    if !path.exists() { return Ok(IntegrateLocalConfig::default()); }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

pub fn save_integrate_local(root: &Path, config: &IntegrateLocalConfig) -> Result<()> {
    let path = root.join("integrate.local.toml");
    let content = toml::to_string_pretty(config).context("serializing integrate.local.toml")?;
    std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}

// ── AI tool identifiers ──────────────────────────────────────────────────────

pub const TOOL_COPILOT_CLI: &str = "copilot-cli";
pub const TOOL_VSCODE: &str = "vscode";
pub const TOOL_CLAUDE_CODE: &str = "claude-code";

/// Returns the list of ResourceTypes supported by a given tool.
#[allow(dead_code)]
pub fn tool_supported_resources(tool: &str) -> &'static [ResourceType] {
    match tool {
        TOOL_COPILOT_CLI => &[ResourceType::Skills],
        TOOL_VSCODE => &[ResourceType::Skills, ResourceType::Agents, ResourceType::Instructions],
        TOOL_CLAUDE_CODE => &[ResourceType::Agents],
        _ => &[],
    }
}

// ── Config-file helpers ──────────────────────────────────────────────────────

fn home_dir_string() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string())
}

fn copilot_cli_config_path() -> PathBuf {
    PathBuf::from(home_dir_string()).join(".copilot").join("config.json")
}

fn vscode_settings_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("Code")
        .join("User")
        .join("settings.json")
}

fn claude_commands_dir() -> PathBuf {
    PathBuf::from(home_dir_string()).join(".claude").join("commands")
}

fn claude_commands_path() -> PathBuf {
    claude_commands_dir().join("ai-workspace")
}

/// Strip `//` line-comments and `/* */` block-comments from JSONC text so
/// that serde_json can parse it.  Not a fully-spec-compliant stripper, but
/// handles every pattern that appears in VS Code's settings.json in practice.
fn strip_jsonc_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(ch) = chars.next() {
        if escape_next {
            escape_next = false;
            out.push(ch);
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            out.push(ch);
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            continue;
        }
        if in_string {
            out.push(ch);
            continue;
        }
        // Outside strings: handle comments
        if ch == '/' {
            match chars.peek() {
                Some('/') => {
                    // line comment: skip to end of line
                    chars.next();
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    // block comment: skip until */
                    chars.next();
                    loop {
                        match chars.next() {
                            Some('*') if chars.peek() == Some(&'/') => {
                                chars.next();
                                break;
                            }
                            Some('\n') => out.push('\n'),
                            None => break,
                            _ => {}
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }
        out.push(ch);
    }
    out
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let clean = strip_jsonc_comments(&text);
    serde_json::from_str(&clean)
        .with_context(|| format!("parsing JSON from {}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text)
        .with_context(|| format!("writing {}", path.display()))
}

// ── Link-mode target paths ────────────────────────────────────────────────────

/// Returns the base directory in the AI tool where individual resources are linked.
fn link_target_base(tool: &str, resource: ResourceType) -> Option<PathBuf> {
    match (tool, resource) {
        (TOOL_COPILOT_CLI, ResourceType::Skills) => {
            Some(PathBuf::from(home_dir_string()).join(".copilot").join("skills"))
        }
        (TOOL_VSCODE, ResourceType::Skills) => {
            // Read first configured directory from settings.json, or use default
            vscode_link_base_from_settings("chat.agentSkillsLocations")
                .or_else(|| Some(PathBuf::from(home_dir_string()).join(".ai-vscode").join("skills")))
        }
        (TOOL_VSCODE, ResourceType::Agents) => {
            vscode_link_base_from_settings("chat.agentFilesLocations")
                .or_else(|| Some(PathBuf::from(home_dir_string()).join(".ai-vscode").join("agents")))
        }
        (TOOL_VSCODE, ResourceType::Instructions) => {
            vscode_link_base_from_settings("chat.instructionsFilesLocations")
                .or_else(|| Some(PathBuf::from(home_dir_string()).join(".ai-vscode").join("instructions")))
        }
        (TOOL_CLAUDE_CODE, ResourceType::Agents) => {
            Some(claude_commands_dir())
        }
        _ => None,
    }
}

/// Read the first key from a VS Code map-style setting to use as link base directory.
fn vscode_link_base_from_settings(setting_key: &str) -> Option<PathBuf> {
    let path = vscode_settings_path();
    if !path.exists() { return None; }
    let v = read_json(&path).ok()?;
    let map = v.get(setting_key)?.as_object()?;
    map.keys().next().map(|k| {
        PathBuf::from(k.trim_end_matches('\\').trim_end_matches('/'))
    })
}

/// Returns the full path where a specific resource should be linked in the AI tool.
pub fn link_target_path(tool: &str, resource: ResourceType, key: &str) -> Option<PathBuf> {
    let base = link_target_base(tool, resource)?;
    Some(match resource {
        ResourceType::Skills | ResourceType::Hooks => base.join(key),
        ResourceType::Agents => {
            if tool == TOOL_CLAUDE_CODE {
                base.join(format!("{}.md", key))
            } else {
                base.join(format!("{}.agent.md", key))
            }
        }
        ResourceType::Instructions => base.join(format!("{}.instructions.md", key)),
        ResourceType::Workflows => base.join(format!("{}.md", key)),
    })
}

// ── Link-mode operations ─────────────────────────────────────────────────────

/// Create a link from the workspace resource to the AI tool's resource directory.
pub fn create_tool_link(root: &Path, tool: &str, resource: ResourceType, key: &str) -> Result<()> {
    let src = crate::resource::target_path(root, resource, key);
    if !src.exists() {
        anyhow::bail!(
            "Resource {} '{}' not found at {}",
            resource.name(), key, src.display()
        );
    }

    let dst = link_target_path(tool, resource, key)
        .ok_or_else(|| anyhow::anyhow!(
            "No link target defined for {}:{}", tool, resource.name()
        ))?;

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }

    // Remove existing link/file if present
    if dst.exists() || is_junction_or_link(&dst) {
        if resource.is_dir() {
            let _ = std::fs::remove_dir(&dst);
        } else {
            let _ = std::fs::remove_file(&dst);
        }
    }

    if resource.is_dir() {
        junction::create(&src, &dst)
            .with_context(|| format!("creating junction {} -> {}", dst.display(), src.display()))?;
    } else {
        std::fs::hard_link(&src, &dst)
            .with_context(|| format!("creating hard link {} -> {}", dst.display(), src.display()))?;
    }

    Ok(())
}

/// Remove a link from the AI tool's resource directory.
pub fn remove_tool_link(tool: &str, resource: ResourceType, key: &str) -> Result<()> {
    let dst = match link_target_path(tool, resource, key) {
        Some(p) => p,
        None => return Ok(()),
    };

    if dst.exists() || is_junction_or_link(&dst) {
        if resource.is_dir() {
            std::fs::remove_dir(&dst)
                .with_context(|| format!("removing junction {}", dst.display()))?;
        } else {
            std::fs::remove_file(&dst)
                .with_context(|| format!("removing link {}", dst.display()))?;
        }
    }
    Ok(())
}

fn is_junction_or_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Sync links for a specific tool+resource: add missing, remove stale.
/// Returns (added, removed) counts.
pub fn sync_tool_links(
    root: &Path,
    tool: &str,
    resource: ResourceType,
    installed_keys: &[String],
    integrate_cfg: &mut IntegrateLocalConfig,
) -> Result<(usize, usize)> {
    let tri = integrate_cfg.get_mut(tool, resource.name());
    if tri.mode != IntegrateMode::Link {
        return Ok((0, 0));
    }

    let installed_set: std::collections::HashSet<&str> =
        installed_keys.iter().map(|s| s.as_str()).collect();
    let linked_set: std::collections::HashSet<String> =
        tri.linked.iter().cloned().collect();

    let mut added = 0usize;
    let mut removed = 0usize;

    // Add missing links (installed but not linked)
    for key in installed_keys {
        if !linked_set.contains(key.as_str()) {
            match create_tool_link(root, tool, resource, key) {
                Ok(()) => {
                    println!("  {} link {} '{}' → created", tool, resource.name(), key);
                    added += 1;
                }
                Err(e) => {
                    eprintln!("  {} link {} '{}' → error: {}", tool, resource.name(), key, e);
                }
            }
        }
    }

    // Remove stale links (linked but no longer installed)
    for key in &linked_set {
        if !installed_set.contains(key.as_str()) {
            match remove_tool_link(tool, resource, key) {
                Ok(()) => {
                    println!("  {} unlink {} '{}' → removed", tool, resource.name(), key);
                    removed += 1;
                }
                Err(e) => {
                    eprintln!("  {} unlink {} '{}' → error: {}", tool, resource.name(), key, e);
                }
            }
        }
    }

    // Update linked list to match installed
    tri.linked = installed_keys.to_vec();

    Ok((added, removed))
}

/// Ensure the link-mode base directory is registered in the tool's config
/// (e.g., for VS Code, add the link base dir to settings.json if not present).
pub fn ensure_link_base_registered(tool: &str, resource: ResourceType) -> Result<()> {
    match tool {
        TOOL_COPILOT_CLI => {
            if resource == ResourceType::Skills {
                let config_path = copilot_cli_config_path();
                if !config_path.exists() { return Ok(()); }
                let mut v = read_json(&config_path)?;
                let base = link_target_base(tool, resource).unwrap();
                if ensure_dir_in_array(&mut v, "skill_directories", &base) {
                    write_json(&config_path, &v)?;
                    println!("  Copilot CLI: added {} to skill_directories", base.display());
                }
            }
        }
        TOOL_VSCODE => {
            let config_path = vscode_settings_path();
            if !config_path.exists() { return Ok(()); }
            let setting_key = match resource {
                ResourceType::Skills => "chat.agentSkillsLocations",
                ResourceType::Agents => "chat.agentFilesLocations",
                ResourceType::Instructions => "chat.instructionsFilesLocations",
                _ => return Ok(()),
            };
            // Only register if we're using a fallback directory (not one already in settings)
            if vscode_link_base_from_settings(setting_key).is_none() {
                let base = link_target_base(tool, resource).unwrap();
                let mut v = read_json(&config_path)?;
                if ensure_dir_in_vscode_map(&mut v, setting_key, &base) {
                    write_json(&config_path, &v)?;
                    println!("  VS Code: added {} to {}", base.display(), setting_key);
                }
            }
        }
        TOOL_CLAUDE_CODE => {
            // Claude commands dir is auto-scanned, no config registration needed
        }
        _ => {}
    }
    Ok(())
}

// ── Status display ───────────────────────────────────────────────────────────

fn check_mark(ok: bool, label: &str) {
    let mark = if ok { "✓" } else { " " };
    println!("  [{}] {}", mark, label);
}

pub fn print_status(root: &Path) {
    let integrate_cfg = load_integrate_local(root).unwrap_or_default();
    let skills_dir = root.join("skills");
    let instrs_dir = root.join("instructions");
    let agents_dir = root.join("agents");

    // ── Copilot CLI ──
    let copilot_path = copilot_cli_config_path();
    println!("Copilot CLI  {}", copilot_path.display());
    if !copilot_path.exists() {
        println!("  [!] config file not found");
    } else {
        let mode = integrate_cfg.get(TOOL_COPILOT_CLI, "skills")
            .map(|t| t.mode).unwrap_or_default();
        match mode {
            IntegrateMode::Config => {
                let ok = read_json(&copilot_path)
                    .map(|v| dir_in_array(&v, "skill_directories", &skills_dir))
                    .unwrap_or(false);
                check_mark(ok, &format!("skills  config  skill_directories → {}", skills_dir.display()));
            }
            IntegrateMode::Link => {
                let tri = integrate_cfg.get(TOOL_COPILOT_CLI, "skills").unwrap();
                let cnt = tri.linked.len();
                check_mark(cnt > 0, &format!("skills  link  ({} linked)", cnt));
            }
        }
    }

    // ── VS Code ──
    println!();
    let vscode_path = vscode_settings_path();
    println!("VS Code  {}", vscode_path.display());
    if !vscode_path.exists() {
        println!("  [!] settings.json not found");
    } else {
        for (res, setting_key, dir) in [
            (ResourceType::Skills, "chat.agentSkillsLocations", &skills_dir),
            (ResourceType::Agents, "chat.agentFilesLocations", &agents_dir),
            (ResourceType::Instructions, "chat.instructionsFilesLocations", &instrs_dir),
        ] {
            let mode = integrate_cfg.get(TOOL_VSCODE, res.name())
                .map(|t| t.mode).unwrap_or_default();
            match mode {
                IntegrateMode::Config => {
                    let ok = read_json(&vscode_path)
                        .map(|v| dir_in_vscode_map(&v, setting_key, dir))
                        .unwrap_or(false);
                    check_mark(ok, &format!("{}  config  {} → {}", res.name(), setting_key, dir.display()));
                }
                IntegrateMode::Link => {
                    let tri = integrate_cfg.get(TOOL_VSCODE, res.name()).unwrap();
                    let cnt = tri.linked.len();
                    check_mark(cnt > 0, &format!("{}  link  ({} linked)", res.name(), cnt));
                }
            }
        }
    }

    // ── Claude Code ──
    println!();
    let claude_dir = claude_commands_dir();
    println!("Claude Code  {}", claude_dir.display());
    let mode = integrate_cfg.get(TOOL_CLAUDE_CODE, "agents")
        .map(|t| t.mode).unwrap_or_default();
    match mode {
        IntegrateMode::Config => {
            let junction = claude_commands_path();
            check_mark(junction.exists(), "agents  config  junction → ai-workspace");
        }
        IntegrateMode::Link => {
            let tri = integrate_cfg.get(TOOL_CLAUDE_CODE, "agents").unwrap();
            let cnt = tri.linked.len();
            check_mark(cnt > 0, &format!("agents  link  ({} linked)", cnt));
        }
    }
}

// ── Copilot CLI integration ──────────────────────────────────────────────────

fn dir_in_array(v: &Value, key: &str, dir: &Path) -> bool {
    let dir_str = dir.to_string_lossy().to_string();
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().any(|s| {
            s.as_str().map(|p| paths_equal(p, &dir_str)).unwrap_or(false)
        }))
        .unwrap_or(false)
}

fn paths_equal(a: &str, b: &str) -> bool {
    // Normalise separators and trailing slashes for comparison
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

pub fn integrate_copilot_cli(root: &Path, _resources: &[ResourceType], dry_run: bool, mode: IntegrateMode) -> Result<()> {
    let skills_dir = root.join("skills");
    let config_path = copilot_cli_config_path();

    // Save mode to integrate.local.toml
    let mut integrate_cfg = load_integrate_local(root)?;
    integrate_cfg.set_mode(TOOL_COPILOT_CLI, "skills", mode);

    match mode {
        IntegrateMode::Config => {
            if !config_path.exists() {
                println!("Copilot CLI config not found at {}", config_path.display());
                println!("Ensure GitHub Copilot CLI is installed and has been launched at least once.");
                save_integrate_local(root, &integrate_cfg)?;
                return Ok(());
            }

            let mut v = read_json(&config_path)?;
            let changed = ensure_dir_in_array(&mut v, "skill_directories", &skills_dir);

            if !changed {
                println!("Copilot CLI: already configured, no changes needed.");
            } else if dry_run {
                println!("Copilot CLI (dry-run): would update {}", config_path.display());
                println!("  skill_directories += {}", skills_dir.display());
            } else {
                write_json(&config_path, &v)?;
                println!("Copilot CLI: updated {}", config_path.display());
                println!("  skill_directories += {}", skills_dir.display());
            }

            // Remove any stale links from previous link mode
            let prev_linked = integrate_cfg.get_mut(TOOL_COPILOT_CLI, "skills").linked.clone();
            for key in &prev_linked {
                let _ = remove_tool_link(TOOL_COPILOT_CLI, ResourceType::Skills, key);
            }
            integrate_cfg.get_mut(TOOL_COPILOT_CLI, "skills").linked.clear();
        }
        IntegrateMode::Link => {
            // Ensure the link base directory is registered in the tool's config
            ensure_link_base_registered(TOOL_COPILOT_CLI, ResourceType::Skills)?;

            // Get installed resources and sync links
            let merged = crate::config::merged(root)?;
            let keys: Vec<String> = ResourceType::Skills.config_map(&merged).keys().cloned().collect();

            if dry_run {
                println!("Copilot CLI (dry-run): would create {} skill links", keys.len());
                for key in &keys { println!("  link skills/{}", key); }
            } else {
                let (added, removed) = sync_tool_links(root, TOOL_COPILOT_CLI, ResourceType::Skills, &keys, &mut integrate_cfg)?;
                println!("Copilot CLI: link sync completed ({} added, {} removed)", added, removed);
            }
        }
    }

    save_integrate_local(root, &integrate_cfg)?;
    Ok(())
}

fn ensure_dir_in_array(v: &mut Value, key: &str, dir: &Path) -> bool {
    let dir_str = dir.to_string_lossy().to_string();
    let arr = v.as_object_mut()
        .and_then(|o| o.entry(key).or_insert_with(|| Value::Array(vec![])).as_array_mut());
    if let Some(arr) = arr {
        if !arr.iter().any(|s| s.as_str().map(|p| paths_equal(p, &dir_str)).unwrap_or(false)) {
            arr.push(Value::String(dir_str));
            return true;
        }
    }
    false
}

// ── VS Code integration ───────────────────────────────────────────────────────

/// Check whether `dir` appears as a key in a VS Code map-style setting.
fn dir_in_vscode_map(v: &Value, setting_key: &str, dir: &Path) -> bool {
    let dir_str = format!("{}\\", dir.to_string_lossy().trim_end_matches('\\'));
    v.get(setting_key)
        .and_then(|o| o.as_object())
        .map(|m| m.keys().any(|k| paths_equal(k, &dir_str) || paths_equal(k, dir.to_str().unwrap_or(""))))
        .unwrap_or(false)
}

/// Ensure `dir` is present as a key (→ true) in a VS Code map-style setting.
/// Returns true if a change was made.
fn ensure_dir_in_vscode_map(v: &mut Value, setting_key: &str, dir: &Path) -> bool {
    let dir_str = format!("{}\\", dir.to_string_lossy().trim_end_matches('\\'));
    if dir_in_vscode_map(v, setting_key, dir) {
        return false;
    }
    let obj = v.as_object_mut().expect("settings.json root is not an object");
    let map = obj.entry(setting_key)
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(m) = map.as_object_mut() {
        m.insert(dir_str, Value::Bool(true));
    }
    true
}

pub fn integrate_vscode(root: &Path, resources: &[ResourceType], dry_run: bool, mode: IntegrateMode) -> Result<()> {
    let config_path = vscode_settings_path();

    // Save mode to integrate.local.toml
    let mut integrate_cfg = load_integrate_local(root)?;
    for r in resources {
        integrate_cfg.set_mode(TOOL_VSCODE, r.name(), mode);
    }

    match mode {
        IntegrateMode::Config => {
            if !config_path.exists() {
                println!("VS Code settings not found at {}", config_path.display());
                println!("Ensure VS Code is installed and has been launched at least once.");
                save_integrate_local(root, &integrate_cfg)?;
                return Ok(());
            }

            let mut v = read_json(&config_path)?;
            let targets: Vec<(&str, std::path::PathBuf)> = resources.iter().filter_map(|r| match r {
                ResourceType::Skills       => Some(("chat.agentSkillsLocations",       root.join("skills"))),
                ResourceType::Agents       => Some(("chat.agentFilesLocations",        root.join("agents"))),
                ResourceType::Instructions => Some(("chat.instructionsFilesLocations", root.join("instructions"))),
                _ => None,
            }).collect();

            let mut changes: Vec<(&str, std::path::PathBuf)> = Vec::new();
            for (key, dir) in &targets {
                if ensure_dir_in_vscode_map(&mut v, key, dir) {
                    changes.push((key, dir.clone()));
                }
            }

            if changes.is_empty() {
                println!("VS Code: already configured, no changes needed.");
            } else if dry_run {
                println!("VS Code (dry-run): would update {}", config_path.display());
                for (key, dir) in &changes {
                    let dir_str = format!("{}\\", dir.to_string_lossy().trim_end_matches('\\'));
                    println!("  {key} += \"{}\" => true", dir_str);
                }
            } else {
                write_json(&config_path, &v)?;
                println!("VS Code: updated {}", config_path.display());
                for (key, dir) in &changes {
                    let dir_str = format!("{}\\", dir.to_string_lossy().trim_end_matches('\\'));
                    println!("  {key} += \"{}\" => true", dir_str);
                }
            }

            // Remove stale links from previous link mode
            for r in resources {
                let prev_linked = integrate_cfg.get_mut(TOOL_VSCODE, r.name()).linked.clone();
                for key in &prev_linked {
                    let _ = remove_tool_link(TOOL_VSCODE, *r, key);
                }
                integrate_cfg.get_mut(TOOL_VSCODE, r.name()).linked.clear();
            }
        }
        IntegrateMode::Link => {
            let merged = crate::config::merged(root)?;

            for r in resources {
                ensure_link_base_registered(TOOL_VSCODE, *r)?;
                let keys: Vec<String> = r.config_map(&merged).keys().cloned().collect();

                if dry_run {
                    println!("VS Code (dry-run): would create {} {} links", keys.len(), r.name());
                    for key in &keys { println!("  link {}/{}", r.name(), key); }
                } else {
                    let (added, removed) = sync_tool_links(root, TOOL_VSCODE, *r, &keys, &mut integrate_cfg)?;
                    println!("VS Code {}: link sync completed ({} added, {} removed)", r.name(), added, removed);
                }
            }
        }
    }

    save_integrate_local(root, &integrate_cfg)?;
    Ok(())
}

// ── Claude Code integration ──────────────────────────────────────────────────

pub fn integrate_claude_code(root: &Path, _resources: &[ResourceType], dry_run: bool, mode: IntegrateMode) -> Result<()> {
    let agents_dir = root.join("agents");

    // Save mode to integrate.local.toml
    let mut integrate_cfg = load_integrate_local(root)?;
    integrate_cfg.set_mode(TOOL_CLAUDE_CODE, "agents", mode);

    match mode {
        IntegrateMode::Config => {
            let junction = claude_commands_path();

            if junction.exists() {
                println!("Claude Code: junction already exists at {}", junction.display());
            } else if dry_run {
                println!("Claude Code (dry-run): would create junction");
                println!("  {} -> {}", junction.display(), agents_dir.display());
            } else {
                if let Some(parent) = junction.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                junction::create(&agents_dir, &junction)
                    .with_context(|| format!(
                        "creating junction {} -> {}",
                        junction.display(), agents_dir.display()
                    ))?;
                println!("Claude Code: created junction");
                println!("  {} -> {}", junction.display(), agents_dir.display());
                println!("  Agents are now available as Claude Code slash commands under /ai-workspace/");
            }

            // Remove stale links from previous link mode
            let prev_linked = integrate_cfg.get_mut(TOOL_CLAUDE_CODE, "agents").linked.clone();
            for key in &prev_linked {
                let _ = remove_tool_link(TOOL_CLAUDE_CODE, ResourceType::Agents, key);
            }
            integrate_cfg.get_mut(TOOL_CLAUDE_CODE, "agents").linked.clear();
        }
        IntegrateMode::Link => {
            // Get installed resources and sync links
            let merged = crate::config::merged(root)?;
            let keys: Vec<String> = ResourceType::Agents.config_map(&merged).keys().cloned().collect();

            if dry_run {
                println!("Claude Code (dry-run): would create {} agent links", keys.len());
                for key in &keys { println!("  link agents/{}", key); }
            } else {
                let (added, removed) = sync_tool_links(root, TOOL_CLAUDE_CODE, ResourceType::Agents, &keys, &mut integrate_cfg)?;
                println!("Claude Code: link sync completed ({} added, {} removed)", added, removed);
            }
        }
    }

    save_integrate_local(root, &integrate_cfg)?;
    Ok(())
}

// ── All ───────────────────────────────────────────────────────────────────────

/// `resources` — empty means "all supported per tool"; non-empty filters by type.
pub fn integrate_all(root: &Path, resources: &[ResourceType], dry_run: bool, mode: IntegrateMode) -> Result<()> {
    // Build per-tool resource lists: use all supported if none specified
    let copilot_res: Vec<ResourceType> = if resources.is_empty() {
        vec![ResourceType::Skills]
    } else {
        resources.iter().filter(|r| **r == ResourceType::Skills).cloned().collect()
    };
    let vscode_supported = [ResourceType::Skills, ResourceType::Agents, ResourceType::Instructions];
    let vscode_res: Vec<ResourceType> = if resources.is_empty() {
        vscode_supported.to_vec()
    } else {
        resources.iter().filter(|r| vscode_supported.contains(r)).cloned().collect()
    };
    let claude_res: Vec<ResourceType> = if resources.is_empty() {
        vec![ResourceType::Agents]
    } else {
        resources.iter().filter(|r| **r == ResourceType::Agents).cloned().collect()
    };

    println!("=== Copilot CLI ===");
    if copilot_res.is_empty() {
        println!("Copilot CLI: no supported resources in selection (supported: skills), skipping.");
    } else {
        integrate_copilot_cli(root, &copilot_res, dry_run, mode)?;
    }
    println!();
    println!("=== VS Code ===");
    if vscode_res.is_empty() {
        println!("VS Code: no supported resources in selection (supported: skills, agents, instructions), skipping.");
    } else {
        integrate_vscode(root, &vscode_res, dry_run, mode)?;
    }
    println!();
    println!("=== Claude Code ===");
    if claude_res.is_empty() {
        println!("Claude Code: no supported resources in selection (supported: agents), skipping.");
    } else {
        integrate_claude_code(root, &claude_res, dry_run, mode)?;
    }
    Ok(())
}

// ── Apply-time link sync ─────────────────────────────────────────────────────

/// Called during `apply` to sync all link-mode integrations.
pub fn apply_sync_links(root: &Path) -> Result<()> {
    let mut integrate_cfg = load_integrate_local(root)?;
    let merged = crate::config::merged(root)?;
    let mut any_change = false;

    let tools: &[(&str, &[ResourceType])] = &[
        (TOOL_COPILOT_CLI, &[ResourceType::Skills]),
        (TOOL_VSCODE,      &[ResourceType::Skills, ResourceType::Agents, ResourceType::Instructions]),
        (TOOL_CLAUDE_CODE,  &[ResourceType::Agents]),
    ];

    for &(tool, supported) in tools {
        for &res in supported {
            let mode = integrate_cfg.get(tool, res.name())
                .map(|t| t.mode).unwrap_or_default();
            if mode != IntegrateMode::Link { continue; }

            let keys: Vec<String> = res.config_map(&merged).keys().cloned().collect();
            let (added, removed) = sync_tool_links(root, tool, res, &keys, &mut integrate_cfg)?;
            if added > 0 || removed > 0 {
                any_change = true;
            }
        }
    }

    if any_change {
        save_integrate_local(root, &integrate_cfg)?;
    }

    Ok(())
}
