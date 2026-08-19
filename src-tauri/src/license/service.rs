use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use super::evaluate::evaluate_license;
use super::model::{
    LicenseCheck, LicenseEvidence, LicenseState, LicenseStatus, StoredLicenseEvidence,
};
use super::policy::LicensePolicy;
use super::provider::{LicenseProvider, ProviderResult};
use super::record::{ProviderStatus, StoredLicenseRecord};
use super::store::{LicenseStore, LicenseStoreError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceMode {
    Development,
    Release,
}

pub struct LicenseService {
    mode: ServiceMode,
    store: Arc<dyn LicenseStore>,
    provider: Arc<dyn LicenseProvider>,
    policy: LicensePolicy,
    state: Arc<RwLock<LicenseState>>,
    operation: Mutex<()>,
}

#[derive(Clone, Copy, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LicenseServiceError {
    #[error("license storage is unavailable")]
    Storage(#[from] LicenseStoreError),
}

impl LicenseService {
    pub fn development() -> Self {
        Self::new(
            ServiceMode::Development,
            Arc::new(UnavailableStore),
            Arc::new(UnavailableProvider),
        )
    }

    pub fn release(store: Arc<dyn LicenseStore>, provider: Arc<dyn LicenseProvider>) -> Self {
        Self::new(ServiceMode::Release, store, provider)
    }

    pub fn release_offline(store: Arc<dyn LicenseStore>) -> Self {
        Self::new(ServiceMode::Release, store, Arc::new(UnavailableProvider))
    }

    #[cfg(test)]
    fn development_with_dependencies(
        store: Arc<dyn LicenseStore>,
        provider: Arc<dyn LicenseProvider>,
    ) -> Self {
        Self::new(ServiceMode::Development, store, provider)
    }

    fn new(
        mode: ServiceMode,
        store: Arc<dyn LicenseStore>,
        provider: Arc<dyn LicenseProvider>,
    ) -> Self {
        let policy = LicensePolicy::default();
        let evidence = if mode == ServiceMode::Development {
            LicenseEvidence::Development
        } else {
            LicenseEvidence::Missing
        };
        let state = evaluate_license(0, evidence, LicenseCheck::NotAttempted, &policy);
        Self {
            mode,
            store,
            provider,
            policy,
            state: Arc::new(RwLock::new(state)),
            operation: Mutex::new(()),
        }
    }

    pub async fn state(&self) -> LicenseState {
        self.state.read().await.clone()
    }

    pub async fn bootstrap(&self, now_unix_ms: i64) -> Result<LicenseState, LicenseServiceError> {
        if self.mode == ServiceMode::Development {
            return self
                .publish(evaluate_license(
                    now_unix_ms,
                    LicenseEvidence::Development,
                    LicenseCheck::NotAttempted,
                    &self.policy,
                ))
                .await;
        }
        let _operation = self.operation.lock().await;

        let Some(mut record) = self.store.load()? else {
            return self
                .publish(evaluate_license(
                    now_unix_ms,
                    LicenseEvidence::Missing,
                    LicenseCheck::NotAttempted,
                    &self.policy,
                ))
                .await;
        };
        let evidence = evidence_from(&record);
        let initial_check = if record.provider_status == ProviderStatus::Revoked {
            LicenseCheck::Revoked
        } else {
            LicenseCheck::NotAttempted
        };
        let initial = evaluate_license(now_unix_ms, evidence, initial_check, &self.policy);
        if initial.status == LicenseStatus::ClockError {
            return self.publish(initial).await;
        }

        record.last_seen_unix_ms = now_unix_ms.max(record.last_seen_unix_ms);
        self.store.save(&record)?;
        let needs_validation = record.provider_status == ProviderStatus::Revoked
            || matches!(
                initial.status,
                LicenseStatus::VerificationDue
                    | LicenseStatus::OfflineGrace
                    | LicenseStatus::VerificationRequired
            );
        if !needs_validation {
            return self
                .publish(evaluate_license(
                    now_unix_ms,
                    evidence_from(&record),
                    LicenseCheck::NotAttempted,
                    &self.policy,
                ))
                .await;
        }

        let result = self
            .provider
            .validate(record.license_key.clone(), record.activation_id.clone())
            .await;
        self.apply_validation(now_unix_ms, record, result).await
    }

    pub async fn activate(
        &self,
        license_key: String,
        now_unix_ms: i64,
    ) -> Result<LicenseState, LicenseServiceError> {
        if self.mode == ServiceMode::Development {
            return self.bootstrap(now_unix_ms).await;
        }
        let _operation = self.operation.lock().await;
        let result = self.provider.activate(license_key.clone()).await;
        let ProviderResult::Granted(activation) = result else {
            let check = check_for(result);
            return self
                .publish(evaluate_license(
                    now_unix_ms,
                    LicenseEvidence::Missing,
                    check,
                    &self.policy,
                ))
                .await;
        };
        let activation_id = activation.activation_id;
        let record = match StoredLicenseRecord::new(
            license_key.clone(),
            activation_id.clone(),
            activation.key_suffix,
            ProviderStatus::Granted,
            now_unix_ms,
            now_unix_ms,
        ) {
            Ok(record) => record,
            Err(_) => {
                let _ = self.provider.deactivate(license_key, activation_id).await;
                return Err(LicenseServiceError::Storage(LicenseStoreError::Corrupt));
            }
        };
        if let Err(error) = self.store.save(&record) {
            let _ = self.provider.deactivate(license_key, activation_id).await;
            return Err(error.into());
        }
        self.publish(evaluate_license(
            now_unix_ms,
            evidence_from(&record),
            LicenseCheck::Granted,
            &self.policy,
        ))
        .await
    }

    pub async fn deactivate(&self) -> Result<LicenseState, LicenseServiceError> {
        if self.mode == ServiceMode::Development {
            return Ok(self.state().await);
        }
        let _operation = self.operation.lock().await;
        let Some(record) = self.store.load()? else {
            return self
                .publish(evaluate_license(
                    0,
                    LicenseEvidence::Missing,
                    LicenseCheck::NotAttempted,
                    &self.policy,
                ))
                .await;
        };
        match self
            .provider
            .deactivate(record.license_key, record.activation_id)
            .await
        {
            ProviderResult::Granted(_) => {
                self.store.delete()?;
                self.publish(evaluate_license(
                    0,
                    LicenseEvidence::Missing,
                    LicenseCheck::NotAttempted,
                    &self.policy,
                ))
                .await
            }
            _ => Ok(self.state().await),
        }
    }

    async fn apply_validation(
        &self,
        now_unix_ms: i64,
        mut record: StoredLicenseRecord,
        result: ProviderResult,
    ) -> Result<LicenseState, LicenseServiceError> {
        match result {
            ProviderResult::Granted(activation) => {
                record.activation_id = activation.activation_id;
                record.key_suffix = activation.key_suffix;
                record.provider_status = ProviderStatus::Granted;
                record.verified_at_unix_ms = now_unix_ms;
                record.last_seen_unix_ms = now_unix_ms;
                record
                    .validate()
                    .map_err(|_| LicenseServiceError::Storage(LicenseStoreError::Corrupt))?;
                self.store.save(&record)?;
                self.publish(evaluate_license(
                    now_unix_ms,
                    evidence_from(&record),
                    LicenseCheck::Granted,
                    &self.policy,
                ))
                .await
            }
            ProviderResult::Revoked => {
                record.provider_status = ProviderStatus::Revoked;
                self.store.save(&record)?;
                self.publish(evaluate_license(
                    now_unix_ms,
                    evidence_from(&record),
                    LicenseCheck::Revoked,
                    &self.policy,
                ))
                .await
            }
            ProviderResult::ServiceUnavailable
                if record.provider_status == ProviderStatus::Revoked =>
            {
                self.publish(evaluate_license(
                    now_unix_ms,
                    evidence_from(&record),
                    LicenseCheck::Revoked,
                    &self.policy,
                ))
                .await
            }
            other => {
                self.publish(evaluate_license(
                    now_unix_ms,
                    evidence_from(&record),
                    check_for(other),
                    &self.policy,
                ))
                .await
            }
        }
    }

    async fn publish(&self, state: LicenseState) -> Result<LicenseState, LicenseServiceError> {
        *self.state.write().await = state.clone();
        Ok(state)
    }
}

fn evidence_from(record: &StoredLicenseRecord) -> LicenseEvidence {
    LicenseEvidence::Stored(StoredLicenseEvidence {
        key_suffix: record.key_suffix.clone(),
        verified_at_unix_ms: record.verified_at_unix_ms,
        last_seen_unix_ms: record.last_seen_unix_ms,
    })
}

fn check_for(result: ProviderResult) -> LicenseCheck {
    match result {
        ProviderResult::Granted(_) => LicenseCheck::Granted,
        ProviderResult::Invalid => LicenseCheck::Invalid,
        ProviderResult::Revoked => LicenseCheck::Revoked,
        ProviderResult::DeviceLimit => LicenseCheck::DeviceLimit,
        ProviderResult::ServiceUnavailable => LicenseCheck::ServiceUnavailable,
    }
}

struct UnavailableStore;

impl LicenseStore for UnavailableStore {
    fn load(&self) -> Result<Option<StoredLicenseRecord>, LicenseStoreError> {
        Err(LicenseStoreError::Unsupported)
    }

    fn save(&self, _record: &StoredLicenseRecord) -> Result<(), LicenseStoreError> {
        Err(LicenseStoreError::Unsupported)
    }

    fn delete(&self) -> Result<(), LicenseStoreError> {
        Err(LicenseStoreError::Unsupported)
    }
}

struct UnavailableProvider;

impl LicenseProvider for UnavailableProvider {
    fn activate(&self, _license_key: String) -> super::provider::ProviderFuture<'_> {
        Box::pin(std::future::ready(ProviderResult::ServiceUnavailable))
    }

    fn validate(
        &self,
        _license_key: String,
        _activation_id: String,
    ) -> super::provider::ProviderFuture<'_> {
        Box::pin(std::future::ready(ProviderResult::ServiceUnavailable))
    }

    fn deactivate(
        &self,
        _license_key: String,
        _activation_id: String,
    ) -> super::provider::ProviderFuture<'_> {
        Box::pin(std::future::ready(ProviderResult::ServiceUnavailable))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::license::provider::{ProviderActivation, ProviderFuture};
    use crate::license::record::StoredLicenseRecord;

    const NOW: i64 = 1_700_000_000_000;
    const KEY: &str = "HUM-SECRET-ABCD1234";
    const ACTIVATION: &str = "activation_123";

    #[derive(Default)]
    struct FakeStore {
        record: Mutex<Option<StoredLicenseRecord>>,
        loads: Mutex<u32>,
        saves: Mutex<u32>,
        deletes: Mutex<u32>,
        fail_save: Mutex<bool>,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl LicenseStore for FakeStore {
        fn load(&self) -> Result<Option<StoredLicenseRecord>, LicenseStoreError> {
            *self.loads.lock().unwrap() += 1;
            Ok(self.record.lock().unwrap().clone())
        }

        fn save(&self, record: &StoredLicenseRecord) -> Result<(), LicenseStoreError> {
            *self.saves.lock().unwrap() += 1;
            if *self.fail_save.lock().unwrap() {
                return Err(LicenseStoreError::Io);
            }
            *self.record.lock().unwrap() = Some(record.clone());
            Ok(())
        }

        fn delete(&self) -> Result<(), LicenseStoreError> {
            self.events.lock().unwrap().push("delete");
            *self.deletes.lock().unwrap() += 1;
            *self.record.lock().unwrap() = None;
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeProvider {
        results: Mutex<VecDeque<ProviderResult>>,
        activate_calls: Mutex<u32>,
        validate_calls: Mutex<u32>,
        deactivate_calls: Mutex<u32>,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl FakeProvider {
        fn with(results: impl IntoIterator<Item = ProviderResult>) -> Self {
            Self {
                results: Mutex::new(results.into_iter().collect()),
                ..Self::default()
            }
        }

        fn next(&self) -> ProviderResult {
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(ProviderResult::ServiceUnavailable)
        }
    }

    impl LicenseProvider for FakeProvider {
        fn activate(&self, _license_key: String) -> ProviderFuture<'_> {
            *self.activate_calls.lock().unwrap() += 1;
            Box::pin(std::future::ready(self.next()))
        }

        fn validate(&self, _license_key: String, _activation_id: String) -> ProviderFuture<'_> {
            *self.validate_calls.lock().unwrap() += 1;
            let result = self.next();
            Box::pin(async move {
                tokio::task::yield_now().await;
                result
            })
        }

        fn deactivate(&self, _license_key: String, _activation_id: String) -> ProviderFuture<'_> {
            self.events.lock().unwrap().push("deactivate");
            *self.deactivate_calls.lock().unwrap() += 1;
            Box::pin(std::future::ready(self.next()))
        }
    }

    fn granted() -> ProviderResult {
        ProviderResult::Granted(ProviderActivation {
            activation_id: ACTIVATION.into(),
            key_suffix: "ABCD1234".into(),
        })
    }

    fn record(verified_at: i64, last_seen: i64) -> StoredLicenseRecord {
        StoredLicenseRecord::new(
            KEY.into(),
            ACTIVATION.into(),
            "ABCD1234".into(),
            ProviderStatus::Granted,
            verified_at,
            last_seen,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn development_bootstrap_touches_neither_store_nor_provider() {
        let store = Arc::new(FakeStore::default());
        let provider = Arc::new(FakeProvider::default());
        let service =
            LicenseService::development_with_dependencies(store.clone(), provider.clone());

        let state = service.bootstrap(NOW).await.unwrap();

        assert_eq!(state.status, LicenseStatus::Development);
        assert_eq!(*store.loads.lock().unwrap(), 0);
        assert_eq!(*provider.validate_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn verified_bootstrap_updates_last_seen_without_network() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider::default());
        let service = LicenseService::release(store.clone(), provider.clone());

        let state = service.bootstrap(NOW + 1_000).await.unwrap();

        assert_eq!(state.status, LicenseStatus::Verified);
        assert_eq!(*provider.validate_calls.lock().unwrap(), 0);
        assert_eq!(
            store
                .record
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .last_seen_unix_ms,
            NOW + 1_000
        );
    }

    #[tokio::test]
    async fn due_bootstrap_validates_and_saves_a_fresh_grant() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider::with([granted()]));
        let service = LicenseService::release(store.clone(), provider.clone());
        let due = NOW + 23 * crate::license::policy::DAY_MS;

        let state = service.bootstrap(due).await.unwrap();

        assert_eq!(state.status, LicenseStatus::Verified);
        assert_eq!(*provider.validate_calls.lock().unwrap(), 1);
        assert_eq!(
            store
                .record
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .verified_at_unix_ms,
            due
        );
    }

    #[tokio::test]
    async fn overlapping_bootstraps_share_one_validation_operation() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider::with([granted(), granted()]));
        let service = Arc::new(LicenseService::release(store, provider.clone()));
        let due = NOW + 23 * crate::license::policy::DAY_MS;

        let (first, second) = tokio::join!(service.bootstrap(due), service.bootstrap(due));

        assert_eq!(first.unwrap().status, LicenseStatus::Verified);
        assert_eq!(second.unwrap().status, LicenseStatus::Verified);
        assert_eq!(*provider.validate_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn grace_keeps_access_when_polar_is_unavailable() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider::with([ProviderResult::ServiceUnavailable]));
        let service = LicenseService::release(store, provider);

        let state = service
            .bootstrap(NOW + 31 * crate::license::policy::DAY_MS)
            .await
            .unwrap();

        assert_eq!(state.status, LicenseStatus::OfflineGrace);
        assert!(state.licensed);
    }

    #[tokio::test]
    async fn release_without_provider_configuration_fails_closed_without_crashing() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let service = LicenseService::release_offline(store);

        let state = service
            .bootstrap(NOW + 31 * crate::license::policy::DAY_MS)
            .await
            .unwrap();

        assert_eq!(state.status, LicenseStatus::OfflineGrace);
        assert!(state.licensed);
    }

    #[tokio::test]
    async fn revoked_validation_is_persisted_and_remains_revoked_offline() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider::with([
            ProviderResult::Revoked,
            ProviderResult::ServiceUnavailable,
        ]));
        let service = LicenseService::release(store.clone(), provider.clone());
        let due = NOW + 23 * crate::license::policy::DAY_MS;

        let first = service.bootstrap(due).await.unwrap();
        let second = service.bootstrap(due + 1).await.unwrap();

        assert_eq!(first.status, LicenseStatus::Revoked);
        assert_eq!(second.status, LicenseStatus::Revoked);
        assert_eq!(
            store
                .record
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .provider_status,
            ProviderStatus::Revoked
        );
        assert_eq!(*provider.validate_calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn revoked_record_can_be_restored_by_a_granted_validation() {
        let store = Arc::new(FakeStore::default());
        let mut revoked = record(NOW, NOW);
        revoked.provider_status = ProviderStatus::Revoked;
        *store.record.lock().unwrap() = Some(revoked);
        let provider = Arc::new(FakeProvider::with([granted()]));
        let service = LicenseService::release(store.clone(), provider);

        let state = service.bootstrap(NOW + 1).await.unwrap();

        assert_eq!(state.status, LicenseStatus::Verified);
        assert_eq!(
            store
                .record
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .provider_status,
            ProviderStatus::Granted
        );
    }

    #[tokio::test]
    async fn clock_error_stops_before_storage_mutation_or_network() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW + 6 * 60_000));
        let provider = Arc::new(FakeProvider::default());
        let service = LicenseService::release(store.clone(), provider.clone());

        let state = service.bootstrap(NOW).await.unwrap();

        assert_eq!(state.status, LicenseStatus::ClockError);
        assert_eq!(*store.saves.lock().unwrap(), 0);
        assert_eq!(*provider.validate_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn activation_failures_publish_distinct_recovery_states() {
        for (result, expected) in [
            (ProviderResult::Invalid, LicenseStatus::Invalid),
            (ProviderResult::Revoked, LicenseStatus::Revoked),
            (ProviderResult::DeviceLimit, LicenseStatus::DeviceLimit),
            (
                ProviderResult::ServiceUnavailable,
                LicenseStatus::ServiceUnavailable,
            ),
        ] {
            let store = Arc::new(FakeStore::default());
            let provider = Arc::new(FakeProvider::with([result]));
            let service = LicenseService::release(store.clone(), provider);

            let state = service.activate(KEY.into(), NOW).await.unwrap();

            assert_eq!(state.status, expected);
            assert!(store.record.lock().unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn activation_save_failure_rolls_back_remote_activation() {
        let store = Arc::new(FakeStore::default());
        *store.fail_save.lock().unwrap() = true;
        let provider = Arc::new(FakeProvider::with([granted(), granted()]));
        let service = LicenseService::release(store, provider.clone());

        assert!(service.activate(KEY.into(), NOW).await.is_err());
        assert_eq!(*provider.deactivate_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn malformed_grant_rolls_back_remote_activation() {
        let store = Arc::new(FakeStore::default());
        let malformed = ProviderResult::Granted(ProviderActivation {
            activation_id: ACTIVATION.into(),
            key_suffix: String::new(),
        });
        let provider = Arc::new(FakeProvider::with([malformed, granted()]));
        let service = LicenseService::release(store, provider.clone());

        assert!(service.activate(KEY.into(), NOW).await.is_err());
        assert_eq!(*provider.deactivate_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn remote_deactivation_failure_preserves_local_state_and_storage() {
        let store = Arc::new(FakeStore::default());
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider::with([ProviderResult::ServiceUnavailable]));
        let service = LicenseService::release(store.clone(), provider);
        service.bootstrap(NOW).await.unwrap();

        let state = service.deactivate().await.unwrap();

        assert_eq!(state.status, LicenseStatus::Verified);
        assert!(store.record.lock().unwrap().is_some());
        assert_eq!(*store.deletes.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn successful_deactivation_releases_remote_before_deleting_local() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::new(FakeStore {
            events: events.clone(),
            ..FakeStore::default()
        });
        *store.record.lock().unwrap() = Some(record(NOW, NOW));
        let provider = Arc::new(FakeProvider {
            events: events.clone(),
            ..FakeProvider::with([granted()])
        });
        let service = LicenseService::release(store.clone(), provider);
        service.bootstrap(NOW).await.unwrap();

        let state = service.deactivate().await.unwrap();

        assert_eq!(state.status, LicenseStatus::Unlicensed);
        assert!(store.record.lock().unwrap().is_none());
        assert_eq!(*store.deletes.lock().unwrap(), 1);
        assert_eq!(*events.lock().unwrap(), ["deactivate", "delete"]);
    }
}
