use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Duration;

use reqwest::blocking::{Client, multipart};

pub fn client() -> Result<Client, String> {
    Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Could not create HTTP client: {e}"))
}

pub fn login(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let response = client
        .post(format!("{base_url}/api/v2/auth/login"))
        .form(&[("username", username), ("password", password)])
        .send()
        .map_err(|e| format!("Login request failed: {e}"))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    // qBittorrent 4.x answers 200 with "Ok." or "Fails."; 5.x answers 204
    // with an empty body on success.
    if status.is_success() && matches!(body.trim(), "" | "Ok.") {
        Ok(())
    } else {
        Err(format!("Login failed (HTTP {}): {}", status.as_u16(), body.trim()))
    }
}

/// Returns the instance's category names, sorted alphabetically.
pub fn fetch_categories(client: &Client, base_url: &str) -> Result<Vec<String>, String> {
    let response = client
        .get(format!("{base_url}/api/v2/torrents/categories"))
        .send()
        .map_err(|e| format!("Category request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Category request failed (HTTP {})", response.status().as_u16()));
    }
    // The response maps category name -> details; the BTreeMap keys give us
    // the sorted names directly.
    let categories: BTreeMap<String, serde_json::Value> = response
        .json()
        .map_err(|e| format!("Could not parse category list: {e}"))?;
    Ok(categories.into_keys().collect())
}

/// Hashes of all torrents currently known to the instance.
pub fn list_hashes(client: &Client, base_url: &str) -> Result<HashSet<String>, String> {
    let response = client
        .get(format!("{base_url}/api/v2/torrents/info"))
        .send()
        .map_err(|e| format!("Torrent list request failed: {e}"))?;
    let torrents: Vec<serde_json::Value> = response
        .json()
        .map_err(|e| format!("Could not parse torrent list: {e}"))?;
    Ok(torrents
        .iter()
        .filter_map(|t| t["hash"].as_str().map(String::from))
        .collect())
}

pub struct TorrentFile {
    pub id: i64,
    pub name: String,
    pub size: u64,
}

/// The files of one torrent; empty until (magnet) metadata is available.
pub fn fetch_files(client: &Client, base_url: &str, hash: &str) -> Result<Vec<TorrentFile>, String> {
    let response = client
        .get(format!("{base_url}/api/v2/torrents/files"))
        .query(&[("hash", hash)])
        .send()
        .map_err(|e| format!("File list request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("File list request failed (HTTP {})", response.status().as_u16()));
    }
    let files: Vec<serde_json::Value> = response
        .json()
        .map_err(|e| format!("Could not parse file list: {e}"))?;
    Ok(files
        .iter()
        .enumerate()
        .map(|(position, f)| TorrentFile {
            // Older API versions have no "index" field; there the position in
            // the array is the file id.
            id: f["index"].as_i64().unwrap_or(position as i64),
            name: f["name"].as_str().unwrap_or("?").to_string(),
            size: f["size"].as_u64().unwrap_or(0),
        })
        .collect())
}

/// Marks the given file ids as "do not download".
pub fn skip_files(client: &Client, base_url: &str, hash: &str, ids: &[i64]) -> Result<(), String> {
    let id_list = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("|");
    let response = client
        .post(format!("{base_url}/api/v2/torrents/filePrio"))
        .form(&[("hash", hash), ("id", &id_list), ("priority", "0")])
        .send()
        .map_err(|e| format!("File priority request failed: {e}"))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("File priority request failed (HTTP {})", response.status().as_u16()))
    }
}

/// POSTs `hashes=<hash>` to the first of the given endpoints the server
/// knows; qBittorrent 5.x renamed pause/resume to stop/start.
fn torrent_action(
    client: &Client,
    base_url: &str,
    endpoints: &[&str],
    hash: &str,
) -> Result<(), String> {
    let mut last_status = 0;
    for endpoint in endpoints {
        let response = client
            .post(format!("{base_url}/api/v2/torrents/{endpoint}"))
            .form(&[("hashes", hash)])
            .send()
            .map_err(|e| format!("{endpoint} request failed: {e}"))?;
        last_status = response.status().as_u16();
        if response.status().is_success() {
            return Ok(());
        }
        if last_status != 404 {
            break;
        }
    }
    Err(format!("{} request failed (HTTP {last_status})", endpoints[0]))
}

pub fn start_torrent(client: &Client, base_url: &str, hash: &str) -> Result<(), String> {
    torrent_action(client, base_url, &["start", "resume"], hash)
}

pub fn stop_torrent(client: &Client, base_url: &str, hash: &str) -> Result<(), String> {
    torrent_action(client, base_url, &["stop", "pause"], hash)
}

pub fn delete_torrent(client: &Client, base_url: &str, hash: &str) -> Result<(), String> {
    let response = client
        .post(format!("{base_url}/api/v2/torrents/delete"))
        .form(&[("hashes", hash), ("deleteFiles", "true")])
        .send()
        .map_err(|e| format!("Delete request failed: {e}"))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Delete request failed (HTTP {})", response.status().as_u16()))
    }
}

/// Returns the instance's default save path (a path on the server).
pub fn fetch_default_save_path(client: &Client, base_url: &str) -> Result<String, String> {
    let response = client
        .get(format!("{base_url}/api/v2/app/defaultSavePath"))
        .send()
        .map_err(|e| format!("Save path request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Save path request failed (HTTP {})", response.status().as_u16()));
    }
    Ok(response.text().map_err(|e| e.to_string())?.trim().to_string())
}

pub fn add_magnet(
    client: &Client,
    base_url: &str,
    magnet: &str,
    category: Option<&str>,
    save_path: Option<&str>,
) -> Result<(), String> {
    // Always add stopped so files can be deselected before anything
    // downloads; qBittorrent 5.x wants "stopped", 4.x "paused" — the unknown
    // one is ignored.
    let mut form = vec![("urls", magnet), ("stopped", "true"), ("paused", "true")];
    if let Some(category) = category {
        form.push(("category", category));
        // Without auto torrent management the category's save path is
        // ignored and the torrent lands in the global default location.
        form.push(("autoTMM", "true"));
    }
    if let Some(save_path) = save_path {
        form.push(("savepath", save_path));
    }
    let response = client
        .post(format!("{base_url}/api/v2/torrents/add"))
        .form(&form)
        .send()
        .map_err(|e| format!("Request failed: {e}"))?;
    check_add_response(response.status().as_u16())
}

pub fn add_torrent_file(
    client: &Client,
    base_url: &str,
    path: &Path,
    category: Option<&str>,
    save_path: Option<&str>,
) -> Result<(), String> {
    let file_part = multipart::Part::bytes(
        std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?,
    )
    .file_name(
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "upload.torrent".to_string()),
    )
    .mime_str("application/x-bittorrent")
    .map_err(|e| e.to_string())?;
    let mut form = multipart::Form::new()
        .part("torrents", file_part)
        .text("stopped", "true")
        .text("paused", "true");
    if let Some(category) = category {
        form = form
            .text("category", category.to_string())
            .text("autoTMM", "true");
    }
    if let Some(save_path) = save_path {
        form = form.text("savepath", save_path.to_string());
    }
    let response = client
        .post(format!("{base_url}/api/v2/torrents/add"))
        .multipart(form)
        .send()
        .map_err(|e| format!("Request failed: {e}"))?;
    check_add_response(response.status().as_u16())
}

fn check_add_response(status: u16) -> Result<(), String> {
    // The ts.net endpoints answer 204 No Content instead of the plain 200 the
    // bash prototype saw on localhost, so accept any 2xx.
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(format!("Failed to add torrent. HTTP Status: {status}"))
    }
}

pub fn is_magnet(target: &str) -> bool {
    target.len() >= 7 && target[..7].eq_ignore_ascii_case("magnet:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnet_detection() {
        assert!(is_magnet("magnet:?xt=urn:btih:abc"));
        assert!(is_magnet("MAGNET:?xt=urn:btih:abc"));
        assert!(!is_magnet("/home/user/file.torrent"));
        assert!(!is_magnet("magnet"));
        assert!(!is_magnet(""));
    }

    #[test]
    fn add_response_check() {
        assert!(check_add_response(200).is_ok());
        assert!(check_add_response(204).is_ok());
        assert_eq!(
            check_add_response(415).unwrap_err(),
            "Failed to add torrent. HTTP Status: 415"
        );
    }
}
