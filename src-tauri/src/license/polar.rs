use std::time::Duration;

use rand::distr::{Alphanumeric, SampleString};
use reqwest::{Client, Request, StatusCode};
use serde_json::{json, Value};

use super::policy::LicensePolicy;
use super::provider::{LicenseProvider, ProviderActivation, ProviderFuture, ProviderResult};

const POLAR_BASE_URL: &str = "https://api.polar.sh/v1/customer-portal/license-keys";
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct PolarLicenseProvider {
    client: Client,
    organization_id: String,
    base_url: String,
    policy: LicensePolicy,
}

#[derive(Clone, Copy)]
enum Operation {
    Activate,
    Validate,
    Deactivate,
}

impl Operation {
    const fn path(self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Validate => "validate",
            Self::Deactivate => "deactivate",
        }
    }
}

impl PolarLicenseProvider {
    pub fn new(organization_id: impl Into<String>) -> Result<Self, &'static str> {
        Self::with_base_url(organization_id, POLAR_BASE_URL)
    }

    fn with_base_url(
        organization_id: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let organization_id = organization_id.into();
        if organization_id.trim().is_empty() {
            return Err("Polar organization ID is not configured");
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "Polar client could not be created")?;
        Ok(Self {
            client,
            organization_id,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            policy: LicensePolicy::default(),
        })
    }

    fn activation_label() -> String {
        let suffix = Alphanumeric.sample_string(&mut rand::rng(), 6);
        format!("Windows PC {suffix}")
    }

    fn build_request(
        &self,
        operation: Operation,
        license_key: &str,
        activation_id: Option<&str>,
        label: Option<&str>,
    ) -> Result<Request, ()> {
        let conditions = json!({ "major_version": self.policy.product_major_version });
        let body = match operation {
            Operation::Activate => json!({
                "key": license_key,
                "organization_id": self.organization_id,
                "label": label.unwrap_or("Windows PC"),
                "conditions": conditions,
            }),
            Operation::Validate => json!({
                "key": license_key,
                "organization_id": self.organization_id,
                "activation_id": activation_id.unwrap_or_default(),
                "conditions": conditions,
            }),
            Operation::Deactivate => json!({
                "key": license_key,
                "organization_id": self.organization_id,
                "activation_id": activation_id.unwrap_or_default(),
            }),
        };
        self.client
            .post(format!("{}/{}", self.base_url, operation.path()))
            .json(&body)
            .build()
            .map_err(|_| ())
    }

    async fn send(
        &self,
        operation: Operation,
        license_key: String,
        activation_id: Option<String>,
    ) -> ProviderResult {
        let label = matches!(operation, Operation::Activate).then(Self::activation_label);
        let request = match self.build_request(
            operation,
            &license_key,
            activation_id.as_deref(),
            label.as_deref(),
        ) {
            Ok(request) => request,
            Err(()) => return ProviderResult::ServiceUnavailable,
        };
        let response = match self.client.execute(request).await {
            Ok(response) => response,
            Err(_) => return ProviderResult::ServiceUnavailable,
        };
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return ProviderResult::ServiceUnavailable;
        }
        let body = match read_limited_body(response).await {
            Ok(body) => body,
            Err(()) => return ProviderResult::ServiceUnavailable,
        };
        self.map_response(
            operation,
            status,
            &body,
            &license_key,
            activation_id.as_deref(),
        )
    }

    fn map_response(
        &self,
        operation: Operation,
        http_status: StatusCode,
        body: &[u8],
        license_key: &str,
        activation_id: Option<&str>,
    ) -> ProviderResult {
        if http_status == StatusCode::TOO_MANY_REQUESTS || http_status.is_server_error() {
            return ProviderResult::ServiceUnavailable;
        }
        if matches!(operation, Operation::Deactivate) && http_status.is_success() {
            return ProviderResult::Granted(ProviderActivation {
                activation_id: activation_id.unwrap_or_default().to_string(),
                key_suffix: safe_key_suffix(license_key),
            });
        }
        if http_status.is_client_error() {
            let detail = String::from_utf8_lossy(body).to_ascii_lowercase();
            if detail.contains("activation limit") || detail.contains("activation_limit") {
                return ProviderResult::DeviceLimit;
            }
            return ProviderResult::Invalid;
        }
        if !http_status.is_success() {
            return ProviderResult::ServiceUnavailable;
        }
        let value: Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => return ProviderResult::ServiceUnavailable,
        };
        self.map_grant_payload(operation, &value, license_key, activation_id)
    }

    fn map_grant_payload(
        &self,
        operation: Operation,
        value: &Value,
        license_key: &str,
        activation_id: Option<&str>,
    ) -> ProviderResult {
        let license = value.get("license_key").unwrap_or(value);
        let status = license
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(status, "revoked" | "disabled") {
            return ProviderResult::Revoked;
        }
        if status != "granted" {
            return ProviderResult::Invalid;
        }
        let organization_matches = license
            .get("organization_id")
            .and_then(Value::as_str)
            .is_some_and(|organization| organization == self.organization_id);
        let activation_limit_matches = license
            .get("activation_limit")
            .and_then(Value::as_u64)
            .is_some_and(|limit| limit == u64::from(self.policy.device_limit));
        let major_version_matches = license
            .get("conditions")
            .and_then(|conditions| conditions.get("major_version"))
            .and_then(Value::as_u64)
            .is_some_and(|version| version == u64::from(self.policy.product_major_version));
        if !organization_matches || !activation_limit_matches || !major_version_matches {
            return ProviderResult::Invalid;
        }
        let provider_activation_id = match operation {
            Operation::Activate => value
                .get("id")
                .or_else(|| value.get("activation_id"))
                .and_then(Value::as_str),
            Operation::Validate => activation_id,
            Operation::Deactivate => activation_id,
        };
        let Some(provider_activation_id) = provider_activation_id else {
            return ProviderResult::Invalid;
        };
        if provider_activation_id.trim().is_empty() {
            return ProviderResult::Invalid;
        }
        ProviderResult::Granted(ProviderActivation {
            activation_id: provider_activation_id.to_string(),
            key_suffix: safe_key_suffix(license_key),
        })
    }
}

impl LicenseProvider for PolarLicenseProvider {
    fn activate(&self, license_key: String) -> ProviderFuture<'_> {
        Box::pin(self.send(Operation::Activate, license_key, None))
    }

    fn validate(&self, license_key: String, activation_id: String) -> ProviderFuture<'_> {
        Box::pin(self.send(Operation::Validate, license_key, Some(activation_id)))
    }

    fn deactivate(&self, license_key: String, activation_id: String) -> ProviderFuture<'_> {
        Box::pin(self.send(Operation::Deactivate, license_key, Some(activation_id)))
    }
}

async fn read_limited_body(mut response: reqwest::Response) -> Result<Vec<u8>, ()> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if response_size_is_oversized(body.len(), chunk.len()) {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn response_size_is_oversized(current_size: usize, next_chunk_size: usize) -> bool {
    current_size.saturating_add(next_chunk_size) > MAX_RESPONSE_BYTES
}

fn safe_key_suffix(license_key: &str) -> String {
    license_key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::AUTHORIZATION;

    const ORG: &str = "org_hum";
    const KEY: &str = "HUM-SECRET-ABCD1234";

    fn provider() -> PolarLicenseProvider {
        PolarLicenseProvider::with_base_url(ORG, "http://127.0.0.1:9/license-keys").unwrap()
    }

    fn granted_payload(status: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "id": "activation_123",
            "license_key": {
                "organization_id": ORG,
                "status": status,
                "activation_limit": 3,
                "conditions": { "major_version": 1 }
            }
        }))
        .unwrap()
    }

    #[test]
    fn requests_use_exact_public_paths_bodies_and_no_authorization() {
        let provider = provider();
        let activate = provider
            .build_request(Operation::Activate, KEY, None, Some("Windows PC ABC123"))
            .unwrap();
        let validate = provider
            .build_request(Operation::Validate, KEY, Some("activation_123"), None)
            .unwrap();
        let deactivate = provider
            .build_request(Operation::Deactivate, KEY, Some("activation_123"), None)
            .unwrap();

        assert_eq!(activate.url().path(), "/license-keys/activate");
        assert_eq!(validate.url().path(), "/license-keys/validate");
        assert_eq!(deactivate.url().path(), "/license-keys/deactivate");
        for request in [&activate, &validate, &deactivate] {
            assert_eq!(request.method(), reqwest::Method::POST);
            assert!(!request.headers().contains_key(AUTHORIZATION));
        }
        let body = |request: &Request| -> Value {
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap()
        };
        assert_eq!(body(&activate)["organization_id"], ORG);
        assert_eq!(body(&activate)["conditions"]["major_version"], 1);
        assert_eq!(body(&activate)["label"], "Windows PC ABC123");
        assert_eq!(body(&validate)["activation_id"], "activation_123");
        assert_eq!(body(&validate)["conditions"]["major_version"], 1);
        assert!(body(&deactivate).get("conditions").is_none());
    }

    #[test]
    fn activation_labels_are_generic_and_randomized() {
        let label = PolarLicenseProvider::activation_label();
        let suffix = label.strip_prefix("Windows PC ").unwrap();
        assert_eq!(suffix.len(), 6);
        assert!(suffix
            .chars()
            .all(|character| character.is_ascii_alphanumeric()));
    }

    #[test]
    fn successful_grants_require_exact_product_contract() {
        let provider = provider();
        let granted = provider.map_response(
            Operation::Activate,
            StatusCode::OK,
            &granted_payload("granted"),
            KEY,
            None,
        );
        assert_eq!(
            granted,
            ProviderResult::Granted(ProviderActivation {
                activation_id: "activation_123".into(),
                key_suffix: "ABCD1234".into(),
            })
        );

        for mutation in [
            json!({"id":"activation_123","license_key":{"organization_id":"wrong","status":"granted","activation_limit":3,"conditions":{"major_version":1}}}),
            json!({"id":"activation_123","license_key":{"organization_id":ORG,"status":"granted","activation_limit":4,"conditions":{"major_version":1}}}),
            json!({"id":"activation_123","license_key":{"organization_id":ORG,"status":"granted","activation_limit":3,"conditions":{"major_version":2}}}),
        ] {
            assert_eq!(
                provider.map_response(
                    Operation::Activate,
                    StatusCode::OK,
                    &serde_json::to_vec(&mutation).unwrap(),
                    KEY,
                    None,
                ),
                ProviderResult::Invalid
            );
        }
    }

    #[test]
    fn provider_status_and_http_failures_map_without_exposing_bodies() {
        let provider = provider();
        for status in ["revoked", "disabled"] {
            assert_eq!(
                provider.map_response(
                    Operation::Validate,
                    StatusCode::OK,
                    &granted_payload(status),
                    KEY,
                    Some("activation_123"),
                ),
                ProviderResult::Revoked
            );
        }
        assert_eq!(
            provider.map_response(
                Operation::Activate,
                StatusCode::UNPROCESSABLE_ENTITY,
                b"activation limit reached for HUM-SECRET",
                KEY,
                None,
            ),
            ProviderResult::DeviceLimit
        );
        assert_eq!(
            provider.map_response(
                Operation::Activate,
                StatusCode::BAD_REQUEST,
                b"invalid HUM-SECRET",
                KEY,
                None,
            ),
            ProviderResult::Invalid
        );
        for status in [StatusCode::TOO_MANY_REQUESTS, StatusCode::BAD_GATEWAY] {
            assert_eq!(
                provider.map_response(Operation::Activate, status, b"secret body", KEY, None),
                ProviderResult::ServiceUnavailable
            );
        }
        assert_eq!(
            provider.map_response(Operation::Activate, StatusCode::OK, b"not json", KEY, None),
            ProviderResult::ServiceUnavailable
        );
    }

    #[tokio::test]
    async fn network_errors_are_service_unavailable() {
        assert_eq!(
            provider().activate(KEY.into()).await,
            ProviderResult::ServiceUnavailable
        );
    }

    #[test]
    fn response_limit_accepts_64_kib_and_rejects_one_byte_more() {
        assert!(!response_size_is_oversized(0, MAX_RESPONSE_BYTES));
        assert!(!response_size_is_oversized(MAX_RESPONSE_BYTES - 1, 1));
        assert!(response_size_is_oversized(MAX_RESPONSE_BYTES, 1));
        assert!(response_size_is_oversized(usize::MAX, 1));
    }
}
