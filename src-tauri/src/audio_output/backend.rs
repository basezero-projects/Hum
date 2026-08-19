#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::model::{AudioOutputDevice, AudioOutputState, SharedAudioOutputState};

pub(crate) const AUDIO_OUTPUTS_CHANGED_EVENT: &str = "audio-outputs-changed";
pub(crate) const ACTIVE_AUDIO_OUTPUT_CHANGED_EVENT: &str = "active-audio-output-changed";

pub(crate) struct AudioOutputBackendContext {
    pub publisher: AudioOutputPublisher,
}

pub(crate) trait AudioOutputBackend {
    fn start(self, context: AudioOutputBackendContext) -> Result<AudioOutputRuntime, String>;
}

pub(crate) trait AudioOutputSource {
    fn sample(&mut self) -> Result<AudioOutputState, String>;
}

#[derive(Clone)]
pub(crate) struct AudioOutputPublisher {
    events: Arc<dyn AudioOutputEventSink>,
    cache: SharedAudioOutputState,
}

impl AudioOutputPublisher {
    pub(crate) fn new(app: AppHandle, cache: SharedAudioOutputState) -> Self {
        Self::with_sink(Arc::new(TauriAudioOutputEventSink(app)), cache)
    }

    fn with_sink(events: Arc<dyn AudioOutputEventSink>, cache: SharedAudioOutputState) -> Self {
        Self { events, cache }
    }

    pub(crate) fn publish_sample(
        &self,
        sample: Result<AudioOutputState, String>,
    ) -> Result<Publication, String> {
        let sample = sample?;
        let active_id = sample.active.as_ref().map(|output| output.id.as_str());
        let next = AudioOutputState::normalized(sample.outputs, active_id);
        let previous = read_cache(&self.cache);
        let publication = Publication {
            outputs: previous.outputs != next.outputs,
            active: previous.active != next.active,
        };
        if publication == Publication::NONE {
            return Ok(publication);
        }

        write_cache(&self.cache, next.clone());
        if publication.outputs {
            self.events
                .emit_outputs(AUDIO_OUTPUTS_CHANGED_EVENT, &next.outputs);
        }
        if publication.active {
            self.events
                .emit_active(ACTIVE_AUDIO_OUTPUT_CHANGED_EVENT, &next.active);
        }
        Ok(publication)
    }
}

pub(crate) trait AudioOutputEventSink: Send + Sync {
    fn emit_outputs(&self, event: &'static str, outputs: &[AudioOutputDevice]);
    fn emit_active(&self, event: &'static str, active: &Option<AudioOutputDevice>);
}

struct TauriAudioOutputEventSink(AppHandle);

impl AudioOutputEventSink for TauriAudioOutputEventSink {
    fn emit_outputs(&self, event: &'static str, outputs: &[AudioOutputDevice]) {
        let _ = self.0.emit(event, outputs);
    }

    fn emit_active(&self, event: &'static str, active: &Option<AudioOutputDevice>) {
        let _ = self.0.emit(event, active);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Publication {
    outputs: bool,
    active: bool,
}

impl Publication {
    pub const NONE: Self = Self {
        outputs: false,
        active: false,
    };
    #[cfg(test)]
    pub const OUTPUTS: Self = Self {
        outputs: true,
        active: false,
    };
    #[cfg(test)]
    pub const ACTIVE: Self = Self {
        outputs: false,
        active: true,
    };
    #[cfg(test)]
    pub const BOTH: Self = Self {
        outputs: true,
        active: true,
    };
}

pub(crate) struct AudioOutputRuntime {
    stop: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

pub(crate) struct ManagedAudioOutputRuntime {
    runtime: Mutex<Option<AudioOutputRuntime>>,
}

impl ManagedAudioOutputRuntime {
    pub(crate) fn new(runtime: AudioOutputRuntime) -> Self {
        Self {
            runtime: Mutex::new(Some(runtime)),
        }
    }
}

pub(crate) fn shutdown_managed_runtime(runtime: &ManagedAudioOutputRuntime) -> bool {
    let runtime = runtime
        .runtime
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let was_running = runtime.is_some();
    drop(runtime);
    was_running
}

impl AudioOutputRuntime {
    pub(crate) fn from_parts(stop: Sender<()>, worker: JoinHandle<()>) -> Self {
        Self {
            stop: Some(stop),
            worker: Some(worker),
        }
    }
}

impl Drop for AudioOutputRuntime {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            if worker.thread().id() != std::thread::current().id() {
                let _ = worker.join();
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct ConsecutiveFailureLog {
    failure_active: bool,
}

impl ConsecutiveFailureLog {
    pub(crate) fn record_failure(&mut self) -> bool {
        let should_log = !self.failure_active;
        self.failure_active = true;
        should_log
    }

    pub(crate) fn record_success(&mut self) {
        self.failure_active = false;
    }
}

pub(crate) fn poll_until_stopped<S, F>(
    mut source: S,
    publisher: AudioOutputPublisher,
    stop: Receiver<()>,
    interval: Duration,
    mut log_failure: F,
) where
    S: AudioOutputSource,
    F: FnMut(&str),
{
    let mut failure_log = ConsecutiveFailureLog::default();
    loop {
        match publisher.publish_sample(source.sample()) {
            Ok(_) => failure_log.record_success(),
            Err(error) => {
                if failure_log.record_failure() {
                    log_failure(&error);
                }
            }
        }
        match stop.recv_timeout(interval) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn read_cache(cache: &SharedAudioOutputState) -> AudioOutputState {
    cache
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn write_cache(cache: &SharedAudioOutputState, value: AudioOutputState) {
    *cache
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
}

pub(crate) fn cached_outputs(cache: &SharedAudioOutputState) -> Vec<AudioOutputDevice> {
    read_cache(cache).outputs
}

pub(crate) fn cached_active_output(cache: &SharedAudioOutputState) -> Option<AudioOutputDevice> {
    read_cache(cache).active
}

#[tauri::command]
pub(crate) fn get_audio_outputs(
    state: tauri::State<'_, SharedAudioOutputState>,
) -> Vec<AudioOutputDevice> {
    cached_outputs(&state)
}

#[tauri::command]
pub(crate) fn get_active_audio_output(
    state: tauri::State<'_, SharedAudioOutputState>,
) -> Option<AudioOutputDevice> {
    cached_active_output(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_output::model::{
        AudioOutputDevice, AudioOutputRoute, AudioOutputState, SharedAudioOutputState,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, RwLock};
    use std::time::{Duration, Instant};

    fn device(id: &str, name: &str, route: AudioOutputRoute) -> AudioOutputDevice {
        AudioOutputDevice {
            id: id.into(),
            display_name: name.into(),
            route,
        }
    }

    fn state(outputs: Vec<AudioOutputDevice>, active_id: Option<&str>) -> AudioOutputState {
        AudioOutputState::normalized(outputs, active_id)
    }

    #[derive(Default)]
    struct RecordingSink {
        cache: Option<SharedAudioOutputState>,
        events: Mutex<Vec<(&'static str, serde_json::Value)>>,
    }

    impl AudioOutputEventSink for RecordingSink {
        fn emit_outputs(&self, event: &'static str, outputs: &[AudioOutputDevice]) {
            let cached = self.cache.as_ref().unwrap().read().unwrap().clone();
            assert_eq!(cached.outputs, outputs);
            self.events
                .lock()
                .unwrap()
                .push((event, serde_json::to_value(outputs).unwrap()));
        }

        fn emit_active(&self, event: &'static str, active: &Option<AudioOutputDevice>) {
            let cached = self.cache.as_ref().unwrap().read().unwrap().clone();
            assert_eq!(&cached.active, active);
            self.events
                .lock()
                .unwrap()
                .push((event, serde_json::to_value(active).unwrap()));
        }
    }

    fn publisher() -> (
        AudioOutputPublisher,
        SharedAudioOutputState,
        Arc<RecordingSink>,
    ) {
        let cache = Arc::new(RwLock::new(AudioOutputState::default()));
        let sink = Arc::new(RecordingSink {
            cache: Some(cache.clone()),
            ..RecordingSink::default()
        });
        let publisher = AudioOutputPublisher::with_sink(sink.clone(), cache.clone());
        (publisher, cache, sink)
    }

    #[test]
    fn cache_is_updated_before_both_events() {
        let (publisher, cache, sink) = publisher();
        let outputs = vec![device("a", "Headphones", AudioOutputRoute::Wired)];
        let next = state(outputs, Some("a"));

        let transition = publisher.publish_sample(Ok(next.clone())).unwrap();

        assert_eq!(cache.read().unwrap().clone(), next);
        assert_eq!(transition, Publication::BOTH);
        assert_eq!(
            sink.events.lock().unwrap().as_slice(),
            [
                (
                    AUDIO_OUTPUTS_CHANGED_EVENT,
                    serde_json::to_value(&next.outputs).unwrap()
                ),
                (
                    ACTIVE_AUDIO_OUTPUT_CHANGED_EVENT,
                    serde_json::to_value(&next.active).unwrap()
                )
            ]
        );
    }

    #[test]
    fn event_names_preserve_exact_contract() {
        assert_eq!(AUDIO_OUTPUTS_CHANGED_EVENT, "audio-outputs-changed");
        assert_eq!(
            ACTIVE_AUDIO_OUTPUT_CHANGED_EVENT,
            "active-audio-output-changed"
        );
    }

    #[test]
    fn identical_inventory_and_active_output_emit_nothing() {
        let (publisher, _, sink) = publisher();
        let next = state(
            vec![device("a", "Headphones", AudioOutputRoute::Wired)],
            Some("a"),
        );
        publisher.publish_sample(Ok(next.clone())).unwrap();
        sink.events.lock().unwrap().clear();

        assert_eq!(
            publisher.publish_sample(Ok(next)).unwrap(),
            Publication::NONE
        );
        assert!(sink.events.lock().unwrap().is_empty());
    }

    #[test]
    fn inventory_only_and_active_only_changes_emit_matching_events() {
        let (publisher, _, sink) = publisher();
        let headphones = device("a", "Headphones", AudioOutputRoute::Wired);
        let speakers = device("b", "Speakers", AudioOutputRoute::Speakers);
        publisher
            .publish_sample(Ok(state(vec![headphones.clone()], Some("a"))))
            .unwrap();
        sink.events.lock().unwrap().clear();

        assert_eq!(
            publisher
                .publish_sample(Ok(state(
                    vec![headphones.clone(), speakers.clone()],
                    Some("a"),
                )))
                .unwrap(),
            Publication::OUTPUTS
        );
        assert_eq!(
            sink.events.lock().unwrap()[0].0,
            AUDIO_OUTPUTS_CHANGED_EVENT
        );
        sink.events.lock().unwrap().clear();

        assert_eq!(
            publisher
                .publish_sample(Ok(state(vec![headphones, speakers], Some("b"))))
                .unwrap(),
            Publication::ACTIVE
        );
        assert_eq!(
            sink.events.lock().unwrap()[0].0,
            ACTIVE_AUDIO_OUTPUT_CHANGED_EVENT
        );
    }

    #[test]
    fn active_removal_emits_json_null_once() {
        let (publisher, _, sink) = publisher();
        let outputs = vec![device("a", "Headphones", AudioOutputRoute::Wired)];
        publisher
            .publish_sample(Ok(state(outputs.clone(), Some("a"))))
            .unwrap();
        sink.events.lock().unwrap().clear();

        assert_eq!(
            publisher
                .publish_sample(Ok(state(outputs.clone(), None)))
                .unwrap(),
            Publication::ACTIVE
        );
        assert_eq!(
            sink.events.lock().unwrap().as_slice(),
            [(ACTIVE_AUDIO_OUTPUT_CHANGED_EVENT, serde_json::Value::Null)]
        );
        sink.events.lock().unwrap().clear();
        assert_eq!(
            publisher.publish_sample(Ok(state(outputs, None))).unwrap(),
            Publication::NONE
        );
        assert!(sink.events.lock().unwrap().is_empty());
    }

    #[test]
    fn failed_sample_preserves_last_good_cache_and_emits_nothing() {
        let (publisher, cache, sink) = publisher();
        let good = state(
            vec![device("a", "Headphones", AudioOutputRoute::Wired)],
            Some("a"),
        );
        publisher.publish_sample(Ok(good.clone())).unwrap();
        sink.events.lock().unwrap().clear();

        assert_eq!(
            publisher
                .publish_sample(Err("enumeration failed".into()))
                .unwrap_err(),
            "enumeration failed"
        );
        assert_eq!(cache.read().unwrap().clone(), good);
        assert!(sink.events.lock().unwrap().is_empty());
    }

    #[test]
    fn runtime_drop_wakes_and_joins_without_waiting_for_poll_interval() {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_in_worker = stopped.clone();
        let worker = std::thread::spawn(move || {
            let _ = stop_rx.recv_timeout(Duration::from_secs(30));
            stopped_in_worker.store(true, Ordering::SeqCst);
        });
        let runtime = AudioOutputRuntime::from_parts(stop_tx, worker);

        let started = Instant::now();
        drop(runtime);

        assert!(stopped.load(Ordering::SeqCst));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn managed_shutdown_takes_and_drops_the_runtime_once() {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_in_worker = stopped.clone();
        let worker = std::thread::spawn(move || {
            let _ = stop_rx.recv_timeout(Duration::from_secs(30));
            stopped_in_worker.store(true, Ordering::SeqCst);
        });
        let managed =
            ManagedAudioOutputRuntime::new(AudioOutputRuntime::from_parts(stop_tx, worker));

        assert!(shutdown_managed_runtime(&managed));
        assert!(stopped.load(Ordering::SeqCst));
        assert!(managed.runtime.lock().unwrap().is_none());
        assert!(!shutdown_managed_runtime(&managed));
    }

    #[test]
    fn cached_commands_read_state_without_a_native_source() {
        let expected = state(
            vec![device("a", "Headphones", AudioOutputRoute::Wired)],
            Some("a"),
        );
        let cache = Arc::new(RwLock::new(expected.clone()));

        assert_eq!(cached_outputs(&cache), expected.outputs);
        assert_eq!(cached_active_output(&cache), expected.active);
    }

    #[test]
    fn consecutive_failure_log_guard_resets_after_recovery() {
        let mut guard = ConsecutiveFailureLog::default();

        assert!(guard.record_failure());
        assert!(!guard.record_failure());
        guard.record_success();
        assert!(guard.record_failure());
    }

    #[test]
    fn poll_loop_retries_failures_and_logs_once_per_failure_run() {
        use std::collections::VecDeque;

        struct ScriptedSource {
            samples: VecDeque<Result<AudioOutputState, String>>,
            stop: std::sync::mpsc::Sender<()>,
        }

        impl AudioOutputSource for ScriptedSource {
            fn sample(&mut self) -> Result<AudioOutputState, String> {
                let sample = self.samples.pop_front().expect("scripted sample");
                if self.samples.is_empty() {
                    let _ = self.stop.send(());
                }
                sample
            }
        }

        let (publisher, cache, _) = publisher();
        let good = state(
            vec![device("a", "Headphones", AudioOutputRoute::Wired)],
            Some("a"),
        );
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let source = ScriptedSource {
            samples: VecDeque::from([
                Err("first failure".into()),
                Err("repeated failure".into()),
                Ok(good.clone()),
                Err("failure after recovery".into()),
            ]),
            stop: stop_tx,
        };
        let mut logged = Vec::new();

        poll_until_stopped(source, publisher, stop_rx, Duration::ZERO, |error| {
            logged.push(error.to_string())
        });

        assert_eq!(cache.read().unwrap().clone(), good);
        assert_eq!(logged, ["first failure", "failure after recovery"]);
    }
}
