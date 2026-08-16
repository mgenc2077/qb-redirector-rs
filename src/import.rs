use std::path::{Path, PathBuf};

use serde::Deserialize;

/// `import-config.json` inside a folder opened with the redirector.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportConfig {
    pub category: Option<String>,
    pub download_path: Option<String>,
    /// Instance name from the main config; skips the instance chooser.
    pub instance: Option<String>,
}

pub fn load(folder: &Path) -> Result<ImportConfig, String> {
    let path = folder.join("import-config.json");
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    serde_json::from_str(&contents).map_err(|e| format!("Invalid {}: {e}", path.display()))
}

/// All `.torrent` files directly inside the folder, sorted by name.
pub fn torrent_files(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(folder)
        .map_err(|e| format!("Could not read {}: {e}", folder.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// The folder's torrents download into `<download_path>/<folder name>`.
pub fn save_path_for(download_path: &str, folder_name: &str) -> String {
    format!("{}/{}", download_path.trim_end_matches('/'), folder_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_path_joins_download_path_and_folder_name() {
        assert_eq!(
            save_path_for("/downloads/Jellyfin/Anime", "[ASW] Show Name"),
            "/downloads/Jellyfin/Anime/[ASW] Show Name"
        );
        assert_eq!(save_path_for("/downloads/", "x"), "/downloads/x");
    }

    #[test]
    fn example_config_parses() {
        let config: ImportConfig = serde_json::from_str(
            r#"{"category": "Anime", "downloadPath": "/downloads/Jellyfin/Anime"}"#,
        )
        .unwrap();
        assert_eq!(config.category.as_deref(), Some("Anime"));
        assert_eq!(config.download_path.as_deref(), Some("/downloads/Jellyfin/Anime"));
        assert_eq!(config.instance, None);
    }
}
