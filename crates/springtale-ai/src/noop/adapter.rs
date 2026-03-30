use async_trait::async_trait;

use crate::adapter::trait_::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ConnectorInfo,
};
use crate::error::AiError;
use springtale_core::rule::types::Rule;

/// The default AI adapter: no AI, no network, no dependencies.
///
/// Returns `Err(AiError::Disabled)` for all methods. `is_available()`
/// returns `false`. The entire platform works correctly with this adapter —
/// rules execute deterministically, commands route to connectors, scheduled
/// tasks fire on time. AI is optional, not foundational.
///
/// When the user plugs in a real adapter (Phase 2), only the adapter
/// implementation changes. Everything else stays the same.
pub struct NoopAdapter;

#[async_trait]
impl AiAdapter for NoopAdapter {
    async fn complete(
        &self,
        _request: AiRequest,
        _options: AiOptions,
    ) -> Result<AiResponse, AiError> {
        Err(AiError::Disabled)
    }

    async fn stream(&self, _request: AiRequest, _options: AiOptions) -> Result<AiStream, AiError> {
        Err(AiError::Disabled)
    }

    async fn parse_rule(
        &self,
        _intent: &str,
        _available_connectors: &[ConnectorInfo],
    ) -> Result<Rule, AiError> {
        Err(AiError::Disabled)
    }

    async fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_complete_returns_disabled() {
        let adapter = NoopAdapter;
        let result = adapter
            .complete(
                AiRequest::Complete {
                    prompt: "hello".into(),
                },
                AiOptions::default(),
            )
            .await;
        assert!(matches!(result, Err(AiError::Disabled)));
    }

    #[tokio::test]
    async fn test_noop_stream_returns_disabled() {
        let adapter = NoopAdapter;
        let result = adapter
            .stream(
                AiRequest::Complete {
                    prompt: "hello".into(),
                },
                AiOptions::default(),
            )
            .await;
        assert!(matches!(result, Err(AiError::Disabled)));
    }

    #[tokio::test]
    async fn test_noop_parse_rule_returns_disabled() {
        let adapter = NoopAdapter;
        let result = adapter.parse_rule("remind me at 5pm", &[]).await;
        assert!(matches!(result, Err(AiError::Disabled)));
    }

    #[tokio::test]
    async fn test_noop_is_available_false() {
        let adapter = NoopAdapter;
        assert!(!adapter.is_available().await);
    }
}
