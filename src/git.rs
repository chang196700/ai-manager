use anyhow::Result;
use std::path::Path;
use std::process::Command;
use crate::resource::OpDesc;

fn git_cmd(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").arg("-C").arg(root).args(args).output()?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn status(root: &Path) -> Result<String> {
    git_cmd(root, &["status"])
}

pub fn auto_commit(root: &Path, ops: &[OpDesc]) -> Result<()> {
    if ops.is_empty() { return Ok(()); }
    let msg = build_commit_message(ops);
    // Add config.toml and user/ (ignore errors if paths don't exist)
    let _ = git_cmd(root, &["add", "config.toml", "user/"]);
    let staged = git_cmd(root, &["diff", "--cached", "--name-only"])?;
    if staged.trim().is_empty() { return Ok(()); }
    git_cmd(root, &["commit", "-m", &msg])?;
    Ok(())
}

fn build_commit_message(ops: &[OpDesc]) -> String {
    let parts: Vec<String> = ops.iter().map(|o| o.to_message()).collect();
    format!("ai-manager: {}\n\nManaged by ai-manager", parts.join(", "))
}

pub fn manual_commit(root: &Path, msg_override: Option<&str>) -> Result<()> {
    let msg = if let Some(m) = msg_override {
        m.to_string()
    } else {
        let stat = git_cmd(root, &["diff", "--cached", "--stat"])?;
        if stat.trim().is_empty() {
            anyhow::bail!("Nothing staged to commit");
        }
        format!("ai-manager: manual commit\n\n{}", stat)
    };
    // Try to add; ignore errors if paths don't exist
    let _ = git_cmd(root, &["add", "--", "config.toml"]);
    let _ = git_cmd(root, &["add", "--", "user/"]);
    git_cmd(root, &["commit", "-m", &msg])?;
    Ok(())
}

pub fn push(root: &Path) -> Result<()> {
    git_cmd(root, &["push"])?;
    Ok(())
}
