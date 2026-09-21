//! Live Omarchy theme: reads the current theme's `colors.toml` and pushes
//! changes to the UI. Polling (rather than inotify) survives Omarchy replacing
//! the whole `current/theme` directory on a theme switch.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tauri::{AppHandle, Emitter};

pub type Colors = BTreeMap<String, String>;

pub fn colors_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".local/state/omarchy/current/theme/colors.toml"))
}

/// Keeps only string values; anything else in the file is ignored.
pub fn parse_colors(src: &str) -> Colors {
    let Ok(table) = src.parse::<toml::Table>() else {
        return Colors::new();
    };
    table
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect()
}

pub fn read_theme() -> Colors {
    colors_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| parse_colors(&s))
        .unwrap_or_default()
}

pub fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = read_theme();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let current = read_theme();
            // An empty read is a theme switch caught mid-write; keep the old one.
            if !current.is_empty() && current != last {
                let _ = app.emit("theme-changed", &current);
                last = current;
            }
        }
    });
}

#[tauri::command]
pub fn get_theme() -> Colors {
    read_theme()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_values_only() {
        let c = parse_colors("mode = \"dark\"\naccent = \"#00f0ff\"\nopacity = 0.9\n");
        assert_eq!(c.get("accent").map(String::as_str), Some("#00f0ff"));
        assert_eq!(c.get("mode").map(String::as_str), Some("dark"));
        assert!(!c.contains_key("opacity"));
    }

    #[test]
    fn invalid_toml_is_empty() {
        assert!(parse_colors("accent = ").is_empty());
    }
}
