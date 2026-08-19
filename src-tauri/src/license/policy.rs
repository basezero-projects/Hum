pub const DAY_MS: i64 = 86_400_000;
pub const MINUTE_MS: i64 = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LicensePolicy {
    pub product_major_version: u16,
    pub device_limit: u8,
    pub verification_interval_days: u16,
    pub offline_grace_days: u16,
    pub warning_days: u8,
    pub clock_rollback_tolerance_minutes: u8,
    pub refund_window_days: u8,
}

impl Default for LicensePolicy {
    fn default() -> Self {
        Self {
            product_major_version: 1,
            device_limit: 3,
            verification_interval_days: 30,
            offline_grace_days: 30,
            warning_days: 7,
            clock_rollback_tolerance_minutes: 5,
            refund_window_days: 30,
        }
    }
}

impl LicensePolicy {
    pub fn verification_interval_ms(self) -> i64 {
        i64::from(self.verification_interval_days).saturating_mul(DAY_MS)
    }

    pub fn offline_grace_ms(self) -> i64 {
        i64::from(self.offline_grace_days).saturating_mul(DAY_MS)
    }

    pub fn warning_ms(self) -> i64 {
        i64::from(self.warning_days).saturating_mul(DAY_MS)
    }

    pub fn clock_rollback_tolerance_ms(self) -> i64 {
        i64::from(self.clock_rollback_tolerance_minutes).saturating_mul(MINUTE_MS)
    }
}
