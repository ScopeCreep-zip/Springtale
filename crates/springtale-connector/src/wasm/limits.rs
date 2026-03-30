/// Resource limits for the WASM sandbox.
///
/// These limits prevent community connectors from consuming excessive
/// resources. They are applied per-connector-invocation.
#[derive(Debug, Clone)]
pub struct SandboxLimits {
    /// Maximum number of WASM instructions per invocation.
    /// Default: 10,000,000 (10M).
    pub fuel: u64,

    /// Maximum memory in bytes.
    /// Default: 67,108,864 (64 MiB = 1024 pages * 64 KiB/page).
    pub memory_bytes: usize,

    /// Maximum wall-clock time per invocation in seconds.
    /// Default: 30.
    pub timeout_secs: u64,

    /// Maximum output size from a connector invocation in bytes.
    /// Default: 1,048,576 (1 MiB).
    pub max_response_bytes: usize,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            fuel: 10_000_000,
            memory_bytes: 64 * 1024 * 1024, // 64 MiB
            timeout_secs: 30,
            max_response_bytes: 1024 * 1024, // 1 MiB
        }
    }
}

impl SandboxLimits {
    /// Create limits with custom values.
    pub fn new(
        fuel: u64,
        memory_bytes: usize,
        timeout_secs: u64,
        max_response_bytes: usize,
    ) -> Self {
        Self {
            fuel,
            memory_bytes,
            timeout_secs,
            max_response_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let limits = SandboxLimits::default();
        assert_eq!(limits.fuel, 10_000_000);
        assert_eq!(limits.memory_bytes, 64 * 1024 * 1024);
        assert_eq!(limits.timeout_secs, 30);
    }
}
