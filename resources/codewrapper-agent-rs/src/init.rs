//! Init command - install codeagent skill

use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

/// Skill template content (embedded)
const SKILL_TEMPLATE: &str = include_str!("../assets/SKILL.md");

/// Run the init command
pub async fn run_init(force: bool) -> Result<()> {
    let skill_dir = get_skill_dir()?;
    let skill_file = skill_dir.join("SKILL.md");

    // Check if already installed
    if skill_file.exists() && !force {
        println!(
            "Codeagent skill is already installed at: {}",
            skill_file.display()
        );
        println!("Use --force to overwrite.");
        return Ok(());
    }

    // Create directory if needed
    tokio::fs::create_dir_all(&skill_dir)
        .await
        .with_context(|| format!("Failed to create skill directory: {}", skill_dir.display()))?;

    // Write skill file
    tokio::fs::write(&skill_file, SKILL_TEMPLATE)
        .await
        .with_context(|| format!("Failed to write skill file: {}", skill_file.display()))?;

    info!(path = %skill_file.display(), "Skill installed");
    println!("✅ Codeagent skill installed to: {}", skill_file.display());

    Ok(())
}

/// Get the skill installation directory
fn get_skill_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".claude").join("skills").join("codeagent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_skill_dir() {
        let dir = get_skill_dir().unwrap();
        assert!(dir.ends_with("codeagent"));
    }
}
