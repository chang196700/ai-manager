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
    let root = config::find_root()?;

    match cli_args.command {
        Some(cmd) => cli::run(&root, cmd),
        None => tui::run(root),
    }
}

