use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::Manager;
use tokio::sync::RwLock;

#[cfg(windows)]
mod smtc;

#[cfg(windows)]
mod itunes;

mod lyrics;

#[cfg(windows)]
use smtc::{CurrentTrack, SharedSnapshot};

#[cfg(not(windows))]
mod smtc {
    use serde::Serialize;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[derive(Clone, Serialize, Debug, Default)]
    pub struct CurrentTrack {}

    pub type SharedSnapshot = Arc<RwLock<CurrentTrack>>;
}

#[cfg(not(windows))]
use smtc::{CurrentTrack, SharedSnapshot};

use lyrics::{CurrentLyrics, SharedLyrics};

#[tauri::command]
async fn get_current_track(
    state: tauri::State<'_, SharedSnapshot>,
) -> Result<CurrentTrack, String> {
    let s = state.read().await;
    Ok(s.clone())
}

#[tauri::command]
async fn get_current_lyrics(
    state: tauri::State<'_, SharedLyrics>,
) -> Result<CurrentLyrics, String> {
    let s = state.read().await;
    Ok(s.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let snapshot: SharedSnapshot = Arc::new(RwLock::new(CurrentTrack::default()));
    let lyrics_state: SharedLyrics = Arc::new(RwLock::new(CurrentLyrics::default()));
    let smtc_active: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(snapshot)
        .manage(lyrics_state)
        .setup(move |app| {
            let snap = app.state::<SharedSnapshot>().inner().clone();
            let lyrics_shared = app.state::<SharedLyrics>().inner().clone();

            #[cfg(windows)]
            {
                smtc::start(app.handle().clone(), snap.clone(), smtc_active.clone());
                itunes::start(app.handle().clone(), snap.clone(), smtc_active.clone());
            }
            #[cfg(not(windows))]
            {
                let _ = &smtc_active;
            }

            lyrics::start(app.handle().clone(), lyrics_shared, snap);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_current_track, get_current_lyrics])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
