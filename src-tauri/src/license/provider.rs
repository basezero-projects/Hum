use std::fmt;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Eq, PartialEq)]
pub struct ProviderActivation {
    pub activation_id: String,
    pub key_suffix: String,
}

impl fmt::Debug for ProviderActivation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderActivation")
            .field("activation_id", &"[redacted]")
            .field("key_suffix", &self.key_suffix)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderResult {
    Granted(ProviderActivation),
    Invalid,
    Revoked,
    DeviceLimit,
    ServiceUnavailable,
}

pub type ProviderFuture<'a> = Pin<Box<dyn Future<Output = ProviderResult> + Send + 'a>>;

pub trait LicenseProvider: Send + Sync {
    fn activate(&self, license_key: String) -> ProviderFuture<'_>;
    fn validate(&self, license_key: String, activation_id: String) -> ProviderFuture<'_>;
    fn deactivate(&self, license_key: String, activation_id: String) -> ProviderFuture<'_>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_debug_output_redacts_activation_identifiers() {
        let activation = ProviderActivation {
            activation_id: "activation-secret".into(),
            key_suffix: "ABCD1234".into(),
        };

        let rendered = format!(
            "{activation:?} {:?}",
            ProviderResult::Granted(activation.clone())
        );

        assert!(!rendered.contains("activation-secret"));
        assert!(rendered.contains("ABCD1234"));
    }
}
