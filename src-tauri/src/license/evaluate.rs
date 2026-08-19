use super::model::{
    LicenseCheck, LicenseEvidence, LicenseState, LicenseStatus, StoredLicenseEvidence,
};
use super::policy::{LicensePolicy, DAY_MS};

pub fn evaluate_license(
    now_unix_ms: i64,
    evidence: LicenseEvidence,
    latest_check: LicenseCheck,
    policy: &LicensePolicy,
) -> LicenseState {
    if evidence == LicenseEvidence::Development {
        return state(LicenseStatus::Development, None, policy, None, None, None);
    }

    if let Some(status) = terminal_status(latest_check) {
        let stored = stored_evidence(&evidence);
        return state(
            status,
            stored.and_then(display_key),
            policy,
            stored.map(|record| record.verified_at_unix_ms),
            None,
            None,
        );
    }

    let LicenseEvidence::Stored(record) = evidence else {
        let status = if latest_check == LicenseCheck::ServiceUnavailable {
            LicenseStatus::ServiceUnavailable
        } else {
            LicenseStatus::Unlicensed
        };
        return state(status, None, policy, None, None, None);
    };

    if now_unix_ms.saturating_add(policy.clock_rollback_tolerance_ms()) < record.last_seen_unix_ms {
        return state(
            LicenseStatus::ClockError,
            display_key(&record),
            policy,
            Some(record.verified_at_unix_ms),
            None,
            None,
        );
    }

    let verified_at_unix_ms = if latest_check == LicenseCheck::Granted {
        now_unix_ms
    } else {
        record.verified_at_unix_ms
    };
    let verify_after_unix_ms =
        verified_at_unix_ms.saturating_add(policy.verification_interval_ms());
    let grace_ends_unix_ms = verify_after_unix_ms.saturating_add(policy.offline_grace_ms());
    let warning_starts_unix_ms = verify_after_unix_ms.saturating_sub(policy.warning_ms());

    let status = if latest_check == LicenseCheck::Granted {
        LicenseStatus::Verified
    } else if now_unix_ms >= grace_ends_unix_ms {
        LicenseStatus::VerificationRequired
    } else if now_unix_ms >= verify_after_unix_ms
        && latest_check == LicenseCheck::ServiceUnavailable
    {
        LicenseStatus::OfflineGrace
    } else if now_unix_ms >= warning_starts_unix_ms {
        LicenseStatus::VerificationDue
    } else {
        LicenseStatus::Verified
    };

    let action_at = match status {
        LicenseStatus::Verified => Some(verify_after_unix_ms),
        LicenseStatus::VerificationDue if now_unix_ms < verify_after_unix_ms => {
            Some(verify_after_unix_ms)
        }
        LicenseStatus::VerificationDue => Some(grace_ends_unix_ms),
        LicenseStatus::OfflineGrace | LicenseStatus::VerificationRequired => {
            Some(grace_ends_unix_ms)
        }
        _ => None,
    };

    let mut result = state(
        status,
        display_key(&record),
        policy,
        Some(verified_at_unix_ms),
        Some(verify_after_unix_ms),
        Some(grace_ends_unix_ms),
    );
    result.days_until_action = action_at.map(|at| days_remaining(now_unix_ms, at));
    result
}

fn terminal_status(check: LicenseCheck) -> Option<LicenseStatus> {
    match check {
        LicenseCheck::Invalid => Some(LicenseStatus::Invalid),
        LicenseCheck::Revoked => Some(LicenseStatus::Revoked),
        LicenseCheck::DeviceLimit => Some(LicenseStatus::DeviceLimit),
        LicenseCheck::NotAttempted | LicenseCheck::Granted | LicenseCheck::ServiceUnavailable => {
            None
        }
    }
}

fn stored_evidence(evidence: &LicenseEvidence) -> Option<&StoredLicenseEvidence> {
    match evidence {
        LicenseEvidence::Stored(record) => Some(record),
        LicenseEvidence::Development | LicenseEvidence::Missing => None,
    }
}

fn display_key(record: &StoredLicenseEvidence) -> Option<String> {
    let safe = record
        .key_suffix
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    let suffix = safe
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    (!suffix.is_empty()).then(|| format!("HUM-****-{suffix}"))
}

fn days_remaining(now_unix_ms: i64, action_at_unix_ms: i64) -> u32 {
    let remaining = action_at_unix_ms.saturating_sub(now_unix_ms);
    if remaining <= 0 {
        return 0;
    }
    let days = remaining.saturating_add(DAY_MS - 1) / DAY_MS;
    u32::try_from(days).unwrap_or(u32::MAX)
}

fn state(
    status: LicenseStatus,
    display_key: Option<String>,
    policy: &LicensePolicy,
    verified_at_unix_ms: Option<i64>,
    verify_after_unix_ms: Option<i64>,
    grace_ends_unix_ms: Option<i64>,
) -> LicenseState {
    let (message, recovery) = copy_for(status);
    LicenseState {
        status,
        licensed: status.is_licensed(),
        display_key,
        device_limit: policy.device_limit,
        verified_at_unix_ms,
        verify_after_unix_ms,
        grace_ends_unix_ms,
        days_until_action: None,
        message: message.to_string(),
        recovery: recovery.to_string(),
    }
}

fn copy_for(status: LicenseStatus) -> (&'static str, &'static str) {
    match status {
        LicenseStatus::Development => (
            "Development entitlement active.",
            "This build does not use a customer activation.",
        ),
        LicenseStatus::Unlicensed => (
            "Hum needs a license before it can show lyrics.",
            "Buy Hum or enter the license key from your receipt.",
        ),
        LicenseStatus::Verified => ("License verified.", "No action is needed."),
        LicenseStatus::VerificationDue => (
            "Hum will need to verify this license soon.",
            "Connect this PC to the internet before the verification date.",
        ),
        LicenseStatus::OfflineGrace => (
            "Hum could not reach the license service, but your license still works offline.",
            "Reconnect before the offline grace period ends and Hum will verify automatically.",
        ),
        LicenseStatus::VerificationRequired => (
            "Hum needs to verify this license before lyrics can continue.",
            "Connect to the internet and try again. Your purchase has not expired.",
        ),
        LicenseStatus::Invalid => (
            "This license key is not valid for Hum.",
            "Check the key against your receipt, then try again.",
        ),
        LicenseStatus::Revoked => (
            "This license is no longer active.",
            "Open your purchase receipt or contact Hum support for help.",
        ),
        LicenseStatus::DeviceLimit => (
            "This license is already active on three devices.",
            "Open the customer portal and release a device, then try again.",
        ),
        LicenseStatus::ClockError => (
            "Hum cannot verify the license because the system clock moved backward.",
            "Set the Windows date and time correctly, then try again.",
        ),
        LicenseStatus::ServiceUnavailable => (
            "Hum could not reach the license service.",
            "Check your connection and try again. No activation was used.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BASE: i64 = 1_700_000_000_000;

    fn stored(verified_at_unix_ms: i64, last_seen_unix_ms: i64) -> LicenseEvidence {
        LicenseEvidence::Stored(StoredLicenseEvidence {
            key_suffix: "12345678".into(),
            verified_at_unix_ms,
            last_seen_unix_ms,
        })
    }

    fn evaluate(now: i64, evidence: LicenseEvidence, check: LicenseCheck) -> LicenseState {
        evaluate_license(now, evidence, check, &LicensePolicy::default())
    }

    #[test]
    fn accepted_policy_is_exact() {
        let policy = LicensePolicy::default();

        assert_eq!(policy.product_major_version, 1);
        assert_eq!(policy.device_limit, 3);
        assert_eq!(policy.verification_interval_days, 30);
        assert_eq!(policy.offline_grace_days, 30);
        assert_eq!(policy.warning_days, 7);
        assert_eq!(policy.clock_rollback_tolerance_minutes, 5);
        assert_eq!(policy.refund_window_days, 30);
    }

    #[test]
    fn statuses_preserve_exact_wire_values_and_license_flags() {
        for (status, wire, licensed) in [
            (LicenseStatus::Development, "development", true),
            (LicenseStatus::Unlicensed, "unlicensed", false),
            (LicenseStatus::Verified, "verified", true),
            (LicenseStatus::VerificationDue, "verification_due", true),
            (LicenseStatus::OfflineGrace, "offline_grace", true),
            (
                LicenseStatus::VerificationRequired,
                "verification_required",
                false,
            ),
            (LicenseStatus::Invalid, "invalid", false),
            (LicenseStatus::Revoked, "revoked", false),
            (LicenseStatus::DeviceLimit, "device_limit", false),
            (LicenseStatus::ClockError, "clock_error", false),
            (
                LicenseStatus::ServiceUnavailable,
                "service_unavailable",
                false,
            ),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), wire);
            assert_eq!(status.is_licensed(), licensed);
        }
    }

    #[test]
    fn development_entitlement_is_licensed() {
        let state = evaluate(
            BASE,
            LicenseEvidence::Development,
            LicenseCheck::NotAttempted,
        );

        assert_eq!(state.status, LicenseStatus::Development);
        assert!(state.licensed);
    }

    #[test]
    fn missing_entitlement_and_first_use_service_failure_are_distinct() {
        let unlicensed = evaluate(BASE, LicenseEvidence::Missing, LicenseCheck::NotAttempted);
        let unavailable = evaluate(
            BASE,
            LicenseEvidence::Missing,
            LicenseCheck::ServiceUnavailable,
        );

        assert_eq!(unlicensed.status, LicenseStatus::Unlicensed);
        assert_eq!(unavailable.status, LicenseStatus::ServiceUnavailable);
        assert!(!unlicensed.licensed);
        assert!(!unavailable.licensed);
    }

    #[test]
    fn verified_record_is_licensed_and_payload_is_safe() {
        let state = evaluate(BASE, stored(BASE, BASE), LicenseCheck::Granted);
        let value = serde_json::to_value(&state).unwrap();
        let json = serde_json::to_string(&state).unwrap();

        assert_eq!(state.status, LicenseStatus::Verified);
        assert!(state.licensed);
        assert_eq!(state.display_key.as_deref(), Some("HUM-****-12345678"));
        assert_eq!(
            value,
            json!({
                "status": "verified",
                "licensed": true,
                "display_key": "HUM-****-12345678",
                "device_limit": 3,
                "verified_at_unix_ms": BASE,
                "verify_after_unix_ms": BASE + 30 * DAY_MS,
                "grace_ends_unix_ms": BASE + 60 * DAY_MS,
                "days_until_action": 30,
                "message": "License verified.",
                "recovery": "No action is needed."
            })
        );
        for secret_field in ["license_key", "activation_id", "customer", "order_id"] {
            assert!(!json.contains(secret_field));
        }
    }

    #[test]
    fn display_key_keeps_only_the_last_eight_safe_characters() {
        let LicenseEvidence::Stored(mut record) = stored(BASE, BASE) else {
            unreachable!();
        };
        record.key_suffix = "full-secret-key-ABCD1234".into();
        let state = evaluate(BASE, LicenseEvidence::Stored(record), LicenseCheck::Granted);

        assert_eq!(state.display_key.as_deref(), Some("HUM-****-ABCD1234"));
        assert!(!serde_json::to_string(&state)
            .unwrap()
            .contains("full-secret"));
    }

    #[test]
    fn warning_verification_and_grace_boundaries_are_exact() {
        let warning_starts = BASE + 23 * DAY_MS;
        let verify_after = BASE + 30 * DAY_MS;
        let grace_ends = BASE + 60 * DAY_MS;

        assert_eq!(
            evaluate(
                warning_starts - 1,
                stored(BASE, warning_starts - 1),
                LicenseCheck::NotAttempted,
            )
            .status,
            LicenseStatus::Verified
        );
        assert_eq!(
            evaluate(
                warning_starts,
                stored(BASE, warning_starts),
                LicenseCheck::NotAttempted,
            )
            .status,
            LicenseStatus::VerificationDue
        );
        assert_eq!(
            evaluate(
                verify_after,
                stored(BASE, verify_after),
                LicenseCheck::ServiceUnavailable,
            )
            .status,
            LicenseStatus::OfflineGrace
        );
        assert_eq!(
            evaluate(
                grace_ends - 1,
                stored(BASE, grace_ends - 1),
                LicenseCheck::ServiceUnavailable,
            )
            .status,
            LicenseStatus::OfflineGrace
        );
        assert_eq!(
            evaluate(
                grace_ends,
                stored(BASE, grace_ends),
                LicenseCheck::ServiceUnavailable,
            )
            .status,
            LicenseStatus::VerificationRequired
        );
    }

    #[test]
    fn successful_check_starts_a_fresh_verification_window() {
        let now = BASE + 75 * DAY_MS;
        let state = evaluate(now, stored(BASE, now), LicenseCheck::Granted);

        assert_eq!(state.status, LicenseStatus::Verified);
        assert_eq!(state.verified_at_unix_ms, Some(now));
        assert_eq!(state.verify_after_unix_ms, Some(now + 30 * DAY_MS));
        assert_eq!(state.grace_ends_unix_ms, Some(now + 60 * DAY_MS));
    }

    #[test]
    fn overdue_check_counts_down_to_grace_end_while_still_licensed() {
        let verify_after = BASE + 30 * DAY_MS;
        let state = evaluate(
            verify_after,
            stored(BASE, verify_after),
            LicenseCheck::NotAttempted,
        );

        assert_eq!(state.status, LicenseStatus::VerificationDue);
        assert!(state.licensed);
        assert_eq!(state.days_until_action, Some(30));
    }

    #[test]
    fn invalid_revoked_and_device_limit_outcomes_stay_distinct() {
        for (check, expected) in [
            (LicenseCheck::Invalid, LicenseStatus::Invalid),
            (LicenseCheck::Revoked, LicenseStatus::Revoked),
            (LicenseCheck::DeviceLimit, LicenseStatus::DeviceLimit),
        ] {
            let result = evaluate(BASE, stored(BASE, BASE), check);
            assert_eq!(result.status, expected);
            assert!(!result.licensed);
        }
    }

    #[test]
    fn clock_rollback_tolerates_five_minutes_but_not_more() {
        let tolerance = LicensePolicy::default().clock_rollback_tolerance_ms();
        let tolerated = evaluate(
            BASE,
            stored(BASE - DAY_MS, BASE + tolerance),
            LicenseCheck::NotAttempted,
        );
        let rejected = evaluate(
            BASE,
            stored(BASE - DAY_MS, BASE + tolerance + 1),
            LicenseCheck::NotAttempted,
        );

        assert_ne!(tolerated.status, LicenseStatus::ClockError);
        assert_eq!(rejected.status, LicenseStatus::ClockError);
        assert!(!rejected.licensed);
    }

    #[test]
    fn remaining_days_round_up_and_never_become_negative() {
        assert_eq!(days_remaining(BASE, BASE), 0);
        assert_eq!(days_remaining(BASE, BASE + 1), 1);
        assert_eq!(days_remaining(BASE, BASE + DAY_MS), 1);
        assert_eq!(days_remaining(BASE, BASE + DAY_MS + 1), 2);
        assert_eq!(days_remaining(BASE, BASE - 1), 0);
    }
}
