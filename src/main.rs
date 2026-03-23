mod cli;
mod config;
mod git;
mod repo;
mod resource;
mod tui;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli_args = cli::Cli::parse();

    // Init doesn't need an existing workspace root
    if let Some(cli::Commands::Init { dir, force, r#override: override_files }) = cli_args.command {
        return cli::init_workspace(dir, force, override_files);
    }

    let root = config::find_root()?;

    match cli_args.command {
        Some(cmd) => cli::run(&root, cmd),
        None => tui::run(root),
    }
}

