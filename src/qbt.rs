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

pub fn add_magnet(client: &Client, base_url: &str, magnet: &str) -> Result<(), String> {
    let response = client
        .post(format!("{base_url}/api/v2/torrents/add"))
        .form(&[("urls", magnet)])
        .send()
        .map_err(|e| format!("Request failed: {e}"))?;
    check_add_response(response.status().as_u16())
}

pub fn add_torrent_file(client: &Client, base_url: &str, path: &Path) -> Result<(), String> {
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
    let response = client
        .post(format!("{base_url}/api/v2/torrents/add"))
        .multipart(multipart::Form::new().part("torrents", file_part))
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
