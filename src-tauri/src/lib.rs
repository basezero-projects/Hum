use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::Manager;
use tokio::sync::RwLock;

#[cfg(windows)]
mod smtc;

#[cfg(windows)]
mod itunes;

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

#[tauri::command]
async fn get_current_track(
    state: tauri::State<'_, SharedSnapshot>,
) -> Result<CurrentTrack, String> {
    let s = state.read().await;
    Ok(s.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let snapshot: SharedSnapshot = Arc::new(RwLock::new(CurrentTrack::default()));
    let smtc_active: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .manage(snapshot)
        .setup(move |app| {
            #[cfg(windows)]
            {
                let snap = app.state::<SharedSnapshot>().inner().clone();
                smtc::start(app.handle().clone(), snap.clone(), smtc_active.clone());
                itunes::start(app.handle().clone(), snap, smtc_active.clone());
            }
            #[cfg(not(windows))]
            {
                let _ = &smtc_active;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_current_track])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
