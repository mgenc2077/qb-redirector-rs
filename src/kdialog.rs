use std::process::Command;

use crate::config::Instance;

const TITLE: &str = "qBittorrent Router";

fn radiolist_args(instances: &[Instance]) -> Vec<String> {
    let mut args = vec![
        "--radiolist".to_string(),
        "Where do you want to send this torrent?".to_string(),
    ];
    // If no instance is marked default, preselect the first one so the
    // dialog always has an active choice.
    let default_index = instances.iter().position(|i| i.default).unwrap_or(0);
    for (index, instance) in instances.iter().enumerate() {
        args.push(index.to_string());
        args.push(instance.name.clone());
        args.push(if index == default_index { "on" } else { "off" }.to_string());
    }
    args.push("--title".to_string());
    args.push(TITLE.to_string());
    args
}

/// Shows the instance chooser. Returns `None` if the user cancelled.
pub fn choose_instance(instances: &[Instance]) -> Result<Option<usize>, String> {
    let output = Command::new("kdialog")
        .args(radiolist_args(instances))
        .output()
        .map_err(|e| format!("Could not run kdialog: {e}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let choice = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if choice.is_empty() {
        return Ok(None);
    }
    let index: usize = choice
        .parse()
        .map_err(|_| format!("Unexpected kdialog output: {choice:?}"))?;
    if index >= instances.len() {
        return Err(format!("Unexpected kdialog choice: {index}"));
    }
    Ok(Some(index))
}

pub fn success_popup(message: &str) {
    let _ = Command::new("kdialog")
        .args(["--passivepopup", message, "3", "--title", TITLE])
        .status();
}

pub fn error_dialog(message: &str) {
    let result = Command::new("kdialog")
        .args(["--error", message, "--title", TITLE])
        .status();
    if result.is_err() {
        eprintln!("{TITLE}: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(name: &str, default: bool) -> Instance {
        Instance {
            name: name.into(),
            url: "https://example.com".into(),
            default,
            username: None,
            password: None,
        }
    }

    #[test]
    fn radiolist_marks_default_instance() {
        let instances = [instance("Private", false), instance("Public", true)];
        let args = radiolist_args(&instances);
        assert_eq!(
            args,
            [
                "--radiolist",
                "Where do you want to send this torrent?",
                "0",
                "Private",
                "off",
                "1",
                "Public",
                "on",
                "--title",
                TITLE,
            ]
        );
    }

    #[test]
    fn radiolist_falls_back_to_first_instance() {
        let instances = [instance("A", false), instance("B", false)];
        let args = radiolist_args(&instances);
        assert_eq!(args[4], "on");
        assert_eq!(args[7], "off");
    }
}
