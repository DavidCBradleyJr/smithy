//! User settings in `~/.config/smithy/settings.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Folders holding models, in priority order (if two folders have a model
    /// with the same name, the earlier folder wins). In each, a model is a
    /// single `.gguf` file, or a folder of weights plus an optional
    /// `mmproj*.gguf` vision tower. Folders may be on removable drives.
    pub model_dirs: Vec<String>,
    /// The llama-server that runs them. A fork that is a superset of upstream
    /// (like PrismML's, which Bonsai needs) can serve every model.
    pub llama_server: String,
    pub port: u16,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

impl Default for Settings {
    fn default() -> Self {
        let home = home();
        let bonsai_fork = home.join(".local/share/bonsai/bin/llama-server");
        Settings {
            model_dirs: vec![home.join("models").to_string_lossy().into_owned()],
            llama_server: if bonsai_fork.is_file() {
                bonsai_fork.to_string_lossy().into_owned()
            } else {
                "llama-server".into()
            },
            // 8081 is where the local model has always been served on this
            // machine, so existing clients (Open WebUI) keep working.
            port: 8081,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| home().join(".config")).join("smithy")
}

fn file(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

impl Settings {
    pub fn load_from(dir: &Path) -> Settings {
        let Some(mut raw) = std::fs::read_to_string(file(dir)).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            return Settings::default();
        };
        // Before multiple folders there was a single `models_dir`.
        if raw.get("model_dirs").is_none() {
            if let Some(old) = raw.get("models_dir").and_then(|v| v.as_str()).map(str::to_string) {
                raw["model_dirs"] = serde_json::json!([old]);
            }
        }
        serde_json::from_value(raw).unwrap_or_default()
    }

    pub fn load() -> Settings {
        Self::load_from(&config_dir())
    }

    pub fn save_to(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        std::fs::write(file(dir), serde_json::to_vec_pretty(self).unwrap()).map_err(|e| e.to_string())
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

#[tauri::command]
pub fn settings_get() -> Settings {
    Settings::load()
}

#[tauri::command]
pub fn settings_set(mut settings: Settings) -> Result<Settings, String> {
    settings.model_dirs.retain(|d| !d.trim().is_empty());
    settings.model_dirs.dedup();
    if settings.model_dirs.is_empty() {
        return Err("add at least one models folder".into());
    }
    // A folder on an unmounted drive is allowed; a typo in a new one isn't.
    let known = Settings::load().model_dirs;
    if let Some(bad) = settings.model_dirs.iter().find(|d| !Path::new(d).is_dir() && !known.contains(d)) {
        return Err(format!("{bad} is not a folder"));
    }
    settings.save_to(&config_dir())?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_single_folder_setting_migrates() {
        let dir = std::env::temp_dir().join(format!("smithy-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(file(&dir), r#"{"models_dir":"/m"}"#).unwrap();
        let s = Settings::load_from(&dir);
        std::fs::write(file(&dir), r#"{"model_dirs":["/a","/b"],"port":9000}"#).unwrap();
        let t = Settings::load_from(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(s.model_dirs, vec!["/m"]);
        assert_eq!(s.port, 8081);
        assert_eq!(t.model_dirs, vec!["/a", "/b"]);
        assert_eq!(t.port, 9000);
    }
}
