use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LICENSE_RECORD_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Granted,
    Revoked,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredLicenseRecord {
    pub format_version: u16,
    pub license_key: String,
    pub activation_id: String,
    pub key_suffix: String,
    pub provider_status: ProviderStatus,
    pub verified_at_unix_ms: i64,
    pub last_seen_unix_ms: i64,
}

impl fmt::Debug for StoredLicenseRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredLicenseRecord")
            .field("format_version", &self.format_version)
            .field("license_key", &"[redacted]")
            .field("activation_id", &"[redacted]")
            .field("key_suffix", &self.key_suffix)
            .field("provider_status", &self.provider_status)
            .field("verified_at_unix_ms", &self.verified_at_unix_ms)
            .field("last_seen_unix_ms", &self.last_seen_unix_ms)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RecordError {
    #[error("unsupported license record version")]
    UnsupportedVersion,
    #[error("license key is missing")]
    BlankLicenseKey,
    #[error("activation identifier is missing")]
    BlankActivationId,
    #[error("license key suffix is invalid")]
    InvalidKeySuffix,
    #[error("license timestamps are invalid")]
    InvalidTimestamps,
    #[error("license record is corrupt")]
    Corrupt,
}

impl StoredLicenseRecord {
    pub fn new(
        license_key: String,
        activation_id: String,
        key_suffix: String,
        provider_status: ProviderStatus,
        verified_at_unix_ms: i64,
        last_seen_unix_ms: i64,
    ) -> Result<Self, RecordError> {
        let record = Self {
            format_version: LICENSE_RECORD_VERSION,
            license_key,
            activation_id,
            key_suffix,
            provider_status,
            verified_at_unix_ms,
            last_seen_unix_ms,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RecordError> {
        let record: Self = serde_json::from_slice(bytes).map_err(|_| RecordError::Corrupt)?;
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), RecordError> {
        if self.format_version != LICENSE_RECORD_VERSION {
            return Err(RecordError::UnsupportedVersion);
        }
        if self.license_key.trim().is_empty() {
            return Err(RecordError::BlankLicenseKey);
        }
        if self.activation_id.trim().is_empty() {
            return Err(RecordError::BlankActivationId);
        }
        if self.key_suffix.is_empty()
            || self.key_suffix.len() > 8
            || !self
                .key_suffix
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(RecordError::InvalidKeySuffix);
        }
        if self.verified_at_unix_ms < 0
            || self.last_seen_unix_ms < 0
            || self.last_seen_unix_ms < self.verified_at_unix_ms
        {
            return Err(RecordError::InvalidTimestamps);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "HUM-SECRET-ABCD1234";
    const ACTIVATION: &str = "activation-secret";

    fn valid_record() -> StoredLicenseRecord {
        StoredLicenseRecord::new(
            KEY.into(),
            ACTIVATION.into(),
            "ABCD1234".into(),
            ProviderStatus::Granted,
            1_700_000_000_000,
            1_700_000_000_000,
        )
        .unwrap()
    }

    #[test]
    fn record_v1_round_trips() {
        let record = valid_record();
        let encoded = serde_json::to_vec(&record).unwrap();
        let decoded = StoredLicenseRecord::decode(&encoded).unwrap();

        assert_eq!(decoded, record);
        assert_eq!(decoded.format_version, 1);
    }

    #[test]
    fn validation_rejects_every_invalid_invariant() {
        let base = valid_record();
        for invalid in [
            StoredLicenseRecord {
                format_version: 2,
                ..base.clone()
            },
            StoredLicenseRecord {
                license_key: " ".into(),
                ..base.clone()
            },
            StoredLicenseRecord {
                activation_id: "".into(),
                ..base.clone()
            },
            StoredLicenseRecord {
                key_suffix: "123456789".into(),
                ..base.clone()
            },
            StoredLicenseRecord {
                key_suffix: "BAD/KEY".into(),
                ..base.clone()
            },
            StoredLicenseRecord {
                verified_at_unix_ms: -1,
                ..base.clone()
            },
            StoredLicenseRecord {
                last_seen_unix_ms: -1,
                ..base.clone()
            },
            StoredLicenseRecord {
                verified_at_unix_ms: 10,
                last_seen_unix_ms: 9,
                ..base.clone()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn debug_and_errors_redact_secret_material() {
        let record = valid_record();
        let rendered = format!("{record:?} {:?}", RecordError::BlankLicenseKey);

        assert!(!rendered.contains(KEY));
        assert!(!rendered.contains(ACTIVATION));
        assert!(rendered.contains("ABCD1234"));
    }
}
