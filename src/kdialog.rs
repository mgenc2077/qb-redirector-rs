use std::process::Command;

const TITLE: &str = "qBittorrent Router";

fn radiolist_args(labels: &[String], default_index: usize) -> Vec<String> {
    let mut args = vec![
        "--radiolist".to_string(),
        "Where do you want to send this torrent?".to_string(),
    ];
    for (index, label) in labels.iter().enumerate() {
        args.push(index.to_string());
        args.push(label.clone());
        args.push(if index == default_index { "on" } else { "off" }.to_string());
    }
    args.push("--title".to_string());
    args.push(TITLE.to_string());
    args
}

/// Shows a radiolist chooser. Returns the selected index, or `None` if the
/// user cancelled.
pub fn radiolist(labels: &[String], default_index: usize) -> Result<Option<usize>, String> {
    let output = Command::new("kdialog")
        .args(radiolist_args(labels, default_index))
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
    if index >= labels.len() {
        return Err(format!("Unexpected kdialog choice: {index}"));
    }
    Ok(Some(index))
}

/// Shows a checklist with every entry preselected. Returns the indices of
/// the entries left checked, or `None` if the user cancelled.
pub fn checklist(question: &str, labels: &[String]) -> Result<Option<Vec<usize>>, String> {
    let mut args = vec!["--checklist".to_string(), question.to_string()];
    for (index, label) in labels.iter().enumerate() {
        args.push(index.to_string());
        args.push(label.clone());
        args.push("on".to_string());
    }
    args.push("--title".to_string());
    args.push(TITLE.to_string());
    let output = Command::new("kdialog")
        .args(args)
        .output()
        .map_err(|e| format!("Could not run kdialog: {e}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    // stdout holds the checked tags as quoted, space-separated numbers,
    // e.g. `"0" "2"`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut selected = Vec::new();
    for token in stdout.split_whitespace() {
        let index: usize = token
            .trim_matches('"')
            .parse()
            .map_err(|_| format!("Unexpected kdialog output: {token:?}"))?;
        selected.push(index);
    }
    Ok(Some(selected))
}

/// Shows a text input box. Returns the entered value, or `None` if the user
/// cancelled.
pub fn input_box(prompt: &str, initial: &str) -> Result<Option<String>, String> {
    let output = Command::new("kdialog")
        .args(["--inputbox", prompt, initial, "--title", TITLE])
        .output()
        .map_err(|e| format!("Could not run kdialog: {e}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
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

    #[test]
    fn radiolist_marks_default_entry() {
        let labels = ["A".to_string(), "B".to_string()];
        let args = radiolist_args(&labels, 1);
        assert_eq!(
            args,
            [
                "--radiolist",
                "Where do you want to send this torrent?",
                "0",
                "A",
                "off",
                "1",
                "B",
                "on",
                "--title",
                TITLE,
            ]
        );
    }
}
