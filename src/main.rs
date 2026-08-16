mod config;
mod import;
mod kdialog;
mod qbt;

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

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

    // A directory means "open folder with" batch mode: add every .torrent
    // inside it according to the folder's import-config.json.
    if Path::new(&target).is_dir() {
        return run_folder_import(Path::new(&target), &config, &clients, &categories);
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

    // The torrent is added stopped; we find it by diffing the hash list,
    // read its files, let the user deselect some, then start it.
    let options = qbt::AddOptions {
        category: picked.category.clone(),
        save_path,
        stopped: true,
    };
    let known_hashes = qbt::list_hashes(client, base_url).unwrap_or_default();
    if qbt::is_magnet(&target) {
        qbt::add_magnet(client, base_url, &target, &options)?;
    } else {
        qbt::add_torrent_file(client, base_url, Path::new(&target), &options)?;
    }
    let hash = poll(Duration::from_secs(10), || {
        qbt::list_hashes(client, base_url)
            .ok()?
            .difference(&known_hashes)
            .next()
            .cloned()
    })
    .ok_or("Torrent was accepted but never appeared in the torrent list.")?;

    // A .torrent file's list is available immediately; a magnet has no files
    // until its metadata arrives, which needs the torrent running. Only
    // metadata can transfer before the file list exists, so nothing unwanted
    // is downloaded while we wait.
    let mut files = poll(Duration::from_secs(3), || {
        non_empty(qbt::fetch_files(client, base_url, &hash).unwrap_or_default())
    });
    if files.is_none() {
        kdialog::success_popup("Fetching torrent metadata…");
        qbt::start_torrent(client, base_url, &hash)?;
        files = poll(Duration::from_secs(60), || {
            non_empty(qbt::fetch_files(client, base_url, &hash).unwrap_or_default())
        });
        let _ = qbt::stop_torrent(client, base_url, &hash);
    }
    let Some(files) = files else {
        // Metadata never arrived; start it with everything selected rather
        // than dropping the torrent.
        qbt::start_torrent(client, base_url, &hash)?;
        kdialog::success_popup(&format!(
            "Sent to {} (metadata still pending, all files selected).",
            picked.label
        ));
        return Ok(());
    };

    if files.len() > 1 {
        let labels: Vec<String> = files
            .iter()
            .map(|f| format!("{} ({})", f.name, human_size(f.size)))
            .collect();
        let Some(checked) = kdialog::checklist("Select the files to download:", &labels)? else {
            let _ = qbt::delete_torrent(client, base_url, &hash);
            return Ok(()); // user cancelled -> remove the stopped torrent
        };
        let skipped: Vec<i64> = files
            .iter()
            .enumerate()
            .filter(|(index, _)| !checked.contains(index))
            .map(|(_, f)| f.id)
            .collect();
        if skipped.len() == files.len() {
            let _ = qbt::delete_torrent(client, base_url, &hash);
            return Ok(()); // nothing selected -> same as cancelling
        }
        if !skipped.is_empty() {
            qbt::skip_files(client, base_url, &hash, &skipped)?;
        }
    }

    qbt::start_torrent(client, base_url, &hash)?;
    kdialog::success_popup(&format!("Successfully sent to {}.", picked.label));
    Ok(())
}

/// Adds every .torrent in the folder, started immediately, with the category
/// from import-config.json and the save path `<downloadPath>/<folder name>`.
fn run_folder_import(
    folder: &Path,
    config: &config::Config,
    clients: &[Client],
    categories: &[Vec<String>],
) -> Result<(), String> {
    let import = import::load(folder)?;
    let torrents = import::torrent_files(folder)?;
    if torrents.is_empty() {
        return Err(format!("No .torrent files in {}.", folder.display()));
    }
    let folder_name = folder
        .canonicalize()
        .map_err(|e| format!("Could not resolve {}: {e}", folder.display()))?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("Could not determine the folder's name.")?;

    let instance_index = if let Some(name) = &import.instance {
        config
            .instances
            .iter()
            .position(|i| &i.name == name)
            .ok_or_else(|| format!("Unknown instance {name:?} in import-config.json."))?
    } else {
        // Preselect the instance that actually has the requested category.
        let preselected = import
            .category
            .as_ref()
            .and_then(|c| categories.iter().position(|list| list.contains(c)))
            .or_else(|| config.instances.iter().position(|i| i.default))
            .unwrap_or(0);
        let labels: Vec<String> = config.instances.iter().map(|i| i.name.clone()).collect();
        match kdialog::radiolist(&labels, preselected)? {
            Some(index) => index,
            None => return Ok(()), // user cancelled
        }
    };
    let instance = &config.instances[instance_index];
    let client = &clients[instance_index];
    let base_url = instance.url.trim_end_matches('/');

    let options = qbt::AddOptions {
        category: import.category.clone(),
        save_path: import
            .download_path
            .as_deref()
            .map(|path| import::save_path_for(path, &folder_name)),
        stopped: false,
    };
    let mut failures = Vec::new();
    for torrent in &torrents {
        if let Err(error) = qbt::add_torrent_file(client, base_url, torrent, &options) {
            let name = torrent.file_name().unwrap_or_default().to_string_lossy().into_owned();
            failures.push(format!("{name}: {error}"));
        }
    }
    if failures.is_empty() {
        kdialog::success_popup(&format!(
            "Sent {} torrents to {}.",
            torrents.len(),
            instance.name
        ));
        Ok(())
    } else {
        Err(format!(
            "{} of {} torrents failed:\n{}",
            failures.len(),
            torrents.len(),
            failures.join("\n")
        ))
    }
}

/// Calls `check` every half second until it yields a value or the timeout
/// elapses.
fn poll<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(value) = check() {
            return Some(value);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn non_empty<T>(items: Vec<T>) -> Option<Vec<T>> {
    if items.is_empty() { None } else { Some(items) }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
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
    fn human_readable_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(214_700_000), "204.8 MiB");
        assert_eq!(human_size(4_284_481_536), "4.0 GiB");
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
