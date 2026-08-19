mod evaluate;
mod model;
mod policy;

pub use evaluate::evaluate_license;
pub use model::{
    LicenseCheck, LicenseEvidence, LicenseState, LicenseStatus, StoredLicenseEvidence,
};
pub use policy::LicensePolicy;
