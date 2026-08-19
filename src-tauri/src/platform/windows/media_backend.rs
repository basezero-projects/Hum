use tauri::Manager;

use crate::media::{MediaBackend, MediaBackendContext, MediaPublisher};

pub(crate) struct WindowsMediaBackend;

impl MediaBackend for WindowsMediaBackend {
    fn start(self, context: MediaBackendContext) {
        let MediaBackendContext {
            app,
            snapshot,
            album_art,
            lyrics,
            smtc_playing,
        } = context;
        let publisher = MediaPublisher::new(app.clone(), album_art);

        crate::smtc::start(
            app.clone(),
            snapshot.clone(),
            smtc_playing.clone(),
            publisher.clone(),
        );
        crate::itunes::start(snapshot.clone(), smtc_playing, publisher.clone());

        let shared_bridge: crate::web_bridge::SharedWebBridge =
            std::sync::Arc::new(tokio::sync::RwLock::new(None));
        app.manage(shared_bridge.clone());
        crate::web_bridge::start(
            app.clone(),
            snapshot.clone(),
            shared_bridge.clone(),
            publisher,
        );
        crate::lyrics::start(app, lyrics, snapshot, shared_bridge);
    }
}
