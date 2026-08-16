use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub instances: Vec<Instance>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Instance {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub default: bool,
    pub username: Option<String>,
    pub password: Option<String>,
}

const DEFAULT_CONFIG: &str = r#"[[instances]]
name = "Private Tracker"
url = "https://qb-private.tail5a66c8.ts.net"
default = true

[[instances]]
name = "Public Tracker"
url = "https://qb-public.tail5a66c8.ts.net"
# username = "admin"   # uncomment to enable auth
# password = "secret"
"#;

fn config_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir().ok_or("Could not determine the config directory.")?;
    Ok(dir.join("qb-redirector").join("config.toml"))
}

/// Reads the config file, creating it with the default contents if it does not exist.
pub fn load_or_create() -> Result<Config, String> {
    let path = config_path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
            }
            fs::write(&path, DEFAULT_CONFIG)
                .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
            DEFAULT_CONFIG.to_string()
        }
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    parse(&contents).map_err(|e| format!("Invalid config {}: {e}", path.display()))
}

fn parse(contents: &str) -> Result<Config, String> {
    let config: Config = toml::from_str(contents).map_err(|e| e.to_string())?;
    if config.instances.is_empty() {
        return Err("no instances defined".into());
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let config = parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(config.instances.len(), 2);
        assert!(config.instances[0].default);
        assert!(!config.instances[1].default);
        assert_eq!(config.instances[0].name, "Private Tracker");
        assert_eq!(config.instances[1].url, "https://qb-public.tail5a66c8.ts.net");
        assert_eq!(config.instances[0].username, None);
    }

    #[test]
    fn credentials_round_trip() {
        let config = Config {
            instances: vec![Instance {
                name: "Private".into(),
                url: "https://example.com".into(),
                default: true,
                username: Some("admin".into()),
                password: Some("secret".into()),
            }],
        };
        let toml_str = toml::to_string(&config).unwrap();
        let parsed = parse(&toml_str).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn empty_instances_rejected() {
        assert!(parse("instances = []").is_err());
    }
}
