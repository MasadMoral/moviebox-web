<div align="center">

# MovieBox Web

**A clean, mobile-first web player for [MovieBox-TUI](https://github.com/mesamirh/MovieBox-Tui).**
Search, browse, stream, resume, subtitles and downloads, from any browser.

![Rust](https://img.shields.io/badge/Rust-1.90%2B-orange?logo=rust)
![Platforms](https://img.shields.io/badge/Android%20%7C%20Linux%20%7C%20macOS%20%7C%20Windows-supported-blue)
![License](https://img.shields.io/badge/license-MIT-green)

</div>

---

## Features

| | |
|---|---|
| **Player-first UI** | Video on top, autoplay, quality picker, Next / Previous episode |
| **Resume** | Remembers the episode and the exact second, with a Library of recent watches |
| **Subtitles** | Language list, load your own `.srt` / `.vtt` / `.ass`, size, color, background, position and sync |
| **Episodes** | "All" panel with a season dropdown and episode grid |
| **Providers** | MovieBox, 4KHDHub, Dramachi, CircleFTP and DhakaFlix (BDIX), switchable from the settings icon |
| **Search** | Live suggestions from the selected provider, with posters |
| **Playback tools** | Hold the video for 2x speed, swipe left/right for brightness/volume, fullscreen with controls (both landscape sides), Fill / Fit / Stretch and 16:9 / 4:3 / 21:9 screen modes, Picture-in-Picture, Bluetooth/headset controls, open in an external player, download with progress notification or copy the download link |
| **Remote rules** | MovieBox field names, notice URLs and media extensions can be updated from `config/provider.json` without rebuilding |
| **Hosting ready** | Optional password and configurable address |

## Quick start

You need [Rust](https://rustup.rs) 1.90 or newer and Git.

```bash
git clone https://github.com/MasadMoral/moviebox-web.git
cd moviebox-web
cargo build --release
./target/release/moviebox-web
```

Open **http://127.0.0.1:8787** in your browser.

> The first build takes a while because it compiles all dependencies
> (about 2-5 minutes on a PC, 10-30 minutes on a phone). Later builds are fast.

### Prebuilt downloads

Check the [Releases](https://github.com/MasadMoral/moviebox-web/releases) page for ready-made builds:

- **Windows / Linux / macOS:** extract the archive, run `moviebox-web` (or `moviebox-web.exe`), then open http://127.0.0.1:8787. Linux builds are static x86_64; macOS builds are universal (Apple Silicon + Intel) and unsigned.
- **Android:** install `moviebox-web-android-arm64.apk` (arm64, Android 7+, Picture-in-Picture needs 8+, debug-signed for sideloading).

The desktop app is a local server, not a GUI window. Keep the terminal open while watching.

---

## Installation

### Android (Termux)

Install Termux from [F-Droid](https://f-droid.org/packages/com.termux/). Do not use the Play Store version.

```bash
pkg update -y && pkg install rust git -y
git clone https://github.com/MasadMoral/moviebox-web.git ~/moviebox-web
cd ~/moviebox-web
cargo build --release
termux-wake-lock
./target/release/moviebox-web
```

Open `http://127.0.0.1:8787` in Chrome, then use **Add to Home screen**.

- Build in Termux's home folder (`~`). Shared storage such as `/storage/emulated/0/Download` blocks running binaries.
- If the build runs out of memory, use `cargo build --release -j2`.
- **External player** opens the Android app chooser (VLC, MX Player, ...).

### Linux

**Debian / Ubuntu**
```bash
sudo apt update && sudo apt install -y build-essential git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

**Fedora**
```bash
sudo dnf install -y gcc git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

**Arch**
```bash
sudo pacman -S --needed base-devel git rustup && rustup default stable
```

Then build and run:
```bash
git clone https://github.com/MasadMoral/moviebox-web.git
cd moviebox-web
cargo build --release
./target/release/moviebox-web
```

On desktop, **External player** copies the stream link. In VLC use *Media > Open Network Stream* and paste it. For mpv: `mpv "<link>"`.

### macOS

1. Install the Apple command line tools (compiler and Git). Skip this if you already have them:
```bash
xcode-select --install
```
2. Install Rust (1.90 or newer):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```
Or with Homebrew: `brew install rust git`

3. Build and run:
```bash
git clone https://github.com/MasadMoral/moviebox-web.git
cd moviebox-web
cargo build --release
./target/release/moviebox-web
```

Open **http://127.0.0.1:8787** in Safari or Chrome. Works on both Apple Silicon and Intel Macs.

**External player** copies the stream link. In VLC use *File > Open Network*, in IINA use *File > Open URL*, or for mpv run `mpv "<link>"`.

### Windows

1. Install the build tools: [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select **Desktop development with C++**.
2. Install Rust from [rustup.rs](https://rustup.rs), or in PowerShell: `winget install Rustlang.Rustup`.
3. Install [Git](https://git-scm.com/download/win).
4. Open a new PowerShell window:

```powershell
git clone https://github.com/MasadMoral/moviebox-web.git
cd moviebox-web
cargo build --release
.\target\release\moviebox-web.exe
```

Open **http://127.0.0.1:8787**. **External player** copies the stream link, so paste it into VLC under *Media > Open Network Stream*.

---

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `MBW_ADDR` | `127.0.0.1:8787` | Address and port to listen on |
| `MBW_PASSWORD` | *(none)* | Enables HTTP basic auth. Use any username and this password |
| `MBW_CONFIG_URL` | this repo's `config/provider.json` on `main` | Remote playback rules URL. Set to `off` to disable remote updates |
| `MBW_CONFIG_CACHE` | `$XDG_CACHE_HOME/moviebox-web/provider.json` (or `~/.cache/...`) | Where the last good rules are cached |

```bash
# Linux / macOS / Termux
MBW_ADDR=0.0.0.0:8787 MBW_PASSWORD=change-me ./target/release/moviebox-web
```
```powershell
# Windows PowerShell
$env:MBW_ADDR="0.0.0.0:8787"; $env:MBW_PASSWORD="change-me"; .\target\release\moviebox-web.exe
```

The page is read from `static/index.html` at runtime when that folder is next to where you run the binary. UI edits only need a browser refresh, not a rebuild.

### Remote playback rules

MovieBox occasionally renames fields or changes the "update the app" notice URL. The parser reads [`config/provider.json`](config/provider.json), so you can fix those changes without shipping a new app:

1. Edit `config/provider.json` and push it to `main`.
2. The app fetches it at startup and every 5 minutes in the background, so normal requests never wait for it. After a failed playback attempt it also tries a fresh copy, waiting at most 1.8 seconds.
3. The last good copy is cached on disk. If the network or the JSON is invalid, the app keeps using that copy; if there is no cache, the copy bundled into the binary is used.
4. Check the active version/source from the provider menu (sliders icon) or `GET /api/config`. Force a refresh with `POST /api/config/refresh`.

The rules cover the detector paths/field names, dub fallback, notice patterns and accepted media extensions. `detectors_paths` and `dubs_paths` use dotted object paths; the other `*_field` values are flat keys. They **cannot** fix a new signing scheme, DRM/attestation, a new login flow, or a changed endpoint that needs different code. Those still need an app update.

### Series playback

MovieBox's native `play-info` and `resource` APIs currently return only the 23-second upgrade clip for series episodes, and `resource-position` comes back with an empty list. The app therefore falls back to MovieBox's **web player endpoint** (`web_play_path` on the hosts in `web_play_domains`) using the visitor token and the subject's `detailPath`. If the localized subject has no streams, it retries the original-audio/dub subjects from `dubs`. Set `"web_play": false` in `config/provider.json` to disable this fallback.

Both the endpoint and the host list are remote-configurable, so a domain change can be fixed without rebuilding. The web player returns signed MP4 links; playback and downloads still go through the normal `/proxy`.

### Automatic API-change recovery

Besides the manual rules file, the parser carries built-in **fallback field names** for the MovieBox payload shapes we have seen (`resourceDetectors[].<entries key>[]`, entry link keys, quality/size/codec/title/uploader/source/season/episode keys, …). When the configured names produce no playable releases, the app automatically probes those alternatives before giving up, so a simple rename/reshuffle upstream no longer means "no playback".

- Primary configured names always win; fallbacks are only consulted when the primary key is missing/empty.
- The parser now walks detector paths (`cfg.detector_paths()`) and entry keys (`cfg.entries_keys()`) with fallback support end-to-end.
- New alternative names can also be added via `config/provider.json` (`*_fallbacks`) without a rebuild; the built-in defaults cover the known renames.
- A failed lookup that uses fallbacks is a strong signal that MovieBox changed something — check `GET /api/debug/moviebox?id=...` and, if the built-in fallbacks don't cover it, add the new key to `config/provider.json` (or propose it) so the app keeps playing.

### Inspecting a MovieBox response

The fastest way is the built-in debug endpoint. On the page, open the provider menu (sliders icon) and choose **Inspect MovieBox response**; it opens the same endpoint for the current title/episode. It uses the same session/signing path as playback and returns the raw details, play-info and resource JSON plus the releases the app parsed from the detector and the web player (`parsed_releases`, `web_play_releases`). For episodes it also probes every resolution through `resource-position` (`resource_probes`):

```text
http://127.0.0.1:8787/api/debug/moviebox?id=SUBJECT_ID
http://127.0.0.1:8787/api/debug/moviebox?id=SUBJECT_ID&season=1&episode=1
```

Get a subject ID from `http://127.0.0.1:8787/api/search?q=title` (the `id.value` field) or from the browser Network tab. If `MBW_PASSWORD` is set, use the same password for these URLs. The response is local-only and is protected by the same auth.

---

## Hosting guide

> **Read first.**
> - This app proxies video through your server. Always set `MBW_PASSWORD` when it is reachable from outside your own device.
> - MovieBox may block datacenter and VPS IP addresses. If streams fail on a VPS but work at home, that is the cause.
> - BDIX providers (CircleFTP, DhakaFlix) only work from a Bangladeshi ISP.
> - Serverless platforms such as Vercel, Netlify and GitHub Pages **cannot** run this app. It needs a long-running server and long streaming responses.

### Option 1: Your own device or home network (recommended)

Run it on a phone, PC or Raspberry Pi and open it from other devices on the same Wi-Fi:

```bash
MBW_ADDR=0.0.0.0:8787 MBW_PASSWORD=change-me ./target/release/moviebox-web
# then open http://<device-ip>:8787 from another device
```

For access away from home without opening router ports, use a private network such as [Tailscale](https://tailscale.com). Install it on both devices and open `http://<tailscale-ip>:8787`.

### Option 2: VPS with systemd and HTTPS

1. Build the binary on the server and copy `target/release/moviebox-web` and the `static/` folder to `/opt/moviebox-web/`.
2. Create `/etc/systemd/system/moviebox-web.service`:

```ini
[Unit]
Description=MovieBox Web
After=network-online.target

[Service]
WorkingDirectory=/opt/moviebox-web
ExecStart=/opt/moviebox-web/moviebox-web
Environment=MBW_ADDR=127.0.0.1:8787
Environment=MBW_PASSWORD=change-me
Restart=on-failure
User=www-data

[Install]
WantedBy=multi-user.target
```
```bash
sudo systemctl daemon-reload && sudo systemctl enable --now moviebox-web
```

3. Put [Caddy](https://caddyserver.com) in front for automatic HTTPS. In `/etc/caddy/Caddyfile`:

```
movies.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

Point your domain's DNS to the server, then `sudo systemctl reload caddy`.

### Option 3: Docker

```bash
docker build -t moviebox-web .
docker run -d --name moviebox-web --restart unless-stopped \
  -p 8787:8787 -e MBW_PASSWORD=change-me moviebox-web
```

The included `Dockerfile` listens on `0.0.0.0:8787` inside the container.

---

## Troubleshooting

| Problem | Fix |
|---|---|
| Build: `Permission denied` on Android | Build in `~`, not in `/storage/emulated/0/...` |
| `No stream sources available` | Check `/api/config` and `/api/debug/moviebox?id=...`; if the fields changed, update `config/provider.json` and push it. If the endpoint/signing changed, update with `cargo update -p moviebox-tui && cargo build --release`, or switch provider |
| Series says `MovieBox has no direct link for this episode` | Check `/api/debug/moviebox?id=...&season=1&episode=1`: `web_play_releases` should list MP4s. If it does not, inspect `web_play_domains`/`web_play_path` in `config/provider.json` and the raw `detailPath`. If the web endpoint changed, update the rules; if it is gone, switch provider or update the app |
| Some episodes play only the upgrade-notice clip | Add the new notice URL/hash to `notice_patterns` in `config/provider.json`, or try another quality/provider |
| Video won't play in the browser (`.mkv`, `.mpd`) | Use **External player** |
| `Address already in use` | An old copy is running. Stop it (`pkill moviebox-web` on Linux or Termux) |

## Updating

```bash
git pull
cargo update -p moviebox-tui
cargo build --release
```

## Credits

- [MovieBox-TUI](https://github.com/mesamirh/MovieBox-Tui) by [@mesamirh](https://github.com/mesamirh), the provider and API library this project is built on (MIT / Apache-2.0).
- Stream recovery fix from PR #208 by [@skumarbsft](https://github.com/skumarbsft).
- Live TV playlists by [iptv-org](https://github.com/iptv-org/iptv), via MovieBox-TUI.

## Disclaimer

This is a third-party client. It does not host or store any media and only
resolves links from upstream providers. It is intended for personal use only.
You are responsible for complying with the laws and terms of service that apply to you.
