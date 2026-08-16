mod config;
mod kdialog;
mod qbt;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            kdialog::error_dialog(&message);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let target = std::env::args()
        .nth(1)
        .filter(|t| !t.is_empty())
        .ok_or("No torrent file or magnet link provided.")?;

    let config = config::load_or_create()?;

    let Some(index) = kdialog::choose_instance(&config.instances)? else {
        return Ok(()); // user cancelled
    };
    let instance = &config.instances[index];
    let base_url = instance.url.trim_end_matches('/');

    let client = qbt::client()?;
    if let (Some(username), Some(password)) = (&instance.username, &instance.password) {
        qbt::login(&client, base_url, username, password)?;
    }

    if qbt::is_magnet(&target) {
        qbt::add_magnet(&client, base_url, &target)?;
    } else {
        qbt::add_torrent_file(&client, base_url, Path::new(&target))?;
    }

    kdialog::success_popup(&format!("Successfully sent to {}.", instance.name));
    Ok(())
}
