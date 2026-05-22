//! Promo rotation engine for ad-break overlays.
//!
//! Loads a list of `Promo` entries from a remote JSON (with disk-cache
//! and bundled-fallback chain), and picks one to show per ad break via
//! weighted-random with a last-shown cooldown.

use serde::{Deserialize, Serialize};

fn default_weight() -> u32 { 1 }
fn default_active() -> bool { true }

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Promo {
    pub id: String,
    pub product_name: String,
    pub tagline: String,
    pub url: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_active")]
    pub active: bool,
    #[serde(default)]
    pub cta_text: Option<String>,
    #[serde(default)]
    pub accent_color: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PromosFile {
    pub version: u32,
    pub promos: Vec<Promo>,
}

/// Pick one promo from the pool using weighted random with a last-shown
/// cooldown. Returns `None` only when the pool is empty.
///
/// - Inactive entries (`active: false`) are excluded.
/// - Entries with `weight == 0` are treated as weight 1 (prevents
///   accidentally-zero weights from making them un-pickable).
/// - When `last_shown_id` matches an entry, that entry is excluded
///   from the draw — unless excluding it would leave zero candidates,
///   in which case cooldown is ignored.
pub fn pick_next_promo<'a>(pool: &'a [Promo], last_shown_id: Option<&str>) -> Option<&'a Promo> {
    use rand::Rng;
    let active: Vec<&Promo> = pool.iter().filter(|p| p.active).collect();
    if active.is_empty() { return None; }

    let after_cooldown: Vec<&Promo> = active.iter()
        .copied()
        .filter(|p| last_shown_id.map_or(true, |id| p.id != id))
        .collect();

    let candidates: &[&Promo] = if after_cooldown.is_empty() {
        // Cooldown would have removed all candidates (e.g. only one active
        // promo and it was just shown). Ignore cooldown so we still pick
        // something.
        &active[..]
    } else {
        &after_cooldown[..]
    };

    let total_weight: u32 = candidates.iter().map(|p| p.weight.max(1)).sum();

    let mut roll: u32 = rand::rng().random_range(0..total_weight);
    for p in candidates {
        let w = p.weight.max(1);
        if roll < w { return Some(p); }
        roll -= w;
    }
    candidates.first().copied()
}

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const REMOTE_URL: &str = "https://syvrstudios.com/hum/promos.json";
const CACHE_FILE_NAME: &str = "promos.json";
const FETCH_TIMEOUT_SECS: u64 = 5;
const REFRESH_INTERVAL_HOURS: u64 = 6;

/// A source of promos. Phase 2 introduces UserLocalSource alongside this.
pub trait PromoSource: Send + Sync {
    fn name(&self) -> &'static str;
    fn promos(&self) -> Vec<Promo>;
}

/// Fetches from `REMOTE_URL`, falls back to disk cache, falls back to
/// bundled defaults, falls back to a single hardcoded entry. The pool
/// is always non-empty after `bootstrap_load()`.
pub struct SyvrRemoteSource {
    pool: Arc<RwLock<Vec<Promo>>>,
    cache_path: PathBuf,
}

impl SyvrRemoteSource {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            pool: Arc::new(RwLock::new(Vec::new())),
            cache_path: cache_dir.join(CACHE_FILE_NAME),
        }
    }

    /// Synchronous bootstrap: read the disk cache (or bundled fallback)
    /// to populate the pool before the app's first ad break. Network
    /// refresh happens in the background.
    pub fn bootstrap_load(&self) {
        let from_disk = std::fs::read_to_string(&self.cache_path)
            .ok()
            .and_then(|s| serde_json::from_str::<PromosFile>(&s).ok());
        let pool = match from_disk {
            Some(f) if f.version == 1 && !f.promos.is_empty() => f.promos,
            _ => bundled_defaults(),
        };
        let pool_arc = self.pool.clone();
        let _guard = tauri::async_runtime::block_on(async move {
            let mut w = pool_arc.write().await;
            *w = pool;
        });
    }

    /// Long-running background task: fetch every REFRESH_INTERVAL_HOURS.
    /// Refreshes the in-memory pool AND writes the cache file on success.
    /// Silent failure on network error — the existing pool stays valid.
    pub async fn run_refresh_loop(self: Arc<Self>) {
        // Initial fetch right at startup (separate from bootstrap_load,
        // which reads from disk synchronously).
        self.refresh_once().await;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            REFRESH_INTERVAL_HOURS * 60 * 60,
        ));
        interval.tick().await; // skip the immediate tick
        loop {
            interval.tick().await;
            self.refresh_once().await;
        }
    }

    async fn refresh_once(&self) {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[promos] http client build failed: {e}");
                return;
            }
        };
        let resp = match client.get(REMOTE_URL).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[promos] fetch failed: {e}");
                return;
            }
        };
        if !resp.status().is_success() {
            eprintln!("[promos] fetch returned {}", resp.status());
            return;
        }
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[promos] body read failed: {e}");
                return;
            }
        };
        let parsed: PromosFile = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[promos] json parse failed: {e}");
                return;
            }
        };
        if parsed.version != 1 || parsed.promos.is_empty() {
            eprintln!("[promos] unexpected schema or empty list — keeping current pool");
            return;
        }
        // Write cache before swapping pool — if cache write fails the
        // in-memory pool still updates (better than the inverse).
        if let Some(parent) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&self.cache_path, &body) {
            eprintln!("[promos] cache write failed: {e}");
        }
        {
            let mut w = self.pool.write().await;
            *w = parsed.promos;
        }
        eprintln!("[promos] refreshed pool from {REMOTE_URL}");
    }
}

impl PromoSource for SyvrRemoteSource {
    fn name(&self) -> &'static str { "syvr-remote" }
    fn promos(&self) -> Vec<Promo> {
        // Block briefly on the async read. The pool is small, contention
        // is negligible, and pick_next_promo is called once per ad break
        // (not in a hot loop).
        tauri::async_runtime::block_on(async {
            self.pool.read().await.clone()
        })
    }
}

impl SyvrRemoteSource {
    /// Async pool read — used by the lyrics resolver (which runs inside
    /// `tauri::async_runtime::spawn`) and by tests that already hold a
    /// tokio runtime. The sync `PromoSource::promos()` implementation
    /// cannot be called from inside a running tokio runtime.
    pub async fn promos_async(&self) -> Vec<Promo> {
        self.pool.read().await.clone()
    }

    /// Seed the pool asynchronously — used in tests.
    #[cfg(test)]
    pub async fn seed_with_defaults(&self) {
        let mut w = self.pool.write().await;
        *w = bundled_defaults();
    }
}

pub fn bundled_defaults() -> Vec<Promo> {
    const RAW: &str = include_str!("../resources/default_promos.json");
    serde_json::from_str::<PromosFile>(RAW)
        .map(|f| f.promos)
        .unwrap_or_else(|_| vec![Promo {
            id: "syvr-studios".into(),
            product_name: "SYVR Studios".into(),
            tagline: "Tools and apps from the makers of Hum.".into(),
            url: "https://syvrstudios.com".into(),
            icon_url: None,
            weight: 1,
            active: true,
            cta_text: None,
            accent_color: None,
        }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promo(id: &str, weight: u32, active: bool) -> Promo {
        Promo {
            id: id.into(),
            product_name: id.into(),
            tagline: "x".into(),
            url: "https://example.com".into(),
            icon_url: None,
            weight,
            active,
            cta_text: None,
            accent_color: None,
        }
    }

    #[test]
    fn empty_pool_returns_none() {
        assert!(pick_next_promo(&[], None).is_none());
    }

    #[test]
    fn all_inactive_returns_none() {
        let pool = vec![promo("a", 1, false), promo("b", 1, false)];
        assert!(pick_next_promo(&pool, None).is_none());
    }

    #[test]
    fn cooldown_excludes_last_shown_when_alternatives_exist() {
        let pool = vec![promo("a", 1, true), promo("b", 1, true)];
        for _ in 0..50 {
            let picked = pick_next_promo(&pool, Some("a")).unwrap();
            assert_eq!(picked.id, "b");
        }
    }

    #[test]
    fn cooldown_ignored_when_only_one_active_promo() {
        let pool = vec![promo("a", 1, true)];
        let picked = pick_next_promo(&pool, Some("a")).unwrap();
        assert_eq!(picked.id, "a");
    }

    #[test]
    fn weight_zero_treated_as_one() {
        let pool = vec![promo("a", 0, true), promo("b", 0, true)];
        // Both weight-0 → both treated as 1 → uniform draw. Just verify
        // we don't crash and pick *something* from the active set.
        let picked = pick_next_promo(&pool, None).unwrap();
        assert!(picked.id == "a" || picked.id == "b");
    }

    #[test]
    fn higher_weight_picked_more_often() {
        let pool = vec![promo("rare", 1, true), promo("common", 9, true)];
        let mut common = 0;
        let mut rare = 0;
        for _ in 0..10_000 {
            let picked = pick_next_promo(&pool, None).unwrap();
            if picked.id == "common" { common += 1; } else { rare += 1; }
        }
        // 90/10 distribution within ±3% slack.
        assert!(common > rare * 7, "common={common} rare={rare}");
    }

    #[test]
    fn bundled_default_parses() {
        let raw = include_str!("../resources/default_promos.json");
        let parsed: PromosFile = serde_json::from_str(raw).expect("default_promos.json must parse");
        assert_eq!(parsed.version, 1);
        assert!(!parsed.promos.is_empty(), "bundled defaults cannot be empty");
        for p in &parsed.promos {
            assert!(!p.id.is_empty(), "every default promo needs an id");
            assert!(p.url.starts_with("https://"), "default promo url must be https: {}", p.url);
        }
    }

    #[test]
    fn bundled_defaults_helper_returns_non_empty() {
        let pool = super::bundled_defaults();
        assert!(!pool.is_empty(), "bundled_defaults() must never return empty");
        assert!(pool.iter().all(|p| !p.id.is_empty()));
    }
}
