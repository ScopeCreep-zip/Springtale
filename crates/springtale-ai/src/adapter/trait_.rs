use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AiError;
use springtale_core::rule::types::Rule;

// ── Stream type ─────────────────────────────────────────────────────

/// A stream of AI response chunks (SSE pattern).
///
/// Uses `futures_core::Stream` — the industry standard for async streaming
/// in Rust (same as async-openai, anthropic-sdk-rust).
pub type AiStream = Pin<Box<dyn futures_core::Stream<Item = Result<StreamChunk, AiError>> + Send>>;

/// A single chunk from a streaming AI response.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Incremental content delta.
    pub delta: String,
    /// Set when the stream is done (e.g., "stop", "length").
    pub finish_reason: Option<String>,
}

// ── Request types ───────────────────────────────────────────────────

/// Request payload sent to the AI adapter.
///
/// This is a **closed enum** — external code cannot add variants.
/// All fields are concrete `String` types. `SecretBox<T>` cannot be used
/// because `SecretBox::Serialize` requires `T: SerializableSecret` (an
/// opt-in marker trait that `String` does not implement by default).
///
/// This is Layer 1 of the two-layer defense: compile-time type safety
/// ensures secrets cannot accidentally enter the AI request.
/// Layer 2 (runtime sanitization) provides defense-in-depth.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum AiRequest {
    /// Simple text completion.
    Complete { prompt: String },

    /// Chat-style completion with message history.
    Chat { messages: Vec<ChatMessage> },
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: "system", "user", or "assistant".
    pub role: String,
    /// Message content — text only.
    pub content: String,
}

// ── Response types ──────────────────────────────────────────────────

/// Response from a non-streaming AI completion.
#[derive(Debug, Clone)]
pub struct AiResponse {
    /// The generated text content.
    pub content: String,
    /// Why the generation stopped (e.g., "stop", "length").
    pub finish_reason: Option<String>,
    /// Token usage statistics (if the provider returns them).
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics from an AI completion.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ── Options ─────────────────────────────────────────────────────────

/// Options controlling an AI adapter call.
#[derive(Debug, Clone)]
pub struct AiOptions {
    /// Maximum tokens to generate. Default: 4096.
    pub max_tokens: u32,
    /// Request timeout. Default: 30 seconds.
    pub timeout: Duration,
    /// Sampling temperature (0.0 = deterministic, 1.0 = creative).
    pub temperature: Option<f32>,
}

impl Default for AiOptions {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            timeout: Duration::from_secs(30),
            temperature: None,
        }
    }
}

// ── Connector metadata for NL→Rule ─────────────────────────────────

/// Controls how much connector metadata the AI can see.
///
/// Each connector's `DataDisclosure` in its manifest determines this.
/// The AI only sees what the connector declares it's willing to share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisclosureLevel {
    /// Name and description only. AI knows the connector exists.
    NameOnly,
    /// Trigger/action names and descriptions included.
    NamesAndDescriptions,
    /// Full parameter schemas included. Most useful but most exposed.
    FullSchema,
}

/// Lightweight connector metadata for AI rule generation.
///
/// Defined in springtale-ai (not springtale-connector) to avoid a
/// dependency cycle. The application layer maps from `ConnectorManifest`
/// to `ConnectorInfo` respecting each connector's `DataDisclosure`.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorInfo {
    /// Connector name (e.g., "connector-kick").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Available triggers (controlled by disclosure level).
    pub triggers: Vec<TriggerInfo>,
    /// Available actions (controlled by disclosure level).
    pub actions: Vec<ActionInfo>,
    /// What level of detail is shared with the AI.
    pub disclosure_level: DisclosureLevel,
}

/// Trigger metadata for AI context.
#[derive(Debug, Clone, Serialize)]
pub struct TriggerInfo {
    pub name: String,
    pub description: String,
    /// Parameter schema (only if disclosure_level == FullSchema).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Action metadata for AI context.
#[derive(Debug, Clone, Serialize)]
pub struct ActionInfo {
    pub name: String,
    pub description: String,
    /// Input parameter schema (only if disclosure_level == FullSchema).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// Output schema (only if disclosure_level == FullSchema).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

// ── Adapter trait ───────────────────────────────────────────────────

/// The AI adapter trait — the socket where users plug in their own AI.
///
/// Phase 1a ships only `NoopAdapter`. Phase 2 adds Ollama, OpenAI,
/// Anthropic adapters. The trait is designed to be correct from day one
/// so Phase 2 implementations don't change the interface.
///
/// **Security boundary:** All requests pass through the sanitization
/// layer (in `crate::sanitize`) before reaching the adapter. The
/// adapter itself does not need to validate inputs — that's handled
/// at a higher layer.
#[async_trait]
pub trait AiAdapter: Send + Sync + 'static {
    /// Non-streaming text completion.
    async fn complete(&self, request: AiRequest, options: AiOptions)
    -> Result<AiResponse, AiError>;

    /// Streaming text completion (SSE pattern).
    async fn stream(&self, request: AiRequest, options: AiOptions) -> Result<AiStream, AiError>;

    /// NL→Rule parser: convert natural language intent into a structured Rule.
    ///
    /// `available_connectors` provides context about what connectors are
    /// installed, respecting each connector's disclosure level.
    async fn parse_rule(
        &self,
        intent: &str,
        available_connectors: &[ConnectorInfo],
    ) -> Result<Rule, AiError>;

    /// Check if the adapter is configured and reachable.
    async fn is_available(&self) -> bool;
}
