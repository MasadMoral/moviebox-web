use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use moviebox_tui::providers::{
    MediaDetails, ProviderKind, Release, ReleaseProvider, ResolutionIntent, SourceMirror,
};
use moviebox_tui::service::MovieBoxService;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

mod config;
use config::{ConfigStore, MovieBoxConfig, find_array};

const INDEX: &str = include_str!("../static/index.html");

// Serve static/index.html from disk when present (UI edits need no rebuild)
async fn index() -> Html<String> {
    for p in ["static/index.html", "../../static/index.html"] {
        if let Ok(s) = std::fs::read_to_string(p) {
            return Html(s);
        }
    }
    Html(INDEX.to_string())
}

async fn auth(req: Request<Body>, next: Next) -> Response {
    use base64::Engine;
    if let Ok(pw) = std::env::var("MBW_PASSWORD") {
        let ok = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Basic "))
            .and_then(|b| {
                base64::engine::general_purpose::STANDARD
                    .decode(b.trim())
                    .ok()
            })
            .and_then(|b| String::from_utf8(b).ok())
            .map(|up| up.splitn(2, ':').nth(1).unwrap_or("") == pw)
            .unwrap_or(false);
        if !ok {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(header::WWW_AUTHENTICATE, "Basic realm=\"MovieBox Web\"")
                .body(Body::from("Login required"))
                .unwrap();
        }
    }
    next.run(req).await
}

#[derive(Clone)]
struct App {
    svc: Arc<MovieBoxService>,
    http: reqwest::Client,
    cfg: Arc<ConfigStore>,
}

type Res = Result<Json<Value>, (StatusCode, String)>;
fn err<E: ToString>(e: E) -> (StatusCode, String) {
    (StatusCode::BAD_GATEWAY, e.to_string())
}

fn prov(p: &Option<String>) -> ProviderKind {
    p.as_deref()
        .and_then(ProviderKind::parse)
        .unwrap_or(ProviderKind::MovieBox)
}

#[derive(Deserialize)]
struct SearchQ {
    q: String,
    page: Option<usize>,
    p: Option<String>,
}
#[derive(Deserialize)]
struct HomeQ {
    tab: Option<String>,
    page: Option<usize>,
}
#[derive(Deserialize)]
struct IdQ {
    id: String,
    p: Option<String>,
}
#[derive(Deserialize)]
struct StreamQ {
    id: String,
    season: Option<usize>,
    episode: Option<usize>,
    p: Option<String>,
}
#[derive(Deserialize)]
struct ProxyQ {
    dl: Option<String>,
    name: Option<String>,
    url: String,
    /// base64(json [[k,v],...])
    h: Option<String>,
}
#[derive(Deserialize)]
struct DebugQ {
    id: String,
    season: Option<usize>,
    episode: Option<usize>,
}

async fn search(State(a): State<App>, Query(q): Query<SearchQ>) -> Res {
    let items = a
        .svc
        .search_typed(prov(&q.p), &q.q, q.page.unwrap_or(1))
        .await
        .map_err(err)?;
    Ok(Json(serde_json::to_value(items).map_err(err)?))
}

async fn home(State(a): State<App>, Query(q): Query<HomeQ>) -> Res {
    let (items, _m) = a
        .svc
        .homepage(q.tab.as_deref().unwrap_or("2"), q.page.unwrap_or(0))
        .await
        .map_err(err)?;
    Ok(Json(serde_json::to_value(items).map_err(err)?))
}

async fn details(State(a): State<App>, Query(q): Query<IdQ>) -> Res {
    let d: MediaDetails = a.svc.details_typed(prov(&q.p), &q.id).await.map_err(err)?;
    Ok(Json(serde_json::to_value(d).map_err(err)?))
}

/// Upstream sometimes answers with a short "update the app" clip instead of
/// the media. Never offer that clip as a playable source.
fn is_notice_url(url: &str, cfg: &MovieBoxConfig) -> bool {
    let u = url.to_ascii_lowercase();
    cfg.notice_patterns
        .iter()
        .any(|pattern| !pattern.is_empty() && u.contains(pattern.as_str()))
}

fn is_media_url(url: &str, cfg: &MovieBoxConfig) -> bool {
    let u = url.to_ascii_lowercase();
    u.starts_with("http")
        && !is_notice_url(url, cfg)
        && cfg
            .media_extensions
            .iter()
            .any(|ext| !ext.is_empty() && u.contains(ext.as_str()))
}

fn field_str<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn json_usize(v: &Value) -> Option<usize> {
    v.as_u64()
        .map(|n| n as usize)
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn json_size(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        v.as_str()
            .and_then(|s| s.trim().replace([',', '_'], "").parse().ok())
    })
}

fn json_quality(v: &Value) -> Option<String> {
    if let Some(n) = v.as_u64() {
        Some(format!("{n}p"))
    } else if let Some(n) = v.as_i64() {
        Some(format!("{n}p"))
    } else {
        v.as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

fn detector_releases_from(
    details: &Value,
    se: usize,
    ep: usize,
    ua: &str,
    language: Option<&str>,
    cfg: &MovieBoxConfig,
) -> Vec<Release> {
    let mut out = Vec::new();
    // Primary configured paths first, then the known fallbacks. This is what
    // makes playback self-heal when MovieBox renames fields / restructures the
    // payload: we probe the alternatives before giving up.
    for detector_path in cfg.detector_paths() {
        let Some(detectors) = find_array(details, std::slice::from_ref(&detector_path)) else {
            continue;
        };
        for det in detectors {
            let det_codec = field_str(det, &cfg.codec_field);
            let det_uploader = field_str(det, &cfg.uploader_field).unwrap_or("MovieBox");
            let det_source = field_str(det, &cfg.source_field)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    cfg.source_fields()
                        .get(1)
                        .and_then(|f| field_str(det, f).map(str::to_string))
                        .unwrap_or_else(|| "MovieBox".to_string())
                });
            let entries = det
                .get(&cfg.resolution_list_field)
                .and_then(Value::as_array)
                .or_else(|| {
                    cfg.entries_keys()
                        .get(1)
                        .and_then(|k| det.get(k).and_then(Value::as_array))
                });
            let Some(entries) = entries else {
                continue;
            };

            for entry in entries {
                let entry_se = entry
                    .get(&cfg.season_field)
                    .and_then(json_usize)
                    .or_else(|| {
                        cfg.season_fields()
                            .get(1)
                            .and_then(|f| entry.get(f).and_then(json_usize))
                    });
                let entry_ep = cfg
                    .episode_fields()
                    .iter()
                    .find_map(|field| entry.get(field).and_then(json_usize))
                    .or_else(|| {
                        cfg.episode_fields()
                            .get(1)
                            .and_then(|field| entry.get(field).and_then(json_usize))
                    });
                if se != 0 || ep != 0 {
                    if entry_se != Some(se) || entry_ep != Some(ep) {
                        continue;
                    }
                }

                let Some(link) = cfg.link_keys().iter().find_map(|field| {
                    entry
                        .get(field)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                }) else {
                    continue;
                };
                if !is_media_url(link, cfg) {
                    continue;
                }

                let quality = entry
                    .get(&cfg.quality_field)
                    .and_then(json_quality)
                    .or_else(|| {
                        cfg.quality_fields()
                            .get(1)
                            .and_then(|f| entry.get(f).and_then(json_quality))
                    });
                let codec = field_str(entry, &cfg.codec_field)
                    .map(str::to_string)
                    .or_else(|| {
                        cfg.codec_fields()
                            .get(1)
                            .and_then(|f| field_str(entry, f).map(str::to_string))
                    })
                    .or_else(|| det_codec.map(str::to_string));
                let size_bytes = entry.get(&cfg.size_field).and_then(json_size).or_else(|| {
                    cfg.size_fields()
                        .get(1)
                        .and_then(|f| entry.get(f).and_then(json_size))
                });
                let resource_id = cfg.resource_id_fields.iter().find_map(|field| {
                    entry.get(field).and_then(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .or_else(|| v.as_u64().map(|n| n.to_string()))
                    })
                });
                let filename = field_str(entry, &cfg.title_field)
                    .map(str::to_string)
                    .or_else(|| {
                        cfg.title_fields()
                            .get(1)
                            .and_then(|f| field_str(entry, f).map(str::to_string))
                    })
                    .unwrap_or_else(|| match &quality {
                        Some(q) => format!("{det_source} {q}"),
                        None => det_source.clone(),
                    });
                let label = field_str(entry, &cfg.uploader_field)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        cfg.uploader_fields()
                            .get(1)
                            .and_then(|f| field_str(entry, f).map(str::to_string))
                            .unwrap_or_else(|| det_uploader.to_string())
                    });

                out.push(Release {
                    provider: ProviderKind::MovieBox,
                    filename,
                    quality,
                    codec,
                    language: language.map(str::to_string),
                    size_bytes,
                    season: entry_se,
                    episode: entry_ep,
                    mirrors: vec![SourceMirror {
                        label,
                        resolver_url: link.to_string(),
                        headers: vec![("User-Agent".to_string(), ua.to_string())],
                        direct_file: true,
                    }],
                    resource_id,
                });
            }
        }
    }

    out.sort_by(|x, y| {
        y.resolution_u64()
            .cmp(&x.resolution_u64())
            .then_with(|| y.size_bytes.cmp(&x.size_bytes))
    });
    out.dedup_by(|x, y| x.quality == y.quality && x.size_bytes == y.size_bytes);
    out
}

/// Read the real media links from the subject details payload.
///
/// `play-info` and `resource` currently return a 23 second upgrade clip, but
/// `resourceDetectors[].resolutionList[]` still carries the actual signed
/// `*.hakunaymatata.com` files. Building releases from those links restores
/// playback without touching the upstream crate.
///
/// A few parent movie entries (for example the original-audio Interstellar
/// result) have no detector links of their own. Their `dubs` list points at the
/// per-language subjects that do, so try those before giving up and label the
/// releases with the dub language.
async fn detector_releases_with_cfg(
    a: &App,
    details: &Value,
    id: &str,
    se: usize,
    ep: usize,
    ua: &str,
    cfg: &MovieBoxConfig,
) -> Vec<Release> {
    let out = detector_releases_from(details, se, ep, ua, None, cfg);
    if !out.is_empty() || se != 0 || ep != 0 || !cfg.fallback_to_dubs {
        return out;
    }

    let Some(dubs) = find_array(details, &cfg.dubs_paths) else {
        return out;
    };
    for dub in dubs.iter().take(cfg.max_dub_fallbacks.min(50)) {
        let Some(sid) = dub.get(&cfg.dub_subject_field).and_then(Value::as_str) else {
            continue;
        };
        if sid.is_empty() || sid == id {
            continue;
        }
        let Ok(d) = a.svc.client.get_details(sid).await else {
            continue;
        };
        let language = dub
            .get(&cfg.dub_language_field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let rels = detector_releases_from(&d, se, ep, ua, language, cfg);
        if !rels.is_empty() {
            return rels;
        }
    }
    out
}

async fn moviebox_detector_releases(a: &App, id: &str, se: usize, ep: usize) -> Vec<Release> {
    let ua = a.svc.client.user_agent().to_string();
    a.cfg.maybe_refresh();
    let cfg = a.cfg.current();

    let Ok(details) = a.svc.client.get_details(id).await else {
        return Vec::new();
    };
    let out = detector_releases_with_cfg(a, &details, id, se, ep, &ua, &cfg.moviebox).await;
    if !out.is_empty() || se != 0 || ep != 0 {
        return out;
    }

    // No links at all. The remote rules may be stale; fetch them once without
    // blocking normal playback for more than a moment, then re-parse the same
    // details payload (no extra MovieBox request).
    if a.cfg.stale() && a.cfg.refresh_now().await {
        let fresh = a.cfg.current();
        if !Arc::ptr_eq(&cfg, &fresh) {
            return detector_releases_with_cfg(a, &details, id, se, ep, &ua, &fresh.moviebox).await;
        }
    }
    out
}

/// Fetch the real files for ONE resolution (recovering from the upgrade-notice clip).
async fn moviebox_res(a: &App, id: &str, se: usize, ep: usize, res: u32) -> Vec<Release> {
    use moviebox_tui::providers::moviebox::adapt;
    let c = &a.svc.client;
    let ua = c.user_agent().to_string();
    let mut pages = vec![ep.saturating_sub(1) / 20 + 1];
    if pages[0] != 1 {
        pages.push(1);
    }
    let mut out: Vec<Release> = Vec::new();
    for pg in pages {
        let Ok((items, _)) = c.fetch_resource_page(id, se, ep, res, pg).await else {
            continue;
        };
        for item in items {
            let mut rel = adapt::moviebox_resource_item_to_release(&item);
            if (se != 0 || ep != 0) && (rel.season != Some(se) || rel.episode != Some(ep)) {
                continue;
            }
            if rel.mirrors.is_empty() {
                let origin = item
                    .get("resourceLink")
                    .or_else(|| item.get("url"))
                    .and_then(Value::as_str)
                    .filter(|u| adapt::is_deprecation_notice_url(u));
                let (Some(rid), Some(origin)) = (rel.resource_id.clone(), origin) else {
                    continue;
                };
                let Ok(payload) = c.get_resource_position(id, &rid, origin).await else {
                    continue;
                };
                let data = payload.get("data").unwrap_or(&payload);
                let Some(list) = data.get("list").and_then(Value::as_array) else {
                    continue;
                };
                // lenient: same resource id, or the only entry returned
                let pick = list
                    .iter()
                    .find(|i| {
                        adapt::moviebox_resource_item_to_release(i)
                            .resource_id
                            .as_deref()
                            == Some(rid.as_str())
                    })
                    .or(if list.len() == 1 { list.first() } else { None });
                let Some(pick) = pick else { continue };
                let cand = adapt::moviebox_resource_item_to_release(pick);
                let Some(url) = cand.direct_url().map(str::to_string) else {
                    continue;
                };
                if !url.starts_with("http") || adapt::is_deprecation_notice_url(&url) {
                    continue;
                }
                rel.mirrors = cand.mirrors;
                for m in &mut rel.mirrors {
                    m.direct_file = true;
                    m.headers = vec![("User-Agent".to_string(), ua.clone())];
                }
            }
            out.push(rel);
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// All resolutions (1080p/720p/480p/...) that are not already in `have`, fetched in parallel.
async fn moviebox_fallback(
    a: &App,
    id: &str,
    se: usize,
    ep: usize,
    have: &std::collections::HashSet<String>,
) -> Vec<Release> {
    let mut resolutions = a
        .svc
        .client
        .fetch_collection_resolutions(id)
        .await
        .unwrap_or_default();
    if resolutions.is_empty() {
        resolutions.push(0);
    }
    resolutions.retain(|r| *r == 0 || !have.contains(&format!("{r}p")));
    let lists =
        futures::future::join_all(resolutions.iter().map(|r| moviebox_res(a, id, se, ep, *r)))
            .await;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for rel in lists.into_iter().flatten() {
        if let Some(u) = rel.direct_url() {
            if !seen.insert(u.split('?').next().unwrap_or(u).to_string()) {
                continue;
            }
        }
        out.push(rel);
    }
    out
}

async fn fetch_releases(
    a: &App,
    p: ProviderKind,
    id: &str,
    se: usize,
    ep: usize,
) -> Result<Vec<Release>, String> {
    let s = &a.svc;
    match p {
        ProviderKind::MovieBox => {
            // Prefer the detector links: play-info/resource currently return the
            // upgrade clip, while the details payload still exposes the real
            // signed files.
            let detector = moviebox_detector_releases(a, id, se, ep).await;
            if !detector.is_empty() {
                return Ok(detector);
            }

            let main = s.client.episode_streams(id, se, ep).await;
            // resolutions we already have as real (non-DASH) files
            let have: std::collections::HashSet<String> = main
                .as_ref()
                .map(|v| {
                    v.iter()
                        .filter(|r| !r.direct_url().unwrap_or("").contains(".mpd"))
                        .filter_map(|r| r.quality.clone())
                        .collect()
                })
                .unwrap_or_default();
            let extra = moviebox_fallback(a, id, se, ep, &have).await;
            match main {
                Ok(mut v) => {
                    let mut seen: std::collections::HashSet<String> = v
                        .iter()
                        .filter_map(|r| r.direct_url())
                        .map(|u| u.split('?').next().unwrap_or(u).to_string())
                        .collect();
                    for r in extra {
                        let key = r
                            .direct_url()
                            .map(|u| u.split('?').next().unwrap_or(u).to_string());
                        if key.map_or(true, |k| seen.insert(k)) {
                            v.push(r);
                        }
                    }
                    v.sort_by(|x, y| y.resolution_u64().cmp(&x.resolution_u64()));
                    Ok(v)
                }
                Err(e) => {
                    if !extra.is_empty() {
                        let mut v = extra;
                        v.sort_by(|x, y| y.resolution_u64().cmp(&x.resolution_u64()));
                        return Ok(v);
                    }
                    // surface the real upstream errors instead of a generic message
                    let pi = s.client.get_play_info(id, se, ep).await;
                    let rs = s.client.get_resources(id, se, ep, 1, None, 20).await;
                    let detail = format!(
                        "{e} | play-info: {} | resources: {}",
                        short_err(&pi),
                        short_err(&rs)
                    );
                    if se != 0 || ep != 0 {
                        Err(format!(
                            "MovieBox has no direct link for this episode. Switch to 4KHDHub or Dramachi from the provider menu (sliders icon). [{detail}]"
                        ))
                    } else {
                        Err(detail)
                    }
                }
            }
        }
        ProviderKind::FourKHdHub => match s.fourk_client.as_ref() {
            Some(c) => c
                .episode_streams(id, se, ep)
                .await
                .map_err(|e| e.to_string()),
            None => Err("4KHDHub unavailable".into()),
        },
        ProviderKind::Dramachi => s
            .dramachi_client
            .episode_streams(id, se, ep)
            .await
            .map_err(|e| e.to_string()),
        ProviderKind::BdixCircleFtp => s
            .circleftp_client
            .episode_streams(id, se, ep)
            .await
            .map_err(|e| e.to_string()),
        ProviderKind::BdixDhakaFlix => s
            .dhakaflix_client
            .episode_streams(id, se, ep)
            .await
            .map_err(|e| e.to_string()),
        ProviderKind::Addons => Err("Addons not supported here".into()),
    }
}

fn short_err<E: std::fmt::Display>(r: &Result<serde_json::Value, E>) -> String {
    match r {
        Ok(j) => {
            let t = j.to_string();
            format!("ok(no streams) {}", t.chars().take(160).collect::<String>())
        }
        Err(e) => e.to_string(),
    }
}

async fn streams(State(a): State<App>, Query(q): Query<StreamQ>) -> Res {
    let rels = fetch_releases(
        &a,
        prov(&q.p),
        &q.id,
        q.season.unwrap_or(0),
        q.episode.unwrap_or(0),
    )
    .await
    .map_err(err)?;
    let out: Vec<Value> = rels
        .iter()
        .filter_map(|r| {
            let m = r.mirrors.first()?;
            let hdrs = serde_json::to_vec(&m.headers).ok()?;
            // 4KHDHub mirrors are landing pages and MovieBox links are signed,
            // so both are re-resolved on demand via /api/resolve.
            let is_moviebox = r.provider == ProviderKind::MovieBox;
            let rel = if is_moviebox || r.provider == ProviderKind::FourKHdHub {
                serde_json::to_vec(r).ok().map(|b| base64_encode(&b))
            } else {
                None
            };
            Some(json!({
                "rel": rel,
                "sid": if is_moviebox { Some(q.id.clone()) } else { None },
                "quality": r.quality,
                "language": r.language,
                "size": r.size_bytes,
                "filename": r.filename,
                "rid": r.resource_id,
                "url": m.resolver_url,
                "h": base64_encode(&hdrs),
            }))
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
struct ResolveIn {
    rel: String,
    sid: Option<String>,
    season: Option<usize>,
    episode: Option<usize>,
}
async fn resolve(State(a): State<App>, Json(b): Json<ResolveIn>) -> Res {
    let bytes = base64_decode(&b.rel).ok_or_else(|| err("bad release"))?;
    let rel: Release = serde_json::from_slice(&bytes).map_err(err)?;

    if rel.provider == ProviderKind::MovieBox {
        if let Some(sid) = b.sid.as_deref().filter(|s| !s.is_empty()) {
            let fresh =
                moviebox_detector_releases(&a, sid, b.season.unwrap_or(0), b.episode.unwrap_or(0))
                    .await;
            let found = fresh.iter().find(|r| {
                (rel.resource_id.is_some() && r.resource_id == rel.resource_id)
                    || (r.quality == rel.quality && r.size_bytes == rel.size_bytes)
            });
            if let Some(found) = found {
                let m = found.mirrors.first().ok_or_else(|| err("no mirror"))?;
                let h = serde_json::to_vec(&m.headers).map_err(err)?;
                return Ok(Json(json!({
                    "url": m.resolver_url,
                    "h": base64_encode(&h),
                    "label": m.label,
                })));
            }
        }
        let m = rel.mirrors.first().ok_or_else(|| err("no mirror"))?;
        let h = serde_json::to_vec(&m.headers).map_err(err)?;
        return Ok(Json(json!({
            "url": m.resolver_url,
            "h": base64_encode(&h),
            "label": m.label,
        })));
    }

    let c = a
        .svc
        .fourk_client
        .as_ref()
        .ok_or_else(|| err("4KHDHub unavailable"))?;
    let ps = c
        .resolve_release(&rel, ResolutionIntent::Playback)
        .await
        .map_err(err)?;
    let h = serde_json::to_vec(&ps.headers).map_err(err)?;
    Ok(Json(
        json!({"url": ps.url, "h": base64_encode(&h), "label": ps.source_label}),
    ))
}

#[derive(Deserialize)]
struct SubQ {
    id: String,
    rid: Option<String>,
    sib: Option<String>,
    season: Option<usize>,
    episode: Option<usize>,
}
/// Subtitle list (MovieBox): [{name, url}]; the page fetches `url` through /proxy
async fn subs(State(a): State<App>, Query(q): Query<SubQ>) -> Json<Value> {
    let sib: Vec<String> = q
        .sib
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let list = a
        .svc
        .get_ext_captions(
            &q.id,
            q.rid.as_deref().unwrap_or(""),
            &sib,
            q.season.unwrap_or(0),
            q.episode.unwrap_or(0),
        )
        .await
        .unwrap_or_default();
    Json(Value::Array(
        list.iter()
            .map(|s| {
                json!({
                    "name": moviebox_tui::tui::text::sanitize_language_label(&s.name),
                    "url": s.url,
                })
            })
            .collect(),
    ))
}

async fn providers() -> Json<Value> {
    Json(Value::Array(
        ProviderKind::ENABLED
            .iter()
            .map(|p| json!({"id": p.cache_key(), "label": p.label()}))
            .collect(),
    ))
}

async fn config_info(State(a): State<App>) -> Json<Value> {
    Json(a.cfg.info())
}

async fn config_refresh(State(a): State<App>) -> Json<Value> {
    let _ = a.cfg.refresh_now().await;
    Json(a.cfg.info())
}

/// The quickest way to see what the app actually receives from MovieBox.
/// Open this in a browser while the server is running:
/// `/api/debug/moviebox?id=SUBJECT_ID&season=1&episode=1`
async fn debug_moviebox(State(a): State<App>, Query(q): Query<DebugQ>) -> Res {
    let se = q.season.unwrap_or(0);
    let ep = q.episode.unwrap_or(0);
    let details = a.svc.client.get_details(&q.id).await.map_err(err)?;
    let play_info = a.svc.client.get_play_info(&q.id, se, ep).await;
    let resources = a.svc.client.get_resources(&q.id, se, ep, 1, None, 20).await;
    let cfg = a.cfg.current();
    let parsed = detector_releases_from(
        &details,
        se,
        ep,
        a.svc.client.user_agent(),
        None,
        &cfg.moviebox,
    );
    Ok(Json(json!({
        "id": q.id,
        "season": se,
        "episode": ep,
        "config_version": cfg.version,
        "details": details,
        "play_info": play_info.as_ref().ok(),
        "play_info_error": play_info.as_ref().err().map(ToString::to_string),
        "resources": resources.as_ref().ok(),
        "resources_error": resources.as_ref().err().map(ToString::to_string),
        "parsed_releases": serde_json::to_value(&parsed).unwrap_or(Value::Null),
    })))
}

#[derive(Deserialize)]
struct SugQ {
    q: String,
    p: Option<String>,
}
async fn suggest(State(a): State<App>, Query(q): Query<SugQ>) -> Json<Value> {
    let items = a
        .svc
        .search_typed(prov(&q.p), &q.q, 1)
        .await
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let out: Vec<Value> = items
        .into_iter()
        .filter(|i| seen.insert(i.id.value.clone()))
        .take(8)
        .map(|i| {
            json!({
                "id": i.id.value,
                "title": i.title,
                "year": i.year,
                "poster": i.poster_url,
                "type": if i.media_type == moviebox_tui::providers::MediaType::Series { "series" } else { "movie" },
            })
        })
        .collect();
    Json(Value::Array(out))
}

fn base64_encode(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .ok()
}

/// Range-aware passthrough so <video> can play streams that need custom headers.
async fn proxy(State(a): State<App>, Query(q): Query<ProxyQ>, req: HeaderMap) -> Response {
    let mut rb = a.http.get(&q.url);
    if let Some(h) = q.h.as_deref().and_then(base64_decode) {
        if let Ok(pairs) = serde_json::from_slice::<Vec<(String, String)>>(&h) {
            for (k, v) in pairs {
                rb = rb.header(k, v);
            }
        }
    }
    if let Some(r) = req.get(header::RANGE) {
        rb = rb.header(header::RANGE, r);
    }
    let up = match rb.send().await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };
    let mut out = Response::builder().status(up.status());
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(v) = up.headers().get(&name) {
            out = out.header(HeaderName::from(name), HeaderValue::from(v.clone()));
        }
    }
    if q.dl.is_some() {
        let name: String = q
            .name
            .clone()
            .unwrap_or_else(|| "video.mp4".into())
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " ._-()[]".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        out = out.header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        );
    }
    out.body(Body::from_stream(up.bytes_stream()))
        .unwrap_or_else(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())
}

/// Run the server. `wait_init`: try to open the MovieBox session (max 10s) before binding.
pub async fn serve(addr: String, wait_init: bool) -> std::io::Result<()> {
    let svc = Arc::new(MovieBoxService::new());
    if wait_init {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), svc.client.init()).await;
    } else {
        let c = svc.client.clone();
        tokio::spawn(async move {
            let _ = c.init().await;
        });
    }
    let http = reqwest::Client::builder().build().unwrap();
    let cfg = Arc::new(ConfigStore::new());
    // Fetch rules in the background; startup and requests keep using the
    // bundled/cached copy, so a slow network never delays the app.
    cfg.maybe_refresh();
    let app = Router::new()
        .route("/", get(index))
        .route("/api/search", get(search))
        .route("/api/home", get(home))
        .route("/api/details", get(details))
        .route("/api/streams", get(streams))
        .route("/api/providers", get(providers))
        .route("/api/config", get(config_info))
        .route("/api/config/refresh", post(config_refresh))
        .route("/api/debug/moviebox", get(debug_moviebox))
        .route("/api/subs", get(subs))
        .route("/api/resolve", post(resolve))
        .route("/api/suggest", get(suggest))
        .route("/proxy", get(proxy))
        .with_state(App { svc, http, cfg });
    // optional MBW_PASSWORD (HTTP basic auth, any username)
    let app = app.layer(middleware::from_fn(auth));
    if std::env::var("MBW_PASSWORD").is_ok() {
        println!("Password protection: ON");
    } else if !addr.starts_with("127.") && !addr.starts_with("localhost") {
        println!(
            "WARNING: listening on {addr} without MBW_PASSWORD - anyone who can reach it can use it!"
        );
    }
    println!("Open http://{addr} in your browser");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}

// ---------- C ABI for the Android app (Flutter calls this through dart:ffi) ----------
static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts the server on 127.0.0.1:`port` in a background thread. `data_dir` is the app's
/// private files folder (used for the session/cache). Returns 0 on success, 1 if already running.
///
/// # Safety
/// `data_dir` must be a valid NUL-terminated string (or null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbw_start(port: u16, data_dir: *const std::os::raw::c_char) -> i32 {
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return 1;
    }
    if !data_dir.is_null() {
        if let Ok(dir) = unsafe { std::ffi::CStr::from_ptr(data_dir) }.to_str() {
            // the library finds its cache/session folders through these
            unsafe {
                std::env::set_var("HOME", dir);
                std::env::set_var("XDG_CONFIG_HOME", format!("{dir}/config"));
                std::env::set_var("XDG_CACHE_HOME", format!("{dir}/cache"));
                std::env::set_var("XDG_DATA_HOME", format!("{dir}/data"));
            }
        }
    }
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        if let Err(e) = rt.block_on(serve(format!("127.0.0.1:{port}"), false)) {
            eprintln!("moviebox-web server stopped: {e}");
            STARTED.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    });
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detector_parser_uses_real_signed_links_only() {
        let payload = json!({
            "data": {
                "resourceDetectors": [{
                    "codecName": "h264",
                    "uploadBy": "uploader",
                    "source": "vegamovies",
                    "resolutionList": [
                        {
                            "resolution": 720,
                            "size": "1809858130",
                            "resourceId": "abc",
                            "resourceLink": "https://bcdn.hakunaymatata.com/vegamovies/x.mp4?sign=a&t=1",
                            "title": "Example"
                        },
                        {
                            "resolution": 480,
                            "size": "824720",
                            "resourceId": "def",
                            "resourceLink": "https://macdn.aoneroom.com/other/2026/10/10/notice.mp4",
                            "title": "Notice"
                        },
                        {
                            "resolution": 360,
                            "size": "204190718",
                            "resourceId": "ghi",
                            "resourceLink": "https://vegamovies.market/abc/Example-360P",
                            "title": "Landing page"
                        }
                    ]
                }]
            }
        });

        let releases = detector_releases_from(
            &payload,
            0,
            0,
            "TestAgent/1.0",
            None,
            &MovieBoxConfig::default(),
        );
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].quality.as_deref(), Some("720p"));
        assert_eq!(releases[0].size_bytes, Some(1_809_858_130));
        assert_eq!(releases[0].resource_id.as_deref(), Some("abc"));
        assert!(releases[0].mirrors[0].direct_file);
        assert_eq!(
            releases[0].mirrors[0].headers,
            vec![("User-Agent".to_string(), "TestAgent/1.0".to_string())]
        );
    }

    #[test]
    fn detector_parser_filters_requested_episode() {
        let payload = json!({
            "data": {
                "resourceDetectors": [{
                    "uploadBy": "uploader",
                    "resolutionList": [
                        {
                            "se": 1, "ep": 1, "resolution": 1080, "size": "900000000",
                            "resourceId": "111", "resourceLink": "https://bcdn.hakunaymatata.com/ep1.mp4?sign=a&t=1"
                        },
                        {
                            "se": 1, "ep": 2, "resolution": 1080, "size": "800000000",
                            "resourceId": "222", "resourceLink": "https://bcdn.hakunaymatata.com/ep2.mp4?sign=b&t=1"
                        }
                    ]
                }]
            }
        });

        let releases = detector_releases_from(
            &payload,
            1,
            2,
            "TestAgent/1.0",
            None,
            &MovieBoxConfig::default(),
        );
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].resource_id.as_deref(), Some("222"));
        assert_eq!(releases[0].season, Some(1));
        assert_eq!(releases[0].episode, Some(2));
    }

    #[test]
    fn detector_parser_follows_remote_rule_names() {
        let payload = json!({
            "result": {
                "rows": [{
                    "codec": "hevc",
                    "uploader": "AnotherUploader",
                    "origin": "MirrorSite",
                    "links": [{
                        "file": "https://cdn.example/movie.mkv?sign=a&t=1",
                        "height": 1080,
                        "bytes": "123456",
                        "uuid": "remote-1",
                        "name": "Remote title",
                        "uploader": "Encoder"
                    }]
                }]
            }
        });

        let mut rules = MovieBoxConfig::default();
        rules.detectors_paths = vec!["result.rows".into()];
        rules.resolution_list_field = "links".into();
        rules.link_fields = vec!["file".into()];
        rules.quality_field = "height".into();
        rules.size_field = "bytes".into();
        rules.codec_field = "codec".into();
        rules.resource_id_fields = vec!["uuid".into()];
        rules.title_field = "name".into();
        rules.uploader_field = "uploader".into();
        rules.season_field = "season".into();
        rules.episode_fields = vec!["part".into()];
        rules.media_extensions = vec![".mkv".into()];

        let releases = detector_releases_from(&payload, 0, 0, "TestAgent/1.0", None, &rules);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].quality.as_deref(), Some("1080p"));
        assert_eq!(releases[0].size_bytes, Some(123456));
        assert_eq!(releases[0].resource_id.as_deref(), Some("remote-1"));
        assert_eq!(releases[0].filename, "Remote title");
        assert_eq!(releases[0].mirrors[0].label, "Encoder");
        assert_eq!(releases[0].codec.as_deref(), Some("hevc"));
    }
    #[test]
    fn detector_parser_falls_back_to_alternative_field_names() {
        let payload = json!({
            "data": {
                "resourceDetectors": [{
                    "uploader": "uploader",
                    "source": "source",
                    "links": [{
                        "season": 1, "episode": 2, "height": 1080, "bytes": "123456",
                        "uuid": "remote-1", "name": "Remote title", "uploader": "Encoder",
                        "file": "https://cdn.example/movie.mkv?sign=a&t=1"
                    }]
                }]
            }
        });

        let releases = detector_releases_from(
            &payload,
            1,
            2,
            "TestAgent/1.0",
            None,
            &MovieBoxConfig::default(),
        );
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].quality.as_deref(), Some("1080p"));
        assert_eq!(releases[0].size_bytes, Some(123456));
        assert_eq!(releases[0].filename, "Remote title");
        assert_eq!(releases[0].mirrors[0].label, "Encoder");
        assert!(releases[0].mirrors[0].direct_file);
    }
}
