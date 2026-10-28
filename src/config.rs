//! Remote playback rules.
//!
//! MovieBox changes field names and notice URLs from time to time. The parser
//! reads this small JSON document, so those changes can be fixed without
//! rebuilding the app. A bundled copy is always available and the last good
//! remote copy is cached on disk.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const BUNDLED_JSON: &str = include_str!("../config/provider.json");
pub const DEFAULT_URL: &str =
    "https://raw.githubusercontent.com/MasadMoral/moviebox-web/main/config/provider.json";

/// Refresh the remote rules in the background after this many seconds.
const REFRESH_SECS: u64 = 300;
/// After a failed fetch, try again sooner than the normal refresh interval.
const RETRY_SECS: u64 = 45;
/// A playback failure may wait this long for a fresh rule set.
const FORCE_REFRESH_MS: u64 = 1_800;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub moviebox: MovieBoxConfig,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            version: 1,
            moviebox: MovieBoxConfig::default(),
        }
    }
}

impl ProviderConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 {
            return Err("version must be at least 1".into());
        }
        let m = &self.moviebox;
        if m.detectors_paths.is_empty() || m.detectors_paths.iter().any(|p| p.trim().is_empty()) {
            return Err("moviebox.detectors_paths must contain at least one path".into());
        }
        if m.resolution_list_field.trim().is_empty() {
            return Err("moviebox.resolution_list_field must not be empty".into());
        }
        if m.link_fields.is_empty() || m.link_fields.iter().any(|p| p.trim().is_empty()) {
            return Err("moviebox.link_fields must contain at least one field".into());
        }
        if m.quality_field.trim().is_empty()
            || m.size_field.trim().is_empty()
            || m.title_field.trim().is_empty()
            || m.season_field.trim().is_empty()
        {
            return Err("moviebox field names must not be empty".into());
        }
        if m.episode_fields.is_empty() || m.episode_fields.iter().any(|p| p.trim().is_empty()) {
            return Err("moviebox.episode_fields must contain at least one field".into());
        }
        if m.dubs_paths.is_empty() || m.dubs_paths.iter().any(|p| p.trim().is_empty()) {
            return Err("moviebox.dubs_paths must contain at least one path".into());
        }
        if m.dub_subject_field.trim().is_empty() || m.dub_language_field.trim().is_empty() {
            return Err("moviebox dub fields must not be empty".into());
        }
        if m.media_extensions.is_empty() || m.media_extensions.iter().any(|p| p.trim().is_empty()) {
            return Err("moviebox.media_extensions must contain at least one extension".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct MovieBoxConfig {
    /// Where the detector array lives. Both are tried; the first match wins.
    pub detectors_paths: Vec<String>,
    /// Field inside each detector that holds the downloadable entries.
    pub resolution_list_field: String,
    /// Entry fields that may contain the real media URL.
    pub link_fields: Vec<String>,
    pub quality_field: String,
    pub size_field: String,
    pub codec_field: String,
    pub resource_id_fields: Vec<String>,
    pub title_field: String,
    pub uploader_field: String,
    pub source_field: String,
    pub season_field: String,
    pub episode_fields: Vec<String>,
    /// Where the per-language dub subjects live.
    pub dubs_paths: Vec<String>,
    pub dub_subject_field: String,
    pub dub_language_field: String,
    pub fallback_to_dubs: bool,
    pub max_dub_fallbacks: usize,
    /// Lowercase substrings that identify the "update the app" clip.
    pub notice_patterns: Vec<String>,
    /// Lowercase extensions accepted as playable media.
    pub media_extensions: Vec<String>,
}

impl Default for MovieBoxConfig {
    fn default() -> Self {
        Self {
            detectors_paths: vec!["resourceDetectors".into(), "data.resourceDetectors".into()],
            resolution_list_field: "resolutionList".into(),
            link_fields: vec![
                "resourceLink".into(),
                "url".into(),
                "downloadUrl".into(),
                "playUrl".into(),
            ],
            quality_field: "resolution".into(),
            size_field: "size".into(),
            codec_field: "codecName".into(),
            resource_id_fields: vec!["resourceId".into(), "id".into()],
            title_field: "title".into(),
            uploader_field: "uploadBy".into(),
            source_field: "source".into(),
            season_field: "se".into(),
            episode_fields: vec!["ep".into(), "episode".into()],
            dubs_paths: vec!["dubs".into(), "data.dubs".into()],
            dub_subject_field: "subjectId".into(),
            dub_language_field: "lanName".into(),
            fallback_to_dubs: true,
            max_dub_fallbacks: 12,
            notice_patterns: vec![
                "/notice.mp4".into(),
                "macdn.aoneroom.com/other/".into(),
                "1c7de0bd3393702d9191801f15f88f8d".into(),
                "9a0461bc39da389663bf3dbb17091d3f".into(),
                "b164fbfb4347792950bdfbfb563d39d9".into(),
            ],
            media_extensions: vec![
                ".mp4".into(),
                ".m3u8".into(),
                ".mpd".into(),
                ".mkv".into(),
                ".webm".into(),
                ".m4v".into(),
                ".ts".into(),
            ],
        }
    }
}

fn default_version() -> u32 {
    1
}

/// Parse and validate a rules document.
pub fn parse_config(text: &str) -> Result<ProviderConfig, String> {
    let mut cfg: ProviderConfig =
        serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
    normalize(&mut cfg);
    cfg.validate()?;
    Ok(cfg)
}

fn normalize(cfg: &mut ProviderConfig) {
    let m = &mut cfg.moviebox;
    for value in &mut m.notice_patterns {
        *value = value.trim().to_ascii_lowercase();
    }
    for value in &mut m.media_extensions {
        *value = value.trim().to_ascii_lowercase();
    }
    for list in [
        &mut m.detectors_paths,
        &mut m.dubs_paths,
        &mut m.link_fields,
        &mut m.resource_id_fields,
        &mut m.episode_fields,
    ] {
        for value in list {
            *value = value.trim().to_string();
        }
    }
    for value in [
        &mut m.resolution_list_field,
        &mut m.quality_field,
        &mut m.size_field,
        &mut m.codec_field,
        &mut m.title_field,
        &mut m.uploader_field,
        &mut m.source_field,
        &mut m.season_field,
        &mut m.dub_subject_field,
        &mut m.dub_language_field,
    ] {
        *value = value.trim().to_string();
    }
}

/// Walk a dotted object path, for example `data.resourceDetectors`.
pub fn get_dot_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim();
    if path.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current)
}

/// Find the first configured path that points at an array.
pub fn find_array<'a>(root: &'a Value, paths: &[String]) -> Option<&'a Vec<Value>> {
    paths
        .iter()
        .find_map(|path| get_dot_path(root, path).and_then(Value::as_array))
}

/// Runtime rules store with a bundled fallback, a disk cache, and a remote
/// refresh that never blocks normal requests.
pub struct ConfigStore {
    current: RwLock<Arc<ProviderConfig>>,
    source: RwLock<String>,
    last_fetch: AtomicU64,
    client: reqwest::Client,
    url: Option<String>,
    cache_path: PathBuf,
    lock: tokio::sync::Mutex<()>,
}

impl ConfigStore {
    pub fn new() -> Self {
        let cache_path = cache_path();
        let mut cfg = parse_config(BUNDLED_JSON).unwrap_or_default();
        let mut source = "bundled".to_string();
        if let Ok(text) = std::fs::read_to_string(&cache_path) {
            if let Ok(cached) = parse_config(&text) {
                cfg = cached;
                source = "cache".to_string();
            }
        }

        let url = match std::env::var("MBW_CONFIG_URL") {
            Ok(value) => {
                let value = value.trim();
                if value.is_empty() || value.eq_ignore_ascii_case("off") || value == "0" {
                    None
                } else {
                    Some(value.to_string())
                }
            }
            Err(_) => Some(DEFAULT_URL.to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(2))
            .user_agent(format!("moviebox-web/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            current: RwLock::new(Arc::new(cfg)),
            source: RwLock::new(source),
            last_fetch: AtomicU64::new(0),
            client,
            url,
            cache_path,
            lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn current(&self) -> Arc<ProviderConfig> {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn source(&self) -> String {
        self.source
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn url(&self) -> Option<String> {
        self.url.clone()
    }

    pub fn cache_path(&self) -> &PathBuf {
        &self.cache_path
    }

    pub fn last_fetch(&self) -> u64 {
        self.last_fetch.load(Ordering::Relaxed)
    }

    pub fn age_secs(&self) -> u64 {
        let last = self.last_fetch();
        if last == 0 {
            u64::MAX
        } else {
            now_secs().saturating_sub(last)
        }
    }

    /// True when a playback failure should try a forced rules refresh.
    pub fn stale(&self) -> bool {
        self.age_secs() >= REFRESH_SECS
    }

    /// Kick off a background refresh when the TTL has expired. The current
    /// request keeps using the rules it already has.
    pub fn maybe_refresh(self: &Arc<Self>) {
        if self.url.is_none() || self.age_secs() < REFRESH_SECS {
            return;
        }
        let store = self.clone();
        tokio::spawn(async move {
            let _ = store.refresh(false).await;
        });
    }

    /// Fetch fresh rules now. Bounded so a slow network cannot hold a playback
    /// request open for long. Returns true when a remote document was applied.
    pub async fn refresh_now(&self) -> bool {
        if self.url.is_none() {
            return false;
        }
        tokio::time::timeout(Duration::from_millis(FORCE_REFRESH_MS), self.refresh(true))
            .await
            .unwrap_or(false)
    }

    async fn refresh(&self, force: bool) -> bool {
        let Some(base_url) = self.url.as_deref() else {
            return false;
        };
        let _guard = self.lock.lock().await;

        let now = now_secs();
        if !force && now.saturating_sub(self.last_fetch()) < REFRESH_SECS {
            return false;
        }

        // A timestamp makes GitHub's raw CDN deliver the newest rules instead
        // of a five-minute-old edge copy.
        let separator = if base_url.contains('?') { '&' } else { '?' };
        let url = format!("{base_url}{separator}t={now}");

        let response = self
            .client
            .get(&url)
            .header(reqwest::header::CACHE_CONTROL, "no-cache")
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await;
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                eprintln!("provider rules: HTTP {} from remote", response.status());
                self.schedule_retry(now);
                return false;
            }
            Err(error) => {
                eprintln!("provider rules: {error}");
                self.schedule_retry(now);
                return false;
            }
        };
        let text = match response.text().await {
            Ok(text) => text,
            Err(error) => {
                eprintln!("provider rules: could not read response: {error}");
                self.schedule_retry(now);
                return false;
            }
        };
        let cfg = match parse_config(&text) {
            Ok(cfg) => cfg,
            Err(error) => {
                eprintln!("provider rules: ignored invalid remote document: {error}");
                self.schedule_retry(now);
                return false;
            }
        };

        *self.current.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(cfg);
        *self.source.write().unwrap_or_else(|e| e.into_inner()) = "remote".to_string();
        self.last_fetch.store(now, Ordering::Relaxed);
        self.write_cache(&text);
        println!(
            "provider rules: loaded remote version {}",
            self.current().version
        );
        true
    }

    fn schedule_retry(&self, now: u64) {
        let next = now.saturating_sub(REFRESH_SECS).saturating_add(RETRY_SECS);
        self.last_fetch.store(next, Ordering::Relaxed);
    }

    fn write_cache(&self, text: &str) {
        if let Some(parent) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.cache_path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.cache_path);
        }
    }

    pub fn info(&self) -> Value {
        let cfg = self.current();
        let last = self.last_fetch();
        json!({
            "version": cfg.version,
            "source": self.source(),
            "url": self.url(),
            "last_fetch": if last == 0 { Value::Null } else { json!(last) },
            "age_secs": if last == 0 { Value::Null } else { json!(now_secs().saturating_sub(last)) },
            "cache_path": self.cache_path().display().to_string(),
            "rules": &cfg.moviebox,
        })
    }
}

fn cache_path() -> PathBuf {
    if let Ok(path) = std::env::var("MBW_CONFIG_CACHE") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("moviebox-web").join("provider.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_rules_are_valid() {
        let cfg = parse_config(BUNDLED_JSON).expect("bundled rules");
        assert!(cfg.version >= 1);
        assert!(cfg.moviebox.media_extensions.contains(&".mp4".to_string()));
    }

    #[test]
    fn missing_fields_use_bundled_defaults() {
        let cfg = parse_config(r#"{"version":2}"#).expect("minimal rules");
        assert_eq!(cfg.version, 2);
        assert!(cfg.moviebox.fallback_to_dubs);
        assert!(cfg.moviebox.media_extensions.contains(&".mp4".to_string()));
    }

    #[test]
    fn rejects_empty_link_fields() {
        let text = r#"{"version":1,"moviebox":{"link_fields":[]}}"#;
        assert!(parse_config(text).is_err());
    }

    #[test]
    fn dotted_paths_work() {
        let value = json!({"data": {"items": [1, 2]}});
        assert_eq!(
            get_dot_path(&value, "data.items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }
}
