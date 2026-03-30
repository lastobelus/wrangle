use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSettings {
    #[serde(default)]
    log_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConfigDiscovery {
    pub project_dir: Option<PathBuf>,
    pub home_dir: PathBuf,
    pub active_models_file: PathBuf,
    pub active_settings_file: Option<PathBuf>,
    pub log_dir: PathBuf,
}

pub fn home_config_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".wrangle")
}

pub fn discover_project_config_dir(start: &Path) -> Option<PathBuf> {
    let home_dir = home_config_dir();
    let mut current = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };

    loop {
        let candidate = current.join(".wrangle");
        if candidate.is_dir() && candidate != home_dir {
            return Some(candidate);
        }

        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_nearest_project_wrangle_dir() {
        let temp_root =
            std::env::temp_dir().join(format!("wrangle-project-config-{}", std::process::id()));
        let nested = temp_root.join("a/b/c");
        std::fs::create_dir_all(temp_root.join(".wrangle")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let discovered = discover_project_config_dir(&nested).unwrap();
        assert_eq!(discovered, temp_root.join(".wrangle"));

        let _ = std::fs::remove_dir_all(temp_root);
    }
}

fn settings_path(dir: &Path) -> PathBuf {
    dir.join("config.json")
}

fn models_path(dir: &Path) -> PathBuf {
    dir.join("models.json")
}

fn resolve_path(base_dir: &Path, value: &str) -> PathBuf {
    let candidate = PathBuf::from(value);
    if candidate.is_absolute() {
        candidate
    } else {
        base_dir.join(candidate)
    }
}

async fn load_settings(path: &Path) -> Result<ProjectSettings> {
    if !path.exists() {
        return Ok(ProjectSettings::default());
    }

    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read wrangle settings: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse wrangle settings: {}", path.display()))
}

pub async fn discover_config(start: &Path) -> Result<ConfigDiscovery> {
    let home_dir = home_config_dir();
    let project_dir = discover_project_config_dir(start);

    let active_models_file = match project_dir.as_ref() {
        Some(project) if models_path(project).exists() => models_path(project),
        _ => models_path(&home_dir),
    };

    let project_settings_path = project_dir.as_ref().map(|dir| settings_path(dir));
    let home_settings_path = settings_path(&home_dir);

    let (active_settings_file, log_dir) = if let Some(project_settings_path) = project_settings_path
        && project_settings_path.exists()
    {
        let settings = load_settings(&project_settings_path).await?;
        let resolved = settings
            .log_dir
            .as_deref()
            .map(|value| {
                resolve_path(
                    project_settings_path.parent().unwrap_or(Path::new(".")),
                    value,
                )
            })
            .unwrap_or_else(|| home_dir.join("logs"));
        (Some(project_settings_path), resolved)
    } else if home_settings_path.exists() {
        let settings = load_settings(&home_settings_path).await?;
        let resolved = settings
            .log_dir
            .as_deref()
            .map(|value| resolve_path(home_settings_path.parent().unwrap_or(Path::new(".")), value))
            .unwrap_or_else(|| home_dir.join("logs"));
        (Some(home_settings_path), resolved)
    } else {
        (None, home_dir.join("logs"))
    };

    Ok(ConfigDiscovery {
        project_dir,
        home_dir,
        active_models_file,
        active_settings_file,
        log_dir,
    })
}
