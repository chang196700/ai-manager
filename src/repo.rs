use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub fn clone_repo(root: &Path, name: &str, url: &str, branch: Option<&str>) -> Result<()> {
    let dest = root.join("repo").join(name);
    if dest.exists() { return Ok(()); }
    std::fs::create_dir_all(root.join("repo"))?;
    let mut args = vec!["clone", "--depth=1"];
    if let Some(b) = branch {
        args.extend_from_slice(&["--branch", b]);
    }
    args.extend_from_slice(&[url, dest.to_str().unwrap_or(name)]);
    let status = Command::new("git").args(&args).status()?;
    if !status.success() { anyhow::bail!("git clone failed for {}", name); }
    Ok(())
}

pub fn update_repo(root: &Path, name: &str, branch: Option<&str>) -> Result<()> {
    let repo_path = root.join("repo").join(name);
    if !repo_path.exists() { anyhow::bail!("repo {} not cloned", name); }
    let dir = repo_path.to_str().unwrap_or(name);
    let fetch = Command::new("git")
        .args(["-C", dir, "fetch", "--depth=1", "origin"])
        .status()?;
    if !fetch.success() { anyhow::bail!("git fetch failed for {}", name); }
    let target = branch.map(|b| format!("origin/{}", b)).unwrap_or_else(|| "origin/HEAD".to_string());
    let reset = Command::new("git")
        .args(["-C", dir, "reset", "--hard", &target])
        .status()?;
    if !reset.success() { anyhow::bail!("git reset failed for {}", name); }
    Ok(())
}

pub fn is_cloned(root: &Path, name: &str) -> bool {
    root.join("repo").join(name).join(".git").exists()
}
