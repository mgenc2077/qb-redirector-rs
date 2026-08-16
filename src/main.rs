mod config;
mod kdialog;
mod qbt;

use std::path::Path;
use std::process::ExitCode;

use reqwest::blocking::Client;

use config::Instance;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            kdialog::error_dialog(&message);
            ExitCode::FAILURE
        }
    }
}

/// One selectable radiolist entry: an instance plus an optional category.
struct Target {
    instance: usize,
    category: Option<String>,
    label: String,
}

/// Builds the combined chooser entries: every instance's category entries
/// first, then all "no category" entries at the bottom. Preselected is the
/// default instance's `default_category` entry when configured (falling back
/// to that instance's first category entry, then its "no category" entry).
fn build_targets(instances: &[Instance], categories: &[Vec<String>]) -> (Vec<Target>, usize) {
    let mut targets = Vec::new();
    let default_instance = instances.iter().position(|i| i.default).unwrap_or(0);
    let mut default_index = None;
    let mut first_default_category = None;
    for (index, instance) in instances.iter().enumerate() {
        for category in &categories[index] {
            if index == default_instance {
                first_default_category.get_or_insert(targets.len());
                if Some(category) == instance.default_category.as_ref() {
                    default_index = Some(targets.len());
                }
            }
            targets.push(Target {
                instance: index,
                category: Some(category.clone()),
                label: format!("{} — {}", instance.name, category),
            });
        }
    }
    for (index, instance) in instances.iter().enumerate() {
        if index == default_instance && default_index.is_none() && first_default_category.is_none()
        {
            default_index = Some(targets.len());
        }
        targets.push(Target {
            instance: index,
            category: None,
            label: format!("{} — no category", instance.name),
        });
    }
    let default_index = default_index.or(first_default_category).unwrap_or(0);
    (targets, default_index)
}

fn run() -> Result<(), String> {
    let target = std::env::args()
        .nth(1)
        .filter(|t| !t.is_empty())
        .ok_or("No torrent file or magnet link provided.")?;

    let config = config::load_or_create()?;

    // Log into every instance up front and fetch its categories. An instance
    // that is unreachable still shows up in the chooser, just without
    // categories — the add step will surface its error if it gets picked.
    let mut clients: Vec<Client> = Vec::new();
    let mut categories: Vec<Vec<String>> = Vec::new();
    for instance in &config.instances {
        let client = qbt::client()?;
        let base_url = instance.url.trim_end_matches('/');
        let session = match (&instance.username, &instance.password) {
            (Some(username), Some(password)) => qbt::login(&client, base_url, username, password),
            _ => Ok(()),
        };
        categories.push(match session {
            Ok(()) => qbt::fetch_categories(&client, base_url).unwrap_or_default(),
            Err(_) => Vec::new(),
        });
        clients.push(client);
    }

    let (targets, default_index) = build_targets(&config.instances, &categories);
    let labels: Vec<String> = targets.iter().map(|t| t.label.clone()).collect();
    let Some(choice) = kdialog::radiolist(&labels, default_index)? else {
        return Ok(()); // user cancelled
    };
    let picked = &targets[choice];
    let instance = &config.instances[picked.instance];
    let client = &clients[picked.instance];
    let base_url = instance.url.trim_end_matches('/');
    let category = picked.category.as_deref();

    // Without a category there is no save path to inherit, so ask for one,
    // prefilled with the instance's default (a path on the server).
    let mut save_path = None;
    if category.is_none() {
        let default_path = qbt::fetch_default_save_path(client, base_url).unwrap_or_default();
        let Some(entered) = kdialog::input_box("Download location:", &default_path)? else {
            return Ok(()); // user cancelled
        };
        if !entered.is_empty() {
            save_path = Some(entered);
        }
    }

    if qbt::is_magnet(&target) {
        qbt::add_magnet(client, base_url, &target, category, save_path.as_deref())?;
    } else {
        qbt::add_torrent_file(
            client,
            base_url,
            Path::new(&target),
            category,
            save_path.as_deref(),
        )?;
    }

    kdialog::success_popup(&format!("Successfully sent to {}.", picked.label));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(name: &str, default: bool, default_category: Option<&str>) -> Instance {
        Instance {
            name: name.into(),
            url: "https://example.com".into(),
            default,
            default_category: default_category.map(String::from),
            username: None,
            password: None,
        }
    }

    #[test]
    fn no_category_entries_sink_to_bottom() {
        let instances = [
            instance("Private", false, None),
            instance("Public", true, Some("Anime")),
        ];
        let categories = [
            vec!["Movies".to_string()],
            vec!["Anime".to_string(), "Light-Novels".to_string()],
        ];
        let (targets, default_index) = build_targets(&instances, &categories);
        let labels: Vec<&str> = targets.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Private — Movies",
                "Public — Anime",
                "Public — Light-Novels",
                "Private — no category",
                "Public — no category",
            ]
        );
        assert_eq!(targets[default_index].label, "Public — Anime");
        assert_eq!(targets[3].category, None);
        assert_eq!(targets[3].instance, 0);
        assert_eq!(targets[4].instance, 1);
    }

    #[test]
    fn preselection_falls_back_to_first_category_then_no_category() {
        // default_category not in the fetched list -> first category entry
        let instances = [instance("A", true, Some("Gone"))];
        let categories = [vec!["X".to_string()]];
        let (targets, default_index) = build_targets(&instances, &categories);
        assert_eq!(targets[default_index].label, "A — X");

        // no categories at all -> the instance's "no category" entry
        let categories = [vec![]];
        let (targets, default_index) = build_targets(&instances, &categories);
        assert_eq!(targets[default_index].label, "A — no category");
    }
}
