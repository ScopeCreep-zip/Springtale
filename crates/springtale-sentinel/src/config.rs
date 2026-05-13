use serde::Deserialize;

use specta::Type;
/// Sentinel configuration.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct SentinelConfig {
    /// Maximum actions per minute per connector. Default: 60.
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_minute: u32,

    /// Consecutive failures before circuit breaker opens. Default: 3.
    #[serde(default = "default_circuit_threshold")]
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker cooldown in seconds. Default: 300 (5 min).
    #[serde(default = "default_cooldown")]
    pub circuit_breaker_cooldown_secs: u64,

    /// Actions per minute without user interaction before dead-man triggers. Default: 120.
    #[serde(default = "default_dead_man")]
    pub dead_man_threshold: u32,

    /// Audit trail retention in days. Default: 90.
    #[serde(default = "default_retention")]
    pub audit_retention_days: u32,
}

fn default_rate_limit() -> u32 {
    60
}
fn default_circuit_threshold() -> u32 {
    3
}
fn default_cooldown() -> u64 {
    300
}
fn default_dead_man() -> u32 {
    120
}
fn default_retention() -> u32 {
    90
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            rate_limit_per_minute: default_rate_limit(),
            circuit_breaker_threshold: default_circuit_threshold(),
            circuit_breaker_cooldown_secs: default_cooldown(),
            dead_man_threshold: default_dead_man(),
            audit_retention_days: default_retention(),
        }
    }
}
