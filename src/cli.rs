use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::Path;
use crate::config;
use crate::resource::{self, OpDesc, ResourceType};
use crate::repo;
use crate::git;

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
    /// Manage repos
    #[command(subcommand)]
    Repo(RepoCommands),
    /// Git operations
    #[command(subcommand)]
    Git(GitCommands),
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
    /// List configured repos
    List,
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

pub fn run(root: &Path, cmd: Commands) -> Result<()> {
    match cmd {
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
            let config = config::merged(root)?;
            let repos: Vec<_> = if let Some(name) = repo {
                config.repos.iter().filter(|r| r.name == name).collect()
            } else {
                config.repos.iter().collect()
            };
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
    }
    Ok(())
}
