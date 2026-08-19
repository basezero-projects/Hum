use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

use crate::license::record::{RecordError, StoredLicenseRecord};
use crate::license::{LicenseStore, LicenseStoreError};

const HUM_ENTROPY: &[u8] = b"com.syvr.hum/license/v1";

pub(crate) struct WindowsLicenseStore {
    path: PathBuf,
}

impl WindowsLicenseStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl LicenseStore for WindowsLicenseStore {
    fn load(&self) -> Result<Option<StoredLicenseRecord>, LicenseStoreError> {
        let protected = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(LicenseStoreError::Io),
        };
        let mut plaintext =
            unprotect_bytes(&protected, HUM_ENTROPY).map_err(|_| LicenseStoreError::Protection)?;
        let decoded = StoredLicenseRecord::decode(&plaintext);
        plaintext.fill(0);
        decoded.map(Some).map_err(|error| match error {
            RecordError::UnsupportedVersion => LicenseStoreError::Unsupported,
            _ => LicenseStoreError::Corrupt,
        })
    }

    fn save(&self, record: &StoredLicenseRecord) -> Result<(), LicenseStoreError> {
        record.validate().map_err(|error| match error {
            RecordError::UnsupportedVersion => LicenseStoreError::Unsupported,
            _ => LicenseStoreError::Corrupt,
        })?;
        let mut plaintext = serde_json::to_vec(record).map_err(|_| LicenseStoreError::Corrupt)?;
        let protected_result = protect_bytes(&plaintext, HUM_ENTROPY);
        plaintext.fill(0);
        let protected = protected_result.map_err(|_| LicenseStoreError::Protection)?;
        atomic_write(&self.path, &protected)
    }

    fn delete(&self) -> Result<(), LicenseStoreError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(LicenseStoreError::Io),
        }
    }
}

fn protect_bytes(plaintext: &[u8], entropy: &[u8]) -> Result<Vec<u8>, ()> {
    crypt_bytes(plaintext, entropy, true)
}

fn unprotect_bytes(protected: &[u8], entropy: &[u8]) -> Result<Vec<u8>, ()> {
    crypt_bytes(protected, entropy, false)
}

fn crypt_bytes(input: &[u8], entropy: &[u8], protect: bool) -> Result<Vec<u8>, ()> {
    let input_length = u32::try_from(input.len()).map_err(|_| ())?;
    let entropy_length = u32::try_from(entropy.len()).map_err(|_| ())?;
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: input_length,
        pbData: input.as_ptr().cast_mut(),
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy_length,
        pbData: entropy.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        if protect {
            CryptProtectData(
                &input_blob,
                PCWSTR::null(),
                Some(&entropy_blob),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                None,
                Some(&entropy_blob),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if result.is_err() {
        return Err(());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(HLOCAL(output.pbData.cast()));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), LicenseStoreError> {
    let parent = path.parent().ok_or(LicenseStoreError::Io)?;
    fs::create_dir_all(parent).map_err(|_| LicenseStoreError::Io)?;
    let temporary = parent.join(format!(".license-{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| LicenseStoreError::Io)?;
        file.write_all(bytes).map_err(|_| LicenseStoreError::Io)?;
        file.flush().map_err(|_| LicenseStoreError::Io)?;
        file.sync_all().map_err(|_| LicenseStoreError::Io)?;

        let source = wide_path(&temporary);
        let destination = wide_path(path);
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|_| LicenseStoreError::Io)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::record::{ProviderStatus, StoredLicenseRecord};
    use crate::license::LicenseStore;

    fn record(key: &str, activation: &str) -> StoredLicenseRecord {
        StoredLicenseRecord::new(
            key.into(),
            activation.into(),
            "ABCD1234".into(),
            ProviderStatus::Granted,
            1_700_000_000_000,
            1_700_000_000_000,
        )
        .unwrap()
    }

    #[test]
    fn dpapi_round_trips_for_current_user_and_rejects_tampering() {
        let plaintext = b"HUM-SECRET-ABCD1234";
        let mut protected = protect_bytes(plaintext, HUM_ENTROPY).unwrap();
        assert_ne!(protected, plaintext);
        assert_eq!(unprotect_bytes(&protected, HUM_ENTROPY).unwrap(), plaintext);

        let middle = protected.len() / 2;
        protected[middle] ^= 0x5a;
        assert!(unprotect_bytes(&protected, HUM_ENTROPY).is_err());
        assert!(
            unprotect_bytes(&protect_bytes(plaintext, b"wrong").unwrap(), HUM_ENTROPY).is_err()
        );
    }

    #[test]
    fn store_handles_missing_save_replace_corrupt_and_delete() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("license.bin");
        let store = WindowsLicenseStore::new(path.clone());

        assert!(store.load().unwrap().is_none());
        let first = record("HUM-SECRET-ABCD1234", "activation-one");
        store.save(&first).unwrap();
        assert_eq!(store.load().unwrap(), Some(first));

        let second = record("HUM-OTHER-ABCD1234", "activation-two");
        store.save(&second).unwrap();
        assert_eq!(store.load().unwrap(), Some(second));

        std::fs::write(&path, b"not protected data").unwrap();
        assert_eq!(
            store.load(),
            Err(crate::license::LicenseStoreError::Protection)
        );

        std::fs::write(&path, protect_bytes(b"not json", HUM_ENTROPY).unwrap()).unwrap();
        assert_eq!(
            store.load(),
            Err(crate::license::LicenseStoreError::Corrupt)
        );

        let mut unsupported = record("HUM-OTHER-ABCD1234", "activation-two");
        unsupported.format_version = 2;
        let plaintext = serde_json::to_vec(&unsupported).unwrap();
        std::fs::write(&path, protect_bytes(&plaintext, HUM_ENTROPY).unwrap()).unwrap();
        assert_eq!(
            store.load(),
            Err(crate::license::LicenseStoreError::Unsupported)
        );

        store.delete().unwrap();
        store.delete().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
