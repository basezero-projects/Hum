pub(crate) mod backend;
pub(crate) mod model;

pub(crate) use backend::{get_active_audio_output, get_audio_outputs};
#[cfg(windows)]
pub(crate) use backend::{
    shutdown_managed_runtime, AudioOutputBackend, AudioOutputBackendContext, AudioOutputPublisher,
    ManagedAudioOutputRuntime,
};
pub(crate) use model::new_shared_state;
#[cfg(windows)]
pub(crate) use model::SharedAudioOutputState;
