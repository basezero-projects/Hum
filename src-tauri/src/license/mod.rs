mod evaluate;
mod model;
mod polar;
mod policy;
mod provider;
pub(crate) mod record;
mod service;
mod store;

pub use evaluate::evaluate_license;
pub use model::{
    LicenseCheck, LicenseEvidence, LicenseState, LicenseStatus, StoredLicenseEvidence,
};
pub use polar::PolarLicenseProvider;
pub use policy::LicensePolicy;
pub use provider::{LicenseProvider, ProviderActivation, ProviderResult};
pub use service::{LicenseService, LicenseServiceError};
pub use store::{LicenseStore, LicenseStoreError};
