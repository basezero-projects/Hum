use std::fmt;

use super::record::StoredLicenseRecord;

pub trait LicenseStore: Send + Sync {
    fn load(&self) -> Result<Option<StoredLicenseRecord>, LicenseStoreError>;
    fn save(&self, record: &StoredLicenseRecord) -> Result<(), LicenseStoreError>;
    fn delete(&self) -> Result<(), LicenseStoreError>;
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum LicenseStoreError {
    Io,
    Protection,
    Corrupt,
    Unsupported,
}

impl fmt::Debug for LicenseStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Io => "LicenseStoreError::Io",
            Self::Protection => "LicenseStoreError::Protection",
            Self::Corrupt => "LicenseStoreError::Corrupt",
            Self::Unsupported => "LicenseStoreError::Unsupported",
        })
    }
}

impl fmt::Display for LicenseStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Io => "license storage is unavailable",
            Self::Protection => "license storage could not be protected",
            Self::Corrupt => "the protected license record is invalid",
            Self::Unsupported => "the protected license record is unsupported",
        })
    }
}

impl std::error::Error for LicenseStoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_errors_never_include_paths_or_secrets() {
        for error in [
            LicenseStoreError::Io,
            LicenseStoreError::Protection,
            LicenseStoreError::Corrupt,
            LicenseStoreError::Unsupported,
        ] {
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains("HUM-SECRET"));
            assert!(!rendered.contains("license.bin"));
            assert!(!rendered.contains(":\\"));
        }
    }
}
