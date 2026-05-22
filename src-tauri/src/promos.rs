//! Promo rotation engine for ad-break overlays.
//!
//! Loads a list of `Promo` entries from a remote JSON (with disk-cache
//! and bundled-fallback chain), and picks one to show per ad break via
//! weighted-random with a last-shown cooldown.

use serde::Deserialize;

fn default_weight() -> u32 { 1 }
fn default_active() -> bool { true }

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
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
    if total_weight == 0 { return Some(candidates[0]); }

    let mut roll: u32 = rand::thread_rng().gen_range(0..total_weight);
    for p in candidates {
        let w = p.weight.max(1);
        if roll < w { return Some(p); }
        roll -= w;
    }
    candidates.first().copied()
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
}
