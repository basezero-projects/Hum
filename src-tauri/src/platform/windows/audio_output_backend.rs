use std::sync::mpsc;
use std::time::Duration;

use windows::core::PWSTR;
use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_EnumeratorName, PKEY_Device_FriendlyName,
};
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, DigitalAudioDisplayDevice, EndpointFormFactor, Headphones, Headset,
    IMMDevice, IMMDeviceEnumerator, LineLevel, MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor,
    Speakers, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantToStringAlloc, PropVariantToUInt32,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};

use crate::audio_output::backend::{
    poll_until_stopped, AudioOutputBackend, AudioOutputBackendContext, AudioOutputRuntime,
    AudioOutputSource,
};
use crate::audio_output::model::{AudioOutputDevice, AudioOutputRoute, AudioOutputState};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const UNKNOWN_OUTPUT_NAME: &str = "Unknown audio output";

pub(crate) struct WindowsAudioOutputBackend;

impl AudioOutputBackend for WindowsAudioOutputBackend {
    fn start(self, context: AudioOutputBackendContext) -> Result<AudioOutputRuntime, String> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("hum-audio-output".into())
            .spawn(move || {
                let _apartment = match ComApartment::initialize() {
                    Ok(apartment) => apartment,
                    Err(error) => {
                        eprintln!("[audio-output] COM initialization failed: {error}");
                        return;
                    }
                };
                poll_until_stopped(
                    WindowsAudioOutputSource::default(),
                    context.publisher,
                    stop_rx,
                    POLL_INTERVAL,
                    |error| eprintln!("[audio-output] poll failed: {error}"),
                );
            })
            .map_err(|error| format!("failed to start audio-output worker: {error}"))?;
        Ok(AudioOutputRuntime::from_parts(stop_tx, worker))
    }
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| error.to_string())?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Default)]
struct WindowsAudioOutputSource {
    enumerator: Option<IMMDeviceEnumerator>,
}

impl AudioOutputSource for WindowsAudioOutputSource {
    fn sample(&mut self) -> Result<AudioOutputState, String> {
        if self.enumerator.is_none() {
            self.enumerator = Some(unsafe {
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|error| format!("create endpoint enumerator failed: {error}"))?
            });
        }
        let result = sample_outputs(
            self.enumerator
                .as_ref()
                .expect("enumerator was initialized"),
        );
        if result.is_err() {
            self.enumerator = None;
        }
        result
    }
}

fn sample_outputs(enumerator: &IMMDeviceEnumerator) -> Result<AudioOutputState, String> {
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .map_err(|error| format!("enumerate active render endpoints failed: {error}"))?;
    let count = unsafe { collection.GetCount() }
        .map_err(|error| format!("read endpoint count failed: {error}"))?;
    let mut outputs = Vec::with_capacity(count as usize);
    for index in 0..count {
        let device = unsafe { collection.Item(index) }
            .map_err(|error| format!("read endpoint {index} failed: {error}"))?;
        if let Ok(output) = read_output_device(&device) {
            outputs.push(output);
        }
    }

    if outputs.is_empty() {
        return Ok(AudioOutputState::normalized(outputs, None));
    }
    let active = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
        .map_err(|error| format!("read default multimedia endpoint failed: {error}"))?;
    let active_id = read_device_id(&active)
        .map_err(|error| format!("read default endpoint ID failed: {error}"))?;
    Ok(AudioOutputState::normalized(outputs, Some(&active_id)))
}

fn read_output_device(device: &IMMDevice) -> Result<AudioOutputDevice, String> {
    let id = read_device_id(device)?;
    let properties = unsafe { device.OpenPropertyStore(STGM_READ) }.ok();
    let friendly_name = properties
        .as_ref()
        .and_then(|store| read_string_property(store, &PKEY_Device_FriendlyName));
    let enumerator_name = properties
        .as_ref()
        .and_then(|store| read_string_property(store, &PKEY_Device_EnumeratorName));
    let form_factor = properties.as_ref().and_then(read_form_factor);
    build_output_device(Ok(id), friendly_name, enumerator_name, form_factor)
}

fn build_output_device(
    id: Result<String, String>,
    friendly_name: Option<String>,
    enumerator_name: Option<String>,
    form_factor: Option<EndpointFormFactor>,
) -> Result<AudioOutputDevice, String> {
    let id = id?;
    let display_name = friendly_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(UNKNOWN_OUTPUT_NAME)
        .to_string();
    let route = classify_route(
        enumerator_name.as_deref(),
        friendly_name.as_deref(),
        form_factor,
    );

    Ok(AudioOutputDevice {
        id,
        display_name,
        route,
    })
}

fn read_device_id(device: &IMMDevice) -> Result<String, String> {
    let raw = unsafe { device.GetId() }.map_err(|error| error.to_string())?;
    pwstr_to_string_and_free(raw)
}

fn read_string_property(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let value = unsafe { store.GetValue(key) }.ok()?;
    let raw = unsafe { PropVariantToStringAlloc(&value) }.ok()?;
    pwstr_to_string_and_free(raw).ok()
}

fn read_form_factor(store: &IPropertyStore) -> Option<EndpointFormFactor> {
    let value = unsafe { store.GetValue(&PKEY_AudioEndpoint_FormFactor) }.ok()?;
    let value = unsafe { PropVariantToUInt32(&value) }.ok()?;
    Some(EndpointFormFactor(value as i32))
}

fn pwstr_to_string_and_free(raw: PWSTR) -> Result<String, String> {
    let value = unsafe { raw.to_string() }.map_err(|error| error.to_string());
    unsafe { CoTaskMemFree(Some(raw.0.cast())) };
    value
}

fn classify_route(
    enumerator_name: Option<&str>,
    friendly_name: Option<&str>,
    form_factor: Option<EndpointFormFactor>,
) -> AudioOutputRoute {
    if [enumerator_name, friendly_name]
        .into_iter()
        .flatten()
        .any(is_bluetooth_identifier)
    {
        return AudioOutputRoute::Bluetooth;
    }
    match form_factor {
        Some(value) if value == DigitalAudioDisplayDevice => AudioOutputRoute::Hdmi,
        Some(value) if value == Speakers => AudioOutputRoute::Speakers,
        Some(value) if [Headphones, Headset, LineLevel].contains(&value) => AudioOutputRoute::Wired,
        _ => AudioOutputRoute::Unknown,
    }
}

fn is_bluetooth_identifier(value: &str) -> bool {
    let value = value.trim();
    value.to_ascii_lowercase().contains("bluetooth")
        || value.eq_ignore_ascii_case("BTHENUM")
        || value.eq_ignore_ascii_case("BTHHFENUM")
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::Audio::{
        DigitalAudioDisplayDevice, Headphones, Headset, LineLevel, Speakers,
    };

    #[test]
    fn bluetooth_text_takes_precedence_over_form_factor() {
        assert_eq!(
            classify_route(Some("BTHENUM"), Some("Bluetooth Speakers"), Some(Speakers)),
            AudioOutputRoute::Bluetooth
        );
        assert_eq!(
            classify_route(
                Some("Bluetooth"),
                Some("HDMI"),
                Some(DigitalAudioDisplayDevice)
            ),
            AudioOutputRoute::Bluetooth
        );
        assert_eq!(
            classify_route(Some("BTHENUM"), Some("WH-1000XM5"), Some(Headphones)),
            AudioOutputRoute::Bluetooth
        );
        assert_eq!(
            classify_route(Some("BTHHFENUM"), Some("WH-1000XM5"), Some(Headset)),
            AudioOutputRoute::Bluetooth
        );
    }

    #[test]
    fn form_factor_classification_covers_hdmi_speakers_wired_and_unknown() {
        assert_eq!(
            classify_route(None, None, Some(DigitalAudioDisplayDevice)),
            AudioOutputRoute::Hdmi
        );
        assert_eq!(
            classify_route(None, None, Some(Speakers)),
            AudioOutputRoute::Speakers
        );
        for form in [Headphones, Headset, LineLevel] {
            assert_eq!(
                classify_route(None, None, Some(form)),
                AudioOutputRoute::Wired
            );
        }
        assert_eq!(classify_route(None, None, None), AudioOutputRoute::Unknown);
    }

    #[test]
    fn only_an_unreadable_endpoint_id_drops_a_device() {
        assert!(build_output_device(Err("missing ID".into()), None, None, None).is_err());

        let output = build_output_device(Ok("opaque-id".into()), None, None, None).unwrap();
        assert_eq!(output.id, "opaque-id");
        assert_eq!(output.display_name, UNKNOWN_OUTPUT_NAME);
        assert_eq!(output.route, AudioOutputRoute::Unknown);
    }
}
