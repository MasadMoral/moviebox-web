# Project context: moviebox-web

## What it is
A Rust web server plus a single-page mobile-first web UI that acts as a browser front end for MovieBox-TUI (https://github.com/mesamirh/MovieBox-Tui, by mesamirh). The TUI repo is used as a LIBRARY for providers/APIs. I want to modify and extend this project. Repo: github.com/MasadMoral/moviebox-web

## Architecture
- Cargo crate `moviebox-web`: library (`src/lib.rs`, crate-type cdylib+rlib) and binary (`src/main.rs`, a thin wrapper).
- Dependency: `moviebox-tui` from git fork `skumarbsft/MovieBox-Tui`, branch `fix/moviebox-stream-recovery-205` (community fix for MovieBox returning an "upgrade notice" clip instead of real streams, PR #208). Edition 2024, Rust 1.90+.
- `src/lib.rs`: axum server. `pub async fn serve(addr, wait_init)`. Also a C-ABI entry `mbw_start(port: u16, data_dir: *const c_char) -> i32` used by the Android app via dart:ffi (starts the server on 127.0.0.1:port in a background thread).
- `static/index.html`: the whole UI (vanilla JS/CSS, inline SVG icons). Served from disk if `static/index.html` exists in the working directory, else the embedded copy (include_str!). UI-only edits need only a browser refresh.
- Env vars: `MBW_ADDR` (default 127.0.0.1:8787), `MBW_PASSWORD` (optional HTTP basic auth, any username).

## HTTP endpoints (all take `p=<provider>`: moviebox | fourkhdhub | dramachi | bdix_circleftp | bdix_dhakaflix)
- GET /api/home, /api/search, /api/suggest (poster+title+year rows), /api/details
- GET /api/streams?id&season&episode -> releases [{quality,language,size,url,h,rid,rel}]
  - `h` = base64url JSON of upstream headers; `rel` = base64 Release for 4KHDHub (resolved on demand)
  - MovieBox: extra per-resolution fallback fetch + resource-position recovery when only upgrade-notice clips come back
- POST /api/resolve {rel} -> resolves a 4KHDHub mirror to a real file URL
- GET /api/subs -> MovieBox subtitle list; the page fetches the file through /proxy
- GET /proxy?url&h[&dl=1&name=] -> Range-aware streaming proxy (adds upstream headers; dl=1 sets Content-Disposition for downloads)
- GET /api/providers

## UI features (static/index.html)
Header: search + live provider suggestions, Home, Library (watch history), provider switcher (settings icon). Detail view: sticky player on top with autoplay, quality dropdown, Next/Prev episode (also in fullscreen), custom fullscreen of the whole player div (sensor landscape, flips 180°, Fill default with Fit/Stretch and 16:9/4:3/21:9 screen-ratio modes), hold-finger = 2x of the current speed, vertical swipe on the video (left half = brightness, right half = system volume, HUD overlay), season chips + "All" episodes panel, resume per title/season/episode/time in localStorage (`hist`), continue-watching row, subtitles (own renderer: .srt/.vtt/.ass, size/color/background/position/sync, remembered language) with a CC menu, External player button (Android intent:// via the local proxy URL; on desktop it copies the link), Download button, Copy link (copies the `/proxy?...&dl=1` URL), Android Picture-in-Picture button. History API integration so Android Back goes back inside the app.

## Android app (serverless)
- `android-app/overlay/`: pubspec.yaml, lib/main.dart (Flutter, flutter_inappwebview, dart:ffi calls mbw_start, waits for the server, shows http://127.0.0.1:8787 in a WebView, handles back/fullscreen/intent:// /downloads, bridges brightness/volume/media-state/PiP to the page via the `mbwNative` JavaScript handler), and Kotlin: MainActivity (MethodChannel `mbw/native`: openExternal, download, volume, brightness, PiP, MediaSession, fullscreen orientation), DownloadService (foreground-service downloader with progress/cancel/complete notifications), MediaActionReceiver (notification playback buttons).
- Manifest additions are applied by `android-app/patch-manifest.sh` (permissions incl. POST_NOTIFICATIONS + FOREGROUND_SERVICE_DATA_SYNC, `supportsPictureInPicture`, DownloadService, MediaActionReceiver, FileProvider). Both `build-android.sh` and CI call it.
- `build-android.sh` (repo root): cargo-ndk builds libmoviebox_web.so for arm64-v8a, `flutter create` a fresh project into `app/`, copies the overlay + .so, patches AndroidManifest (INTERNET, cleartext), builds `moviebox-web.apk`. `.github/workflows/android.yml` does the same on GitHub Actions. `app/`, `jniLibs/`, `*.apk` are gitignored.
- `.github/workflows/release.yml`: on `v*` tags, builds Linux x86_64 static-musl, Windows x86_64, macOS universal (arm64+x86_64 via lipo), and the arm64 APK, adds SHA256SUMS, and publishes a GitHub Release. Also runnable via workflow_dispatch for build validation (publishes only for tag refs).
- Known build issue: flutter_inappwebview_android 1.1.3 references `proguard-android.txt`, which newer Android Gradle Plugin versions reject. `android-app/patch-deps.sh` rewrites it to `proguard-android-optimize.txt`; both `build-android.sh` and CI run it after `flutter pub get`.

## Run / build
- Desktop: `cargo build --release && ./target/release/moviebox-web`, open http://127.0.0.1:8787
- Android APK: see build-android.sh. Needs Rust + target aarch64-linux-android, cargo-ndk, Android SDK 36 + NDK 27.2, JDK 17, Flutter stable.
- Dev machine: Arch Linux.

## Known limitations / notes
- MovieBox changes its API often; some episodes return only the upgrade-notice clip. Other providers are fallbacks. DASH (.mpd) and .mkv streams do not play in the browser; use External player.
- BDIX providers only work from a Bangladesh ISP. VPS IPs may be blocked by MovieBox.
- Subtitles exist only for MovieBox (the repo has no subtitle support for the other providers); users can load a local file.
- Downloads run in a `dataSync` foreground service (same process as the embedded server, so the local proxy URL stays reachable). Android 13+ needs the POST_NOTIFICATIONS runtime permission for progress/completion/media notifications.
- PiP and Bluetooth controls ride on the WebView's `<video>` through a MediaSession; a few OEM builds may pause WebView media in PiP or route headset buttons to another player.
- Debug-signed APK; each CI build has a different signing key.
- Third-party client for personal use; keep credits to mesamirh (MovieBox-TUI), skumarbsft (PR #208), iptv-org in the README.

## How I want you to work
- Make small, targeted edits; show diffs or only the changed code, not whole files.
- Do not rewrite the UI as a framework app; keep static/index.html a single file.
- After Rust changes run `cargo check`; after UI changes test in a browser with a mock if the real API is unreachable.
