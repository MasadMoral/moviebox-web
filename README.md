<div align="center">

# MovieBox Web

**A clean, mobile-first web player for [MovieBox-TUI](https://github.com/mesamirh/MovieBox-Tui).**
Search, browse, stream, resume, subtitles and downloads, from any browser.

![Rust](https://img.shields.io/badge/Rust-1.90%2B-orange?logo=rust)
![Platforms](https://img.shields.io/badge/Android%20%7C%20Linux%20%7C%20Windows-supported-blue)
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
| **Playback tools** | Hold the video for 2x speed, fullscreen with controls, open in an external player, download to storage |
| **Hosting ready** | Optional password and configurable address |

## Quick start

You need [Rust](https://rustup.rs) 1.90 or newer and Git.

```bash
git clone https://github.com/YOUR-GITHUB-USERNAME/moviebox-web.git
cd moviebox-web
cargo build --release
./target/release/moviebox-web
```

Open **http://127.0.0.1:8787** in your browser.

> The first build takes a while because it compiles all dependencies
> (about 2-5 minutes on a PC, 10-30 minutes on a phone). Later builds are fast.

---

## Installation

### Android (Termux)

Install Termux from [F-Droid](https://f-droid.org/packages/com.termux/). Do not use the Play Store version.

```bash
pkg update -y && pkg install rust git -y
git clone https://github.com/YOUR-GITHUB-USERNAME/moviebox-web.git ~/moviebox-web
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
git clone https://github.com/YOUR-GITHUB-USERNAME/moviebox-web.git
cd moviebox-web
cargo build --release
./target/release/moviebox-web
```

On desktop, **External player** copies the stream link. In VLC use *Media > Open Network Stream* and paste it. For mpv: `mpv "<link>"`.

### Windows

1. Install the build tools: [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select **Desktop development with C++**.
2. Install Rust from [rustup.rs](https://rustup.rs), or in PowerShell: `winget install Rustlang.Rustup`.
3. Install [Git](https://git-scm.com/download/win).
4. Open a new PowerShell window:

```powershell
git clone https://github.com/YOUR-GITHUB-USERNAME/moviebox-web.git
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

```bash
# Linux / macOS / Termux
MBW_ADDR=0.0.0.0:8787 MBW_PASSWORD=change-me ./target/release/moviebox-web
```
```powershell
# Windows PowerShell
$env:MBW_ADDR="0.0.0.0:8787"; $env:MBW_PASSWORD="change-me"; .\target\release\moviebox-web.exe
```

The page is read from `static/index.html` at runtime when that folder is next to where you run the binary. UI edits only need a browser refresh, not a rebuild.

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
| `No stream sources available` | MovieBox changed its API. Update with `cargo update -p moviebox-tui && cargo build --release`, or switch provider |
| Some episodes play only the upgrade-notice clip | Upstream issue. Try another quality or provider |
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
