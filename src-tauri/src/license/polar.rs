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
            .user_agent(crate::USER_AGENT)
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
        // Sent because Polar accepts it and would be the natural place for a
        // major-version gate. It does NOT currently enforce it: a probe with
        // `major_version: 9` against a 1.x key returned 200, and the license
        // record Polar sends back contains no `conditions` field at all, so
        // there is nothing to verify on the way home either. Hum therefore
        // does not claim to enforce the boundary. See BUGS.md before relying
        // on it to protect a paid 2.0 upgrade.
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
            // Checked first and against the raw text, because the device limit
            // is a real answer about this key whatever shape it arrives in.
            let detail = String::from_utf8_lossy(body).to_ascii_lowercase();
            if detail.contains("activation limit") || detail.contains("activation_limit") {
                return ProviderResult::DeviceLimit;
            }
            // Polar answers its own 4xx with JSON, so a 4xx carrying anything
            // else was produced in front of the API and never reached it. A
            // Cloudflare block is the case seen in the wild: 403 with
            // "error code: 1010" and no JSON at all. Calling that an invalid
            // key blames the customer for an outage and sends them to support
            // with the one explanation that cannot be true.
            if serde_json::from_slice::<Value>(body).is_err() {
                return ProviderResult::ServiceUnavailable;
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
        // Polar calls this `limit_activations`. Hum read `activation_limit`
        // for its first three releases, a field the API has never sent, so
        // this comparison was false for every key in existence and no
        // customer could activate. Verified against the live API 2026-09-30.
        let activation_limit_matches = license
            .get("limit_activations")
            .and_then(Value::as_u64)
            .is_some_and(|limit| limit == u64::from(self.policy.device_limit));
        if !organization_matches || !activation_limit_matches {
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

    /// Shaped from a real `POST /customer-portal/license-keys/activate`
    /// response captured on 2026-09-30. The previous fixture invented
    /// `activation_limit` and a `conditions` object, neither of which Polar
    /// has ever returned, so the suite agreed with the bug instead of
    /// catching it. Keep this mirroring the live payload.
    fn granted_payload(status: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "id": "activation_123",
            "label": "Windows PC",
            "license_key_id": "c80ca9b9-e604-4ba2-bebe-51063936901b",
            "license_key": {
                "id": "c80ca9b9-e604-4ba2-bebe-51063936901b",
                "organization_id": ORG,
                "status": status,
                "limit_activations": 3,
                "limit_usage": null,
                "usage": 0,
                "validations": 1,
                "expires_at": null,
                "benefit_id": "a0f83688-b79a-4824-b978-b89f9df4e546"
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
    fn the_grant_contract_matches_the_field_names_polar_actually_sends() {
        // Regression for the bug that made activation impossible for every
        // customer: Hum required `activation_limit` and `conditions
        // .major_version`, and Polar sends neither. This payload is copied
        // from a real activate response, so it fails if the contract drifts
        // back to an invented shape.
        let provider = provider();
        let live = br#"{
            "id": "0a7e8359-4917-4535-afb8-2f9956b1c20c",
            "label": "Windows PC",
            "license_key": {
                "id": "c80ca9b9-e604-4ba2-bebe-51063936901b",
                "organization_id": "org_hum",
                "status": "granted",
                "limit_activations": 3,
                "limit_usage": null,
                "usage": 0,
                "validations": 5,
                "expires_at": null
            }
        }"#;
        assert_eq!(
            provider.map_response(Operation::Activate, StatusCode::OK, live, KEY, None),
            ProviderResult::Granted(ProviderActivation {
                activation_id: "0a7e8359-4917-4535-afb8-2f9956b1c20c".into(),
                key_suffix: "ABCD1234".into(),
            }),
            "a real Polar activation payload must be accepted"
        );

        // The fields Hum used to demand are absent from that payload. If
        // either is ever required again, the assertion above breaks.
        let parsed: Value = serde_json::from_slice(live).unwrap();
        let license = parsed.get("license_key").unwrap();
        assert!(license.get("activation_limit").is_none());
        assert!(license.get("conditions").is_none());
    }

    #[test]
    fn an_edge_block_is_an_outage_and_never_an_invalid_key() {
        let provider = provider();

        // Cloudflare fronts api.polar.sh. With no User-Agent it answers 403
        // and "error code: 1010" before Polar sees the request. Seen live on
        // 2026-09-30: a real Granted key was reported to the customer as
        // "This license key is not valid for Hum."
        for (status, body) in [
            (StatusCode::FORBIDDEN, &b"error code: 1010"[..]),
            (
                StatusCode::FORBIDDEN,
                &b"<!DOCTYPE html><html>Attention Required</html>"[..],
            ),
            (StatusCode::BAD_REQUEST, &b""[..]),
        ] {
            assert_eq!(
                provider.map_response(Operation::Activate, status, body, KEY, None),
                ProviderResult::ServiceUnavailable,
                "a 4xx with a non-JSON body did not come from Polar"
            );
        }

        // Polar's own errors are JSON and must still be believed. This is the
        // exact body the live API returns for a key that does not exist.
        assert_eq!(
            provider.map_response(
                Operation::Activate,
                StatusCode::NOT_FOUND,
                br#"{"error":"ResourceNotFound","detail":"Not found"}"#,
                KEY,
                None,
            ),
            ProviderResult::Invalid,
            "a genuine unknown key must still read as invalid"
        );

        // And the device-limit case keeps its own outcome.
        assert_eq!(
            provider.map_response(
                Operation::Activate,
                StatusCode::FORBIDDEN,
                br#"{"detail":"activation limit reached"}"#,
                KEY,
                None,
            ),
            ProviderResult::DeviceLimit
        );
    }

    #[test]
    fn every_request_carries_a_user_agent() {
        // reqwest sends no User-Agent unless the client is told to, which is
        // exactly what got the license service blocked at Cloudflare. This has
        // to go over a real socket: a client-level default header is applied
        // when the request is sent, not when it is built, so inspecting the
        // built Request would pass while the wire stayed empty.
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind probe listener");
        let port = listener.local_addr().unwrap().port();

        let seen = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut buffer = [0u8; 2048];
            let read = socket.read(&mut buffer).unwrap_or(0);
            let _ = socket.write_all(
                b"HTTP/1.1 503 Service Unavailable
Content-Length: 0

",
            );
            String::from_utf8_lossy(&buffer[..read]).to_string()
        });

        let provider =
            PolarLicenseProvider::with_base_url(ORG, format!("http://127.0.0.1:{port}")).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = runtime.block_on(provider.activate(KEY.to_string()));

        let request = seen.join().expect("probe thread");
        let agent = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("user-agent:"))
            .unwrap_or_default()
            .to_string();
        assert!(
            agent.to_ascii_lowercase().contains("hum/"),
            "the activation request reached the wire without a Hum User-Agent: {request:?}"
        );
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
            json!({"id":"activation_123","license_key":{"organization_id":"wrong","status":"granted","limit_activations":3}}),
            json!({"id":"activation_123","license_key":{"organization_id":ORG,"status":"granted","limit_activations":4}}),
            // A key issued with no device cap is not this product's key.
            json!({"id":"activation_123","license_key":{"organization_id":ORG,"status":"granted","limit_activations":null}}),
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
        // Body shape matters now. Polar reports its own errors as JSON, and a
        // 4xx without JSON is treated as an edge failure rather than a verdict
        // on the key (see an_edge_block_is_an_outage_and_never_an_invalid_key).
        // This fixture carries the key inside a realistic Polar error so the
        // no-leak assertion below still has something to catch.
        assert_eq!(
            provider.map_response(
                Operation::Activate,
                StatusCode::BAD_REQUEST,
                br#"{"error":"BadRequest","detail":"invalid HUM-SECRET"}"#,
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
