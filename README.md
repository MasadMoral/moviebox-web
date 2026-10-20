# MovieBox Web

A mobile-friendly web UI for MovieBox-TUI, built to run on Android (Termux).
Search, details, season/episode picker, autoplay, resume, subtitles,
external player, and downloads.

## Credits
- [MovieBox-TUI](https://github.com/mesamirh/MovieBox-Tui) by @mesamirh:
  the provider and API library this project is built on (MIT / Apache-2.0).
- Stream recovery fix from PR #208 by @skumarbsft.
- Live TV playlists by [iptv-org](https://github.com/iptv-org/iptv) (via MovieBox-TUI).

## Run (Termux)
    pkg install rust git -y
    cargo build --release
    ./target/release/moviebox-web
Then open http://127.0.0.1:8787 in Chrome.

## Disclaimer
Third-party client. It does not host or store any media and only
resolves links from upstream providers. Personal use only.
