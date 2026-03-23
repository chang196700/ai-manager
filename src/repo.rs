use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn clone_repo(root: &Path, name: &str, url: &str) -> Result<()> {
    let dest = root.join("repo").join(name);
    if dest.exists() { return Ok(()); }
    std::fs::create_dir_all(root.join("repo"))?;
    let status = Command::new("git")
        .args(["clone", "--depth=1", url, dest.to_str().unwrap_or(name)])
        .status()?;
    if !status.success() { anyhow::bail!("git clone failed for {}", name); }
    Ok(())
}

pub fn update_repo(root: &Path, name: &str) -> Result<()> {
    let repo_path = root.join("repo").join(name);
    if !repo_path.exists() { anyhow::bail!("repo {} not cloned", name); }
    let status = Command::new("git")
        .args(["-C", repo_path.to_str().unwrap_or(name), "pull"])
        .status()?;
    if !status.success() { anyhow::bail!("git pull failed for {}", name); }
    Ok(())
}

pub fn is_cloned(root: &Path, name: &str) -> bool {
    root.join("repo").join(name).join(".git").exists()
}
