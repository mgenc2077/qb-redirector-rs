# qb-redirector

A small torrent router for machines that talk to more than one qBittorrent
instance (e.g. one for private trackers, one for public). Opening a `.torrent`
file or clicking a `magnet:` link pops a kdialog chooser for the target
instance and category, offers per-file selection, and submits the torrent to
that instance's Web API. A folder can also be opened for batch imports.

## Installation from releases

Grab the package for your distro from the
[latest release](../../releases/latest) and install it:

```bash
# Arch / CachyOS / Manjaro
sudo pacman -U qb-redirector-*.pkg.tar.zst

# Debian / Ubuntu
sudo apt install ./qb-redirector_*_amd64.deb

# Fedora / openSUSE
sudo rpm -i qb-redirector-*.rpm
```

The package installs `/usr/bin/qb-redirector-rs` and a desktop entry that
registers as the handler for `.torrent` files, `magnet:` links, and folders
(right-click → Open With). `kdialog` is the only runtime dependency.

To build from source instead: `cargo build --release`, copy
`target/release/qb-redirector-rs` to `~/.local/bin/` and
`qb-redirector.desktop` to `~/.local/share/applications/`.

> If you previously copied the binary to `~/.local/bin`, remove it before
> installing a package — `~/.local/bin` precedes `/usr/bin` on `PATH`, so the
> stale copy would keep running.

## Configuration

The config lives at `~/.config/qb-redirector/config.toml` and is generated
with defaults on first run. Each `[[instances]]` block accepts:

| Key                | Required | Meaning                                                                     |
| ------------------ | -------- | --------------------------------------------------------------------------- |
| `name`             | yes      | Label shown in the chooser and used by `instance` in import configs.         |
| `url`              | yes      | Base URL of the qBittorrent Web UI (e.g. `https://qb.example.ts.net`).       |
| `default`          | no       | `true` marks the instance whose entry is preselected in the chooser.         |
| `default_category` | no       | Category preselected in the chooser (falls back to the first category).      |
| `username`         | no       | Web UI login; needed unless the API is reachable without authentication.     |
| `password`         | no       | Web UI password; both must be set for login to happen.                       |

Example:

```toml
[[instances]]
name = "Private Tracker"
url = "https://qb-private.example.ts.net"
username = "admin"
password = "secret"

[[instances]]
name = "Public Tracker"
url = "https://qb-public.example.ts.net"
default = true
default_category = "Anime"
username = "admin"
password = "secret"
```

## Usage

Single torrents (`.torrent` file or magnet link):

1. A radiolist shows every instance–category combination ("no category"
   entries last). Picking a category sends the torrent to that category's save
   path; picking "no category" asks for a download path (prefilled with the
   instance's default).
2. The torrent is added stopped and a checklist of its files appears — untick
   what you don't want (skipped files are set to "do not download"). Magnets
   briefly start to fetch metadata first. Cancelling the checklist removes the
   torrent again.
3. The torrent starts. Single-file torrents skip the checklist.

## Batch import (open a folder)

Opening a **folder** with the redirector adds every `.torrent` directly inside
it in one go — started immediately, no per-file dialogs. The folder must
contain an `import-config.json`:

```json
{
  "category": "Anime",
  "downloadPath": "/downloads/Jellyfin/Anime",
  "instance": "Public Tracker"
}
```

All keys are optional:

| Key            | Meaning                                                                                        |
| -------------- | ---------------------------------------------------------------------------------------------- |
| `category`     | Category assigned to every added torrent.                                                      |
| `downloadPath` | Base path on the **server**; torrents download into `downloadPath` + `/` + the folder's name.  |
| `instance`     | Instance `name` from the main config; skips the instance dialog. Without it a chooser appears, preselecting the instance that has `category`. |

With the example above, torrents from a folder named `My Show S01` land in
`/downloads/Jellyfin/Anime/My Show S01/`. When `downloadPath` is set, the
category is assigned as a label only — the explicit path wins (no automatic
torrent management). Failures are collected and reported per file; the rest
are still added.

## Releases

Tagging `vX.Y.Z` (or running the workflow manually) builds deb, rpm, and arch
packages via [nFPM](https://nfpm.goreleaser.com/) from `packaging/nfpm.yaml`
and attaches them to a GitHub release. Locally:

```bash
cargo build --release
PKG_VERSION=0.0.0 nfpm pkg --packager archlinux --config packaging/nfpm.yaml --target dist/
```

## License

[GPL-3.0](LICENSE)
