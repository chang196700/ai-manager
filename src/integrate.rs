use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use serde_json::Value;
use crate::resource::ResourceType;

// ── Config-file helpers ──────────────────────────────────────────────────────

fn copilot_cli_config_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".copilot").join("config.json")
}

fn vscode_settings_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("Code")
        .join("User")
        .join("settings.json")
}

fn claude_commands_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".claude").join("commands").join("ai-workspace")
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

// ── Status structs ───────────────────────────────────────────────────────────

pub struct CopilotCliStatus {
    pub config_path: PathBuf,
    pub config_exists: bool,
    pub skills_ok: bool,
}

pub struct VscodeStatus {
    pub config_path: PathBuf,
    pub config_exists: bool,
    pub instructions_ok: bool,
}

pub struct ClaudeStatus {
    pub junction_path: PathBuf,
    pub junction_ok: bool,
}

pub struct IntegrationStatus {
    pub copilot_cli: CopilotCliStatus,
    pub vscode: VscodeStatus,
    pub claude: ClaudeStatus,
}

// ── Status check ─────────────────────────────────────────────────────────────

pub fn check_status(root: &Path) -> IntegrationStatus {
    let skills_dir  = root.join("skills");
    let instrs_dir  = root.join("instructions");

    // Copilot CLI
    let copilot_path = copilot_cli_config_path();
    let skills_ok = if copilot_path.exists() {
        match read_json(&copilot_path) {
            Ok(v) => dir_in_array(&v, "skill_directories", &skills_dir),
            Err(_) => false,
        }
    } else {
        false
    };

    // VS Code
    let vscode_path = vscode_settings_path();
    let instructions_ok = if vscode_path.exists() {
        match read_json(&vscode_path) {
            Ok(v) => instructions_in_vscode(&v, &instrs_dir),
            Err(_) => false,
        }
    } else {
        false
    };

    // Claude
    let junction = claude_commands_path();
    let junction_ok = junction.exists();

    IntegrationStatus {
        copilot_cli: CopilotCliStatus {
            config_path: copilot_path.clone(),
            config_exists: copilot_path.exists(),
            skills_ok,
        },
        vscode: VscodeStatus {
            config_path: vscode_path.clone(),
            config_exists: vscode_path.exists(),
            instructions_ok,
        },
        claude: ClaudeStatus {
            junction_path: junction,
            junction_ok,
        },
    }
}

pub fn print_status(root: &Path) {
    let s = check_status(root);

    println!("Copilot CLI  {}", s.copilot_cli.config_path.display());
    if !s.copilot_cli.config_exists {
        println!("  [!] config file not found");
    } else {
        check_mark(s.copilot_cli.skills_ok, &format!("skill_directories  {}", root.join("skills").display()));
    }

    println!();
    println!("VS Code  {}", s.vscode.config_path.display());
    if !s.vscode.config_exists {
        println!("  [!] settings.json not found");
    } else {
        check_mark(s.vscode.instructions_ok, &format!("chat.instructionsFilesLocations  {}", root.join("instructions").display()));
    }

    println!();
    println!("Claude Code  {}", s.claude.junction_path.display());
    check_mark(s.claude.junction_ok, "junction created");
}

fn check_mark(ok: bool, label: &str) {
    let mark = if ok { "✓" } else { " " };
    println!("  [{}] {}", mark, label);
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

pub fn integrate_copilot_cli(root: &Path, _resources: &[ResourceType], dry_run: bool) -> Result<()> {
    // Only Skills is accepted (enforced by CopilotCliResource enum)
    let skills_dir = root.join("skills");
    let config_path = copilot_cli_config_path();

    if !config_path.exists() {
        println!("Copilot CLI config not found at {}", config_path.display());
        println!("Ensure GitHub Copilot CLI is installed and has been launched at least once.");
        return Ok(());
    }

    let mut v = read_json(&config_path)?;

    let changed = ensure_dir_in_array(&mut v, "skill_directories", &skills_dir);

    if !changed {
        println!("Copilot CLI: already configured, no changes needed.");
        return Ok(());
    }

    if dry_run {
        println!("Copilot CLI (dry-run): would update {}", config_path.display());
        println!("  skill_directories += {}", skills_dir.display());
    } else {
        write_json(&config_path, &v)?;
        println!("Copilot CLI: updated {}", config_path.display());
        println!("  skill_directories += {}", skills_dir.display());
    }
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

fn instructions_in_vscode(v: &Value, dir: &Path) -> bool {
    let dir_str = format!("{}\\", dir.to_string_lossy().trim_end_matches('\\'));
    v.get("chat.instructionsFilesLocations")
        .and_then(|o| o.as_object())
        .map(|m| m.keys().any(|k| paths_equal(k, &dir_str) || paths_equal(k, dir.to_str().unwrap_or(""))))
        .unwrap_or(false)
}

pub fn integrate_vscode(root: &Path, _resources: &[ResourceType], dry_run: bool) -> Result<()> {
    // Only Instructions is accepted (enforced by VscodeResource enum)
    let instrs_dir = root.join("instructions");
    let config_path = vscode_settings_path();

    if !config_path.exists() {
        println!("VS Code settings not found at {}", config_path.display());
        println!("Ensure VS Code is installed and has been launched at least once.");
        return Ok(());
    }

    let mut v = read_json(&config_path)?;
    let key = "chat.instructionsFilesLocations";
    // Normalise: trailing backslash
    let dir_str = format!("{}\\", instrs_dir.to_string_lossy().trim_end_matches('\\'));

    let already = instructions_in_vscode(&v, &instrs_dir);
    if already {
        println!("VS Code: already configured, no changes needed.");
        return Ok(());
    }

    if dry_run {
        println!("VS Code (dry-run): would add to {key} in {}", config_path.display());
        println!("  {} => true", dir_str);
        return Ok(());
    }

    // Ensure the key exists as an object
    let obj = v.as_object_mut().context("settings.json root is not an object")?;
    let loc = obj.entry(key).or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(map) = loc.as_object_mut() {
        map.insert(dir_str.clone(), Value::Bool(true));
    }
    write_json(&config_path, &v)?;
    println!("VS Code: updated {}", config_path.display());
    println!("  {key} += \"{}\" => true", dir_str);
    Ok(())
}

// ── Claude Code integration ──────────────────────────────────────────────────

pub fn integrate_claude_code(root: &Path, _resources: &[ResourceType], dry_run: bool) -> Result<()> {
    // Only Agents is accepted (enforced by ClaudeCodeResource enum)
    let agents_dir = root.join("agents");
    let junction    = claude_commands_path();

    if junction.exists() {
        println!("Claude Code: junction already exists at {}", junction.display());
        return Ok(());
    }

    if dry_run {
        println!("Claude Code (dry-run): would create junction");
        println!("  {} -> {}", junction.display(), agents_dir.display());
        return Ok(());
    }

    // Ensure parent directory exists
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
    Ok(())
}

// ── All ───────────────────────────────────────────────────────────────────────

/// `resources` — empty means "all supported per tool"; non-empty filters by type.
pub fn integrate_all(root: &Path, resources: &[ResourceType], dry_run: bool) -> Result<()> {
    // Build per-tool resource lists: use all supported if none specified
    let copilot_res: Vec<ResourceType> = if resources.is_empty() {
        vec![ResourceType::Skills]
    } else {
        resources.iter().filter(|r| **r == ResourceType::Skills).cloned().collect()
    };
    let vscode_res: Vec<ResourceType> = if resources.is_empty() {
        vec![ResourceType::Instructions]
    } else {
        resources.iter().filter(|r| **r == ResourceType::Instructions).cloned().collect()
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
        integrate_copilot_cli(root, &copilot_res, dry_run)?;
    }
    println!();
    println!("=== VS Code ===");
    if vscode_res.is_empty() {
        println!("VS Code: no supported resources in selection (supported: instructions), skipping.");
    } else {
        integrate_vscode(root, &vscode_res, dry_run)?;
    }
    println!();
    println!("=== Claude Code ===");
    if claude_res.is_empty() {
        println!("Claude Code: no supported resources in selection (supported: agents), skipping.");
    } else {
        integrate_claude_code(root, &claude_res, dry_run)?;
    }
    Ok(())
}
