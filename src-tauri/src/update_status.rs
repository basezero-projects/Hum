use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpdatePhase {
    Idle,
    Checking,
    Current,
    Available,
    /// Waiting on a second click, because installing would cut off playback.
    Confirming,
    Downloading,
    Installing,
    Restarting,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpdateErrorStage {
    Check,
    Download,
    Install,
    Restart,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateStatus {
    pub(crate) phase: UpdatePhase,
    #[serde(default)]
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) progress: Option<u16>,
    #[serde(default)]
    pub(crate) retryable: bool,
    #[serde(default)]
    pub(crate) stage: Option<UpdateErrorStage>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TrayProjection {
    pub(crate) text: String,
    pub(crate) enabled: bool,
}

fn safe_version(version: Option<&str>) -> Option<&str> {
    let value = version?.trim();
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return None;
    }
    Some(value)
}

pub(crate) fn tray_projection(status: &UpdateStatus) -> TrayProjection {
    let version = safe_version(status.version.as_deref());
    let (text, enabled) = match status.phase {
        UpdatePhase::Idle => ("Check for updates".to_string(), true),
        UpdatePhase::Checking => ("Checking for updates...".to_string(), false),
        UpdatePhase::Current => ("Hum is up to date".to_string(), false),
        UpdatePhase::Available => (
            version
                .map(|value| format!("Install update v{value}"))
                .unwrap_or_else(|| "Update available".to_string()),
            true,
        ),
        UpdatePhase::Confirming => (
            version
                .map(|value| format!("Install v{value} and restart now"))
                .unwrap_or_else(|| "Install and restart now".to_string()),
            true,
        ),
        UpdatePhase::Downloading => (
            status
                .progress
                .map(|progress| format!("Downloading update: {}%", progress.min(100)))
                .unwrap_or_else(|| "Downloading update...".to_string()),
            false,
        ),
        UpdatePhase::Installing => ("Installing update...".to_string(), false),
        UpdatePhase::Restarting => ("Restarting Hum...".to_string(), false),
        UpdatePhase::Error if status.retryable => {
            let text = match status.stage {
                Some(UpdateErrorStage::Check) => "Retry update check".to_string(),
                Some(UpdateErrorStage::Restart) => "Retry restart".to_string(),
                Some(UpdateErrorStage::Download | UpdateErrorStage::Install) => version
                    .map(|value| format!("Retry update v{value}"))
                    .unwrap_or_else(|| "Retry update".to_string()),
                None => "Retry update".to_string(),
            };
            (text, true)
        }
        UpdatePhase::Error => ("Update needs attention".to_string(), false),
    };
    TrayProjection { text, enabled }
}

#[cfg(test)]
mod tests {
    use super::{tray_projection, UpdateErrorStage, UpdatePhase, UpdateStatus};

    fn status(phase: UpdatePhase) -> UpdateStatus {
        UpdateStatus {
            phase,
            version: None,
            progress: None,
            retryable: false,
            stage: None,
        }
    }

    #[test]
    fn every_phase_has_exact_tray_copy_and_actionability() {
        let cases = [
            (UpdatePhase::Idle, "Check for updates", true),
            (UpdatePhase::Checking, "Checking for updates...", false),
            (UpdatePhase::Current, "Hum is up to date", false),
            (UpdatePhase::Installing, "Installing update...", false),
            (UpdatePhase::Restarting, "Restarting Hum...", false),
        ];

        for (phase, text, enabled) in cases {
            let actual = tray_projection(&status(phase));
            assert_eq!(actual.text, text);
            assert_eq!(actual.enabled, enabled);
        }

        let available = tray_projection(&UpdateStatus {
            phase: UpdatePhase::Available,
            version: Some("1.2.3".into()),
            progress: None,
            retryable: true,
            stage: None,
        });
        assert_eq!(available.text, "Install update v1.2.3");
        assert!(available.enabled);

        // The confirm step names the consequence, not the action, and stays
        // clickable: the second click is the one that actually installs, so a
        // disabled item here would strand the user.
        let confirming = tray_projection(&UpdateStatus {
            phase: UpdatePhase::Confirming,
            version: Some("1.2.3".into()),
            progress: None,
            retryable: true,
            stage: None,
        });
        assert_eq!(confirming.text, "Install v1.2.3 and restart now");
        assert!(confirming.enabled);

        // A version that fails sanitizing still leaves an actionable item
        // rather than a dead menu row.
        let confirming_unnamed = tray_projection(&status(UpdatePhase::Confirming));
        assert_eq!(confirming_unnamed.text, "Install and restart now");
        assert!(confirming_unnamed.enabled);

        let downloading = tray_projection(&UpdateStatus {
            phase: UpdatePhase::Downloading,
            version: Some("1.2.3".into()),
            progress: Some(142),
            retryable: false,
            stage: None,
        });
        assert_eq!(downloading.text, "Downloading update: 100%");
        assert!(!downloading.enabled);

        let retry_check = tray_projection(&UpdateStatus {
            phase: UpdatePhase::Error,
            version: None,
            progress: None,
            retryable: true,
            stage: Some(UpdateErrorStage::Check),
        });
        assert_eq!(retry_check.text, "Retry update check");
        assert!(retry_check.enabled);

        let retry_install = tray_projection(&UpdateStatus {
            phase: UpdatePhase::Error,
            version: Some("1.2.3".into()),
            progress: None,
            retryable: true,
            stage: Some(UpdateErrorStage::Install),
        });
        assert_eq!(retry_install.text, "Retry update v1.2.3");
        assert!(retry_install.enabled);

        let retry_restart = tray_projection(&UpdateStatus {
            phase: UpdatePhase::Error,
            version: Some("1.2.3".into()),
            progress: None,
            retryable: true,
            stage: Some(UpdateErrorStage::Restart),
        });
        assert_eq!(retry_restart.text, "Retry restart");
        assert!(retry_restart.enabled);
    }

    #[test]
    fn hostile_or_oversized_versions_never_reach_the_tray() {
        for version in ["<script>alert(1)</script>", &"a".repeat(80)] {
            let actual = tray_projection(&UpdateStatus {
                phase: UpdatePhase::Available,
                version: Some(version.into()),
                progress: None,
                retryable: true,
                stage: None,
            });
            assert_eq!(actual.text, "Update available");
            assert!(actual.enabled);
        }
    }

    #[test]
    fn wire_contract_uses_snake_case_phases() {
        let parsed: UpdateStatus = serde_json::from_value(serde_json::json!({
            "phase": "downloading",
            "version": "1.2.3",
            "progress": 20,
            "retryable": false,
            "stage": null
        }))
        .unwrap();
        assert_eq!(parsed.phase, UpdatePhase::Downloading);
        assert_eq!(parsed.progress, Some(20));

        // The frontend sends this one whenever a restart would interrupt
        // playback, so a rename on either side has to break a test.
        let confirming: UpdateStatus = serde_json::from_value(serde_json::json!({
            "phase": "confirming",
            "version": "1.2.3",
            "progress": null,
            "retryable": true,
            "stage": null
        }))
        .unwrap();
        assert_eq!(confirming.phase, UpdatePhase::Confirming);
    }
}
