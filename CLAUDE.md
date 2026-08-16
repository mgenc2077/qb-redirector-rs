# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Rust GUI-less desktop tool that routes torrents (`.torrent` files, `magnet:` links, or whole import folders) to one of several qBittorrent instances over their Web API, prompting the user through `kdialog` subprocesses. It is launched by the desktop environment via `qb-redirector.desktop` (`%U`), not from a terminal — all user-visible errors must go through `kdialog::error_dialog`, never stderr. README.md documents the user-facing behavior; keep the two in sync.

## Commands

```
cargo build
cargo test
cargo run -- <torrent-file | magnet-url | import-folder>
cargo build --release
PKG_VERSION=0.0.0 nfpm pkg --packager archlinux --config packaging/nfpm.yaml --target dist/
```

Running without arguments or with dialogs headless will block or pop dialogs on the user's desktop — verify API behavior with `curl` against the instances instead of driving the GUI.

## Architecture

`main.rs` owns the flow; the other modules are thin and stateless:

- `config.rs` — `~/.config/qb-redirector/config.toml` (`[[instances]]`: name, url, default, default_category, username, password), auto-generated on first run.
- `kdialog.rs` — `std::process::Command` wrappers: `radiolist` (chooser), `checklist` (file selection), `input_box` (save path), `success_popup`, `error_dialog`. kdialog cancel = non-zero exit or empty stdout → treated as user cancel (silent exit 0).
- `qbt.rs` — blocking reqwest client (rustls, cookie store, 15 s timeout) for the qBittorrent Web API; `AddOptions` centralizes the add-form logic.
- `import.rs` — folder batch mode: `import-config.json` (`category`, `downloadPath`, `instance`), `.torrent` scan, save-path formula `downloadPath + "/" + folder name`.

Flow for single torrents: connect + login to every instance and fetch categories up front → one radiolist of "Instance — Category" entries (no-category entries last; preselection from `default`/`default_category`) → no-category picks prompt for a server-side download path → add **stopped** → find the new torrent by diffing the hash list → file checklist → unchecked files get priority 0 via `filePrio` → start. Folder mode adds everything started immediately, no checklist.

## qBittorrent API gotchas (learned against v5.2.3)

- **Login** (`/api/v2/auth/login`) returns **204 with an empty body** on success in 5.x; 4.x returned 200 `"Ok."`/`"Fails."`. Treat any 2xx with empty or `Ok.` body as success.
- **5.x renamed pause/resume to `stop`/`start`** (`/api/v2/torrents/stop|start`); `torrent_action` tries the new name and falls back on 404. Similarly, add takes `stopped=true` in 5.x, `paused=true` in 4.x — both are sent, the unknown one is ignored.
- **`autoTMM` decides whether the category's save path applies.** Category without explicit path → send `autoTMM=true`, or the torrent lands in the global default dir. Explicit `savepath` → must NOT send autoTMM, or it overrides the path. `AddOptions::auto_tmm()` encodes this; don't bypass it.
- A magnet's file list is empty until metadata arrives, and metadata only downloads while the torrent is running — hence the brief start/stop dance before the checklist. Only metadata can transfer in that window, so nothing unwanted is downloaded.
- Success for `/torrents/add` is any 2xx (204 observed through proxies), not literally 200.
- Localhost auth bypass does not extend to remote/proxied access — instances generally need `username`/`password` in the config; qBittorrent temporarily bans an IP after a few failed logins.

## Releasing

`.github/workflows/release.yaml` triggers on `v*.*.*` tags (or manual dispatch): validates the tag against a whitelist regex **before any use** (ref names can contain shell/sed metacharacters — keep the env-var + validation pattern), runs tests, builds release, then packages deb + rpm + archlinux from the single `packaging/nfpm.yaml` (version injected via nfpm's `${PKG_VERSION}` env expansion, no sed). Arch `pkgver` forbids hyphens, so the tag regex rejects hyphenated suffixes.
