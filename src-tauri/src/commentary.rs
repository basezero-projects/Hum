//! Claude-API-backed song commentary.
//!
//! Generates 2-3 sentence context notes per track (references, era,
//! samples, callbacks) on demand. Results are cached in-memory by
//! track key (`artist|title|album`) so replays don't re-spend API
//! tokens. Persistent caching is a follow-up — keeps the v1 surface
//! simple.
//!
//! API key comes from `settings.claude_api_key`. When empty, the
//! Tauri command returns an empty string (UI-side displays a "Set
//! your Claude API key in Settings → Commentary" placeholder).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;

use crate::settings::SharedSettings;

// Sonnet 4.5 — small, cheap, fast for ~200-token responses.
const MODEL: &str = "claude-sonnet-4-5";
const MAX_TOKENS: u32 = 220;
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Serialize, Clone, Debug)]
pub struct Commentary {
    pub track_key: String,
    pub text: String,
    pub source: String, // "cache" | "api" | "empty" | "error"
    pub error: Option<String>,
}

/// Cache of (track_key → commentary text). Lifetime = process lifetime.
pub type CommentaryCache = Arc<RwLock<HashMap<String, String>>>;

pub fn new_cache() -> CommentaryCache {
    Arc::new(RwLock::new(HashMap::new()))
}

#[tauri::command]
pub async fn get_track_commentary(
    app: AppHandle,
    title: String,
    artist: String,
    album: String,
) -> Result<Commentary, String> {
    let track_key = format!("{}|{}|{}", artist, title, album);
    if title.trim().is_empty() && artist.trim().is_empty() {
        return Ok(Commentary {
            track_key,
            text: String::new(),
            source: "empty".into(),
            error: None,
        });
    }

    let cache = app
        .try_state::<CommentaryCache>()
        .ok_or_else(|| "commentary cache not managed".to_string())?
        .inner()
        .clone();

    {
        let r = cache.read().await;
        if let Some(hit) = r.get(&track_key) {
            return Ok(Commentary {
                track_key: track_key.clone(),
                text: hit.clone(),
                source: "cache".into(),
                error: None,
            });
        }
    }

    let api_key = {
        let settings = app
            .try_state::<SharedSettings>()
            .ok_or_else(|| "settings not managed".to_string())?
            .inner()
            .clone();
        let s = settings.read().await;
        s.claude_api_key.trim().to_string()
    };
    if api_key.is_empty() {
        return Ok(Commentary {
            track_key,
            text: String::new(),
            source: "empty".into(),
            error: Some("no API key".into()),
        });
    }

    match fetch_commentary(&api_key, &title, &artist, &album).await {
        Ok(text) => {
            let trimmed = text.trim().to_string();
            cache.write().await.insert(track_key.clone(), trimmed.clone());
            Ok(Commentary {
                track_key,
                text: trimmed,
                source: "api".into(),
                error: None,
            })
        }
        Err(e) => {
            eprintln!("[commentary] fetch failed for '{title}' / '{artist}': {e:#}");
            Ok(Commentary {
                track_key,
                text: String::new(),
                source: "error".into(),
                error: Some(format!("{e:#}")),
            })
        }
    }
}

async fn fetch_commentary(
    api_key: &str,
    title: &str,
    artist: &str,
    album: &str,
) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("build reqwest client")?;

    let user_prompt = format!(
        "Write 2-3 short sentences about \"{title}\" by {artist}{album_part}. \
         Pretend you're a friend who knows music texting another friend who just heard the song. \
         Specific concrete facts beat general vibes: name the sample, the year, the producer, the place, the beef, the chart position, the cover origin — whatever's actually interesting and verifiable. \
         \n\nHARD BANS (do not use these phrases or patterns at all):\n\
         - Em-dashes (— or --). Use periods or commas instead.\n\
         - The words: essentially, basically, ultimately, really, truly, perhaps, arguably, underscores, delves, captures, explores, weaves, navigates, transcends, elevates, masterfully, beautifully, profoundly\n\
         - Sentence shapes like \"It's not just X, it's Y\" / \"X is a Y that Z\" / \"More than just X, it's Y\"\n\
         - Summarizing the lyrics back to the listener (assume they hear them right now)\n\
         - Vague attributions like \"many critics\", \"some say\", \"often considered\"\n\
         - Promotional language (\"iconic\", \"legendary\", \"timeless\", \"groundbreaking\", \"classic\")\n\
         - Triple lists (rule-of-three): \"X, Y, and Z\" structures used for emphasis\n\
         \nWrite plainly. Short sentences. Direct claims with specifics. Plain paragraph, no markdown, no bullets, no headers. \
         If you genuinely don't know the song, just write: \"No reliable info on this track.\" Don't guess.",
        album_part = if album.trim().is_empty() {
            String::new()
        } else {
            format!(" from the album \"{}\"", album)
        }
    );

    let body = serde_json::json!({
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "messages": [{
            "role": "user",
            "content": user_prompt,
        }]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("anthropic POST")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }

    let json: serde_json::Value = resp.json().await.context("parse JSON")?;
    let text = json
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow!("no content[0].text in response: {json}"))?
        .to_string();
    Ok(text)
}
