use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseStatus {
    Development,
    Unlicensed,
    Verified,
    VerificationDue,
    OfflineGrace,
    VerificationRequired,
    Invalid,
    Revoked,
    DeviceLimit,
    ClockError,
    ServiceUnavailable,
}

impl LicenseStatus {
    pub const fn is_licensed(self) -> bool {
        matches!(
            self,
            Self::Development | Self::Verified | Self::VerificationDue | Self::OfflineGrace
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LicenseEvidence {
    Development,
    Missing,
    Stored(StoredLicenseEvidence),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredLicenseEvidence {
    pub key_suffix: String,
    pub verified_at_unix_ms: i64,
    pub last_seen_unix_ms: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LicenseCheck {
    NotAttempted,
    Granted,
    Invalid,
    Revoked,
    DeviceLimit,
    ServiceUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LicenseState {
    pub status: LicenseStatus,
    pub licensed: bool,
    pub display_key: Option<String>,
    pub device_limit: u8,
    pub verified_at_unix_ms: Option<i64>,
    pub verify_after_unix_ms: Option<i64>,
    pub grace_ends_unix_ms: Option<i64>,
    pub days_until_action: Option<u32>,
    pub message: String,
    pub recovery: String,
}
