use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::path::Path;
use crate::config;
use crate::resource::{self, OpDesc, ResourceType};
use crate::repo;
use crate::git;

// Known repos embedded at compile time
const KNOWN_REPOS_TOML: &str = include_str!("known_repos.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct KnownRepo {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KnownRepoFile {
    repos: Vec<KnownRepo>,
}

pub fn known_repos() -> Vec<KnownRepo> {
    toml::from_str::<KnownRepoFile>(KNOWN_REPOS_TOML)
        .map(|f| f.repos)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ResourceTypeCli {
    Skills,
    Agents,
    Instructions,
    Hooks,
    Workflows,
}

impl From<ResourceTypeCli> for ResourceType {
    fn from(v: ResourceTypeCli) -> Self {
        match v {
            ResourceTypeCli::Skills => ResourceType::Skills,
            ResourceTypeCli::Agents => ResourceType::Agents,
            ResourceTypeCli::Instructions => ResourceType::Instructions,
            ResourceTypeCli::Hooks => ResourceType::Hooks,
            ResourceTypeCli::Workflows => ResourceType::Workflows,
        }
    }
}

#[derive(Parser)]
#[command(name = "ai-manager", about = "Manage .ai workspace resources")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new .ai workspace
    Init {
        /// Target directory (default: current directory)
        dir: Option<std::path::PathBuf>,
        /// Create missing files without overwriting existing ones
        #[arg(long)]
        force: bool,
        /// Create and overwrite all files
        #[arg(long, conflicts_with = "force")]
        r#override: bool,
    },
    /// List available or installed resources
    List {
        #[arg(value_enum)]
        resource_type: ResourceTypeCli,
        #[arg(long)]
        installed: bool,
        /// Filter by repo or user group (e.g. "github-awesome-copilot" or "user:default")
        #[arg(long)]
        source: Option<String>,
    },
    /// Add a resource
    Add {
        #[arg(value_enum)]
        resource_type: ResourceTypeCli,
        /// relpath within the source (e.g. "skills/csharp-xunit" or "skills/csharp-xunit.agent.md")
        name: String,
        /// Source: "repo:<name>" or "user:<group>"
        #[arg(long)]
        source: String,
        /// Override the key (default: auto-generated)
        #[arg(long)]
        key: Option<String>,
        /// Save to config.local.toml
        #[arg(long)]
        local: bool,
    },
    /// Remove a resource by key
    Remove {
        #[arg(value_enum)]
        resource_type: ResourceTypeCli,
        key: String,
    },
    /// Show git status
    Status,
    /// Update (pull) repos
    Update {
        repo: Option<String>,
    },
    /// Apply all links from config (create missing links)
    Apply,
    /// Launch interactive TUI
    Tui,
    /// Manage repos
    #[command(subcommand)]
    Repo(RepoCommands),
    /// Git operations
    #[command(subcommand)]
    Git(GitCommands),
    /// Integrate with external AI tools (Copilot CLI, VS Code, Claude Code)
    #[command(subcommand)]
    Integrate(IntegrateCommands),
}

#[derive(Subcommand)]
pub enum RepoCommands {
    /// Add a repo to config
    Add {
        name: String,
        url: String,
        #[arg(long)]
        local: bool,
    },
    /// Remove a repo from config
    Remove {
        name: String,
    },
    /// Update (clone or pull) repos
    Update {
        /// Specific repo name (default: all)
        repo: Option<String>,
    },
    /// List configured repos
    List,
    /// Manage built-in known repos
    #[command(subcommand)]
    Known(KnownCommands),
}

#[derive(Subcommand)]
pub enum KnownCommands {
    /// List all built-in known repos
    List,
    /// Add a known repo to config by name
    Add {
        name: String,
        /// Save to config.local.toml
        #[arg(long)]
        local: bool,
    },
}

/// Resources supported by GitHub Copilot CLI integration
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CopilotCliResource {
    Skills,
}

impl From<CopilotCliResource> for ResourceType {
    fn from(v: CopilotCliResource) -> Self {
        match v {
            CopilotCliResource::Skills => ResourceType::Skills,
        }
    }
}

/// Resources supported by VS Code Copilot integration
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VscodeResource {
    Skills,
    Agents,
    Instructions,
}

impl From<VscodeResource> for ResourceType {
    fn from(v: VscodeResource) -> Self {
        match v {
            VscodeResource::Skills       => ResourceType::Skills,
            VscodeResource::Agents       => ResourceType::Agents,
            VscodeResource::Instructions => ResourceType::Instructions,
        }
    }
}

/// Resources supported by Claude Code integration
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClaudeCodeResource {
    Agents,
}

impl From<ClaudeCodeResource> for ResourceType {
    fn from(v: ClaudeCodeResource) -> Self {
        match v {
            ClaudeCodeResource::Agents => ResourceType::Agents,
        }
    }
}

#[derive(Subcommand)]
pub enum IntegrateCommands {
    /// Show integration status for all tools
    Status,
    /// Configure GitHub Copilot CLI skill directories
    CopilotCli {
        /// Resources to integrate. Must specify at least one.
        #[arg(value_enum, required = true)]
        resources: Vec<CopilotCliResource>,
        /// Show what would be changed without applying
        #[arg(long)]
        dry_run: bool,
    },
    /// Configure VS Code Copilot instruction paths
    Vscode {
        /// Resources to integrate. Must specify at least one.
        #[arg(value_enum, required = true)]
        resources: Vec<VscodeResource>,
        /// Show what would be changed without applying
        #[arg(long)]
        dry_run: bool,
    },
    /// Configure Claude Code agent commands directory
    ClaudeCode {
        /// Resources to integrate. Must specify at least one.
        #[arg(value_enum, required = true)]
        resources: Vec<ClaudeCodeResource>,
        /// Show what would be changed without applying
        #[arg(long)]
        dry_run: bool,
    },
    /// Integrate with all tools (optionally filter resource types)
    All {
        /// Resource types to include (default: all supported per tool)
        #[arg(value_enum)]
        resources: Vec<ResourceTypeCli>,
        /// Show what would be changed without applying
        #[arg(long)]
        dry_run: bool,
    },
}


#[derive(Subcommand)]
pub enum GitCommands {
    /// Show git status
    Status,
    /// Commit staged changes
    Commit {
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Push to remote
    Push,
}

fn do_repo_update(root: &Path, name_filter: Option<&str>) -> Result<()> {
    let cfg = config::merged(root)?;
    let repos: Vec<_> = if let Some(name) = name_filter {
        cfg.repos.iter().filter(|r| r.name == name).collect()
    } else {
        cfg.repos.iter().collect()
    };
    if repos.is_empty() {
        println!("No repos to update.");
    }
    for r in repos {
        if repo::is_cloned(root, &r.name) {
            print!("Updating {}... ", r.name);
            repo::update_repo(root, &r.name, r.branch.as_deref())?;
            println!("done");
        } else {
            print!("Cloning {}... ", r.name);
            repo::clone_repo(root, &r.name, &r.url, r.branch.as_deref())?;
            println!("done");
        }
    }
    Ok(())
}

pub fn init_workspace(dir: Option<std::path::PathBuf>, force: bool, override_files: bool) -> Result<()> {
    let target = match dir {
        Some(d) => d,
        None => std::env::current_dir().context("getting current dir")?,
    };

    std::fs::create_dir_all(&target)?;

    // Check if directory has content (excluding .git)
    let has_content = std::fs::read_dir(&target)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|e| e.file_name() != ".git")
        })
        .unwrap_or(false);

    if has_content && !force && !override_files {
        anyhow::bail!(
            "Directory '{}' is not empty. Use --force to create missing files, or --override to overwrite all files.",
            target.display()
        );
    }

    // Files to create: (relative path, content)
    let default_gitignore = "\
# Repo shallow clones
/repo/*/

# Manager binary (standalone git repo)
/manager/

# User local config (not synced)
config.local.toml
user.local/
";

    let default_config = "\
# .ai workspace configuration
# Add repos, skills, agents, instructions, hooks and workflows here.

# Example:
# [[repos]]
# name = \"awesome-copilot\"
# url = \"https://github.com/github/awesome-copilot\"
# branch = \"main\"
";

    let files: &[(&str, &str)] = &[
        (".gitignore",              default_gitignore),
        ("config.toml",             default_config),
        ("skills/.gitkeep",         ""),
        ("agents/.gitkeep",         ""),
        ("instructions/.gitkeep",   ""),
        ("hooks/.gitkeep",          ""),
        ("workflows/.gitkeep",      ""),
        ("repo/.gitkeep",           ""),
    ];

    for (rel, content) in files {
        let path = target.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() && !override_files {
            println!("  skip   {}", rel);
            continue;
        }
        std::fs::write(&path, content)?;
        println!("  create {}", rel);
    }

    // Init git repo if not already initialised
    let git_dir = target.join(".git");
    if !git_dir.exists() {
        let status = std::process::Command::new("git")
            .args(["init", target.to_str().unwrap_or(".")])
            .status()?;
        if !status.success() {
            anyhow::bail!("git init failed");
        }
        println!("  git init");
    }

    // Initial commit
    let add = std::process::Command::new("git")
        .current_dir(&target)
        .args(["add", "-A"])
        .status()?;
    if add.success() {
        let _ = std::process::Command::new("git")
            .current_dir(&target)
            .args(["commit", "-m", "Initial commit"])
            .status();
    }

    println!("\nWorkspace initialised at '{}'", target.display());
    Ok(())
}

pub fn run(root: &Path, cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Init { .. } => unreachable!("Init is handled before run()"),
        Commands::List { resource_type, installed, source } => {
            let rtype: ResourceType = resource_type.into();
            let config = config::merged(root)?;
            if installed {
                let items = resource::list_installed(root, &config, rtype);
                if items.is_empty() {
                    println!("No installed {} items.", rtype.name());
                } else {
                    println!("Installed {}:", rtype.name());
                    for (key, value, link_ok) in &items {
                        if let Some(ref s) = source {
                            if !value.contains(s.as_str()) { continue; }
                        }
                        let status = if *link_ok { "✓" } else { "✗" };
                        println!("  [{status}] {key} = {value}");
                    }
                }
            } else {
                let items = resource::list_available(root, &config, rtype)?;
                let filtered: Vec<_> = items.iter().filter(|item| {
                    source.as_ref().map_or(true, |s| item.display_source.contains(s.as_str()))
                }).collect();
                if filtered.is_empty() {
                    println!("No available {} items.", rtype.name());
                } else {
                    println!("Available {} ({} items):", rtype.name(), filtered.len());
                    for item in &filtered {
                        println!("  [key: {}]  {}  (from {})", item.suggested_key, item.source_value, item.display_source);
                    }
                }
            }
        }

        Commands::Add { resource_type, name, source, key, local } => {
            let rtype: ResourceType = resource_type.into();
            // Parse source "repo:<name>" or "user:<group>"
            let source_parts: Vec<&str> = source.splitn(2, ':').collect();
            if source_parts.len() != 2 {
                anyhow::bail!("--source must be 'repo:<name>' or 'user:<group>'");
            }
            let source_value = format!("{}:{}", source, name);

            // Auto-generate key if not provided
            let effective_key = if let Some(k) = key {
                k
            } else {
                // strip suffix if file type
                let stem = if let Some(suffix) = rtype.file_suffix() {
                    name.strip_suffix(suffix).unwrap_or(&name).to_string()
                } else {
                    name.clone()
                };
                // strip path prefix
                let basename = stem.split('/').last().unwrap_or(&stem).to_string();
                if source_parts[0] == "repo" {
                    format!("{}-{}", source_parts[1], basename)
                } else {
                    basename
                }
            };

            let mut shared = config::load_shared(root)?;
            let mut local_cfg = config::load_local(root)?;
            let op = resource::add_resource(root, &mut shared, &mut local_cfg, rtype, &source_value, &effective_key, local)?;

            if local {
                config::save_local(root, &local_cfg)?;
                println!("Added {} '{}' to config.local.toml", rtype.name(), effective_key);
            } else {
                config::save_shared(root, &shared)?;
                println!("Added {} '{}' to config.toml", rtype.name(), effective_key);
                git::auto_commit(root, &[op])?;
            }
        }

        Commands::Remove { resource_type, key } => {
            let rtype: ResourceType = resource_type.into();
            let mut shared = config::load_shared(root)?;
            let mut local_cfg = config::load_local(root)?;
            let op = resource::remove_resource(root, &mut shared, &mut local_cfg, rtype, &key)?;
            config::save_shared(root, &shared)?;
            config::save_local(root, &local_cfg)?;
            println!("Removed {} '{}'", rtype.name(), key);
            git::auto_commit(root, &[op])?;
        }

        Commands::Status => {
            let output = git::status(root)?;
            print!("{}", output);
        }

        Commands::Update { repo } => {
            do_repo_update(root, repo.as_deref())?;
        }

        Commands::Apply => {
            let mut shared = config::load_shared(root)?;
            let mut local_cfg = config::load_local(root)?;
            let merged = config::merged(root)?;
            let mut ops: Vec<OpDesc> = Vec::new();

            for rtype in ResourceType::all() {
                let rtype = *rtype;
                let entries: Vec<(String, String)> = rtype.config_map(&merged)
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (key, value) in entries {
                    let dst = resource::target_path(root, rtype, &key);
                    if !dst.exists() {
                        match resource::add_resource(root, &mut shared, &mut local_cfg, rtype, &value, &key, false) {
                            Ok(op) => {
                                println!("Linked {} '{}'", rtype.name(), key);
                                ops.push(op);
                            }
                            Err(e) => eprintln!("Error linking {} '{}': {}", rtype.name(), key, e),
                        }
                    }
                }
            }

            if !ops.is_empty() {
                config::save_shared(root, &shared)?;
            }
        }

        Commands::Tui => {
            crate::tui::run(root.to_path_buf())?;
        }

        Commands::Repo(repo_cmd) => match repo_cmd {
            RepoCommands::Add { name, url, local } => {
                let repo_cfg = crate::config::RepoConfig {
                    name: name.clone(),
                    url: url.clone(),
                    ..Default::default()
                };
                if local {
                    let mut cfg = config::load_local(root)?;
                    cfg.repos.retain(|r| r.name != name);
                    cfg.repos.push(repo_cfg);
                    config::save_local(root, &cfg)?;
                    println!("Added repo '{}' to config.local.toml", name);
                } else {
                    let mut cfg = config::load_shared(root)?;
                    cfg.repos.retain(|r| r.name != name);
                    cfg.repos.push(repo_cfg);
                    config::save_shared(root, &cfg)?;
                    println!("Added repo '{}' to config.toml", name);
                    let op = OpDesc {
                        op: crate::resource::OpType::RepoAdd,
                        resource_type: None,
                        key: name,
                    };
                    git::auto_commit(root, &[op])?;
                }
            }
            RepoCommands::Remove { name } => {
                let mut shared = config::load_shared(root)?;
                let mut local_cfg = config::load_local(root)?;
                let in_shared = shared.repos.iter().any(|r| r.name == name);
                let in_local  = local_cfg.repos.iter().any(|r| r.name == name);
                if !in_shared && !in_local {
                    anyhow::bail!("Repo '{}' not found in config.", name);
                }
                shared.repos.retain(|r| r.name != name);
                local_cfg.repos.retain(|r| r.name != name);
                config::save_shared(root, &shared)?;
                config::save_local(root, &local_cfg)?;
                println!("Removed repo '{}'", name);
                let op = OpDesc {
                    op: crate::resource::OpType::RepoAdd, // reuse closest type
                    resource_type: None,
                    key: name,
                };
                git::auto_commit(root, &[op])?;
            }
            RepoCommands::Update { repo } => {
                do_repo_update(root, repo.as_deref())?;
            }
            RepoCommands::List => {
                let config = config::merged(root)?;
                if config.repos.is_empty() {
                    println!("No repos configured.");
                } else {
                    println!("Repos:");
                    for r in &config.repos {
                        let cloned = if repo::is_cloned(root, &r.name) { "✓" } else { " " };
                        println!("  [{}] {} — {}", cloned, r.name, r.url);
                    }
                }
            }
            RepoCommands::Known(known_cmd) => {
                let all_known = known_repos();
                match known_cmd {
                    KnownCommands::List => {
                        let config = config::merged(root)?;
                        let configured_names: std::collections::HashSet<&str> =
                            config.repos.iter().map(|r| r.name.as_str()).collect();
                        if all_known.is_empty() {
                            println!("No known repos defined.");
                        } else {
                            println!("Known repos:");
                            for kr in &all_known {
                                let mark = if configured_names.contains(kr.name.as_str()) { "✓" } else { " " };
                                let desc = kr.description.as_deref().unwrap_or("");
                                let branch = kr.branch.as_deref().map(|b| format!(" [{}]", b)).unwrap_or_default();
                                println!("  [{}] {}{} — {}", mark, kr.name, branch, desc);
                            }
                        }
                    }
                    KnownCommands::Add { name, local } => {
                        let kr = all_known.iter().find(|r| r.name == name)
                            .ok_or_else(|| anyhow::anyhow!(
                                "Unknown repo '{}'. Run `repo known list` to see available repos.", name
                            ))?;
                        let repo_cfg = crate::config::RepoConfig {
                            name: kr.name.clone(),
                            url: kr.url.clone(),
                            branch: kr.branch.clone(),
                            ..Default::default()
                        };
                        if local {
                            let mut cfg = config::load_local(root)?;
                            cfg.repos.retain(|r| r.name != kr.name);
                            cfg.repos.push(repo_cfg);
                            config::save_local(root, &cfg)?;
                            println!("Added known repo '{}' to config.local.toml", kr.name);
                        } else {
                            let mut cfg = config::load_shared(root)?;
                            cfg.repos.retain(|r| r.name != kr.name);
                            cfg.repos.push(repo_cfg);
                            config::save_shared(root, &cfg)?;
                            println!("Added known repo '{}' to config.toml", kr.name);
                            let op = OpDesc {
                                op: crate::resource::OpType::RepoAdd,
                                resource_type: None,
                                key: kr.name.clone(),
                            };
                            git::auto_commit(root, &[op])?;
                        }
                    }
                }
            }
        },

        Commands::Git(git_cmd) => match git_cmd {
            GitCommands::Status => {
                let output = git::status(root)?;
                print!("{}", output);
            }
            GitCommands::Commit { message } => {
                git::manual_commit(root, message.as_deref())?;
                println!("Committed.");
            }
            GitCommands::Push => {
                git::push(root)?;
                println!("Pushed.");
            }
        },

        Commands::Integrate(int_cmd) => match int_cmd {
            IntegrateCommands::Status => {
                crate::integrate::print_status(root);
            }
            IntegrateCommands::CopilotCli { resources, dry_run } => {
                let res: Vec<ResourceType> = resources.into_iter().map(Into::into).collect();
                crate::integrate::integrate_copilot_cli(root, &res, dry_run)?;
            }
            IntegrateCommands::Vscode { resources, dry_run } => {
                let res: Vec<ResourceType> = resources.into_iter().map(Into::into).collect();
                crate::integrate::integrate_vscode(root, &res, dry_run)?;
            }
            IntegrateCommands::ClaudeCode { resources, dry_run } => {
                let res: Vec<ResourceType> = resources.into_iter().map(Into::into).collect();
                crate::integrate::integrate_claude_code(root, &res, dry_run)?;
            }
            IntegrateCommands::All { resources, dry_run } => {
                let res: Vec<ResourceType> = resources.into_iter().map(Into::into).collect();
                crate::integrate::integrate_all(root, &res, dry_run)?;
            }
        },
    }
    Ok(())
}
