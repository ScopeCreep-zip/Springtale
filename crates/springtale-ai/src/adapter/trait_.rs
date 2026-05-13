use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use specta::Type;
use crate::error::AiError;
use crate::extractor::StructuredExtractor;
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
#[derive(Debug, Clone, Serialize, Type)]
#[non_exhaustive]
pub enum AiRequest {
    /// Simple text completion.
    Complete { prompt: String },

    /// Chat-style completion with message history.
    Chat { messages: Vec<ChatMessage> },
}

/// A single message in a chat conversation.
///
/// For simple text turns only `role` and `content` are set. When the
/// model emits a tool call the assistant message carries `tool_calls`
/// with `content` empty. When the bot runtime sends a tool's output
/// back, it uses role `"tool"` plus `tool_call_id` to match the id the
/// model produced. Adapters that support tool calling translate these
/// into their vendor-specific wire format; adapters without tool
/// support simply drop the extra fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct ChatMessage {
    /// Role: `"system"`, `"user"`, `"assistant"`, or `"tool"`.
    pub role: String,
    /// Message content — text only (empty when the assistant emits only
    /// a tool call).
    pub content: String,
    /// Tool calls the assistant emitted on this turn, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For `role == "tool"` messages, the id of the matching assistant
    /// tool call this message carries the result for. Used by
    /// OpenAI/Anthropic adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Function name for tool result messages. Ollama uses `tool_name`
    /// instead of `tool_call_id` to correlate results with calls.
    /// The tool_runner populates both so each adapter picks what its
    /// vendor expects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            tool_name: None,
        }
    }
}

// ── Tool-calling types ──────────────────────────────────────────────

/// Description of a tool the model may call.
///
/// The bot runtime builds this from each enabled connector action's
/// `ActionDecl`. Names follow the convention `<connector>__<action>`
/// so OpenAI/Anthropic can accept them (hyphen is allowed but `::` and
/// `.` are not).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input. Adapters pass this straight to
    /// their vendor's tool field so connector manifests drive the model's
    /// understanding directly.
    pub input_schema: serde_json::Value,
}

/// A tool invocation emitted by the model.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ToolCall {
    /// Vendor-issued id; echoed back on the tool result message so the
    /// model can correlate requests and responses.
    pub id: String,
    /// Tool name (matches [`ToolDefinition::name`]).
    pub name: String,
    /// Arguments object emitted by the model. Always a JSON value so
    /// callers can validate against the tool's input schema.
    pub arguments: serde_json::Value,
}

/// Result of executing a tool, sent back to the model on the next turn.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ToolResult {
    /// Id echoed from the [`ToolCall`] this result corresponds to.
    pub id: String,
    /// Serialized output. Adapters transport this as the content of a
    /// `"tool"` role message.
    pub content: String,
    /// Hint to the model that the call failed. Vendors render this
    /// differently (Anthropic `is_error: true`, OpenAI prepends
    /// `[ERROR]` in content).
    #[serde(default)]
    pub is_error: bool,
}

// ── Tool policy ─────────────────────────────────────────────────────

/// Per-bot policy controlling which connector actions the AI can see and call.
///
/// Pattern: LangChain's `bind_tools` + MCP `annotations.requiresConsent`.
/// Default is ZERO tools (empty `allow` list) per OWASP LLM06:
/// "Limit extensions to the minimum necessary."
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct ToolPolicy {
    /// Glob allow-list. e.g. `["connector-telegram__*", "connector-github__read_*"]`.
    /// Empty = no tools exposed (safe default).
    #[serde(default)]
    pub allow: Vec<String>,
    /// Glob deny-list. Overrides allow. e.g. `["*__execute", "*__delete_*"]`.
    #[serde(default)]
    pub deny: Vec<String>,
    /// Max tool-call iterations per AI invocation. 0 = use default (5).
    #[serde(default)]
    pub max_iterations: u8,
}

impl ToolPolicy {
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        if self.allow.is_empty() {
            return false;
        }
        let allowed = self.allow.iter().any(|pat| glob_match(pat, tool_name));
        let denied = self.deny.iter().any(|pat| glob_match(pat, tool_name));
        allowed && !denied
    }

    pub fn effective_max_iterations(&self) -> usize {
        if self.max_iterations > 0 {
            self.max_iterations as usize
        } else {
            5
        }
    }
}

/// Minimal glob: supports `*` at start (suffix match), end (prefix match),
/// or both (contains match). Single `*` matches everything.
fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match (pattern.starts_with('*'), pattern.ends_with('*')) {
        (true, true) => {
            let inner = &pattern[1..pattern.len() - 1];
            name.contains(inner)
        }
        (true, false) => {
            let suffix = &pattern[1..];
            name.ends_with(suffix)
        }
        (false, true) => {
            let prefix = &pattern[..pattern.len() - 1];
            name.starts_with(prefix)
        }
        (false, false) => pattern == name,
    }
}

/// Field names in a tool's input schema that suggest secrets. Tools whose
/// schemas contain these are rejected from the AI's catalog — credentials
/// come from the connector config, never from the model.
const SECRET_FIELD_PATTERNS: &[&str] = &["_key", "_secret", "_token", "_password", "passphrase"];

/// Hard cap on tools per AI call. Anthropic docs report degradation past ~50.
pub const MAX_TOOLS_HARD_CAP: usize = 50;

/// Returns true if a JSON Schema's `properties` object contains field
/// names that look like they hold secrets.
pub fn schema_has_secret_fields(schema: &serde_json::Value) -> bool {
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for key in props.keys() {
            let lower = key.to_ascii_lowercase();
            if SECRET_FIELD_PATTERNS.iter().any(|pat| lower.contains(pat)) {
                return true;
            }
        }
    }
    false
}

// ── Response types ──────────────────────────────────────────────────

/// Response from a non-streaming AI completion.
#[derive(Debug, Clone, Default)]
pub struct AiResponse {
    /// The generated text content.
    pub content: String,
    /// Why the generation stopped (e.g., `"stop"`, `"length"`, or
    /// `"tool_use"` when the model requested one or more tool calls).
    pub finish_reason: Option<String>,
    /// Token usage statistics (if the provider returns them).
    pub usage: Option<TokenUsage>,
    /// Tool calls the model emitted on this turn. Empty when the
    /// response is plain text.
    pub tool_calls: Vec<ToolCall>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
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
#[derive(Debug, Clone, Serialize, Type)]
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
#[derive(Debug, Clone, Serialize, Type)]
pub struct TriggerInfo {
    pub name: String,
    pub description: String,
    /// Parameter schema (only if disclosure_level == FullSchema).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Action metadata for AI context.
#[derive(Debug, Clone, Serialize, Type)]
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

    /// Non-streaming completion with tool-calling support.
    ///
    /// Adapters translate [`ToolDefinition`] entries into their native
    /// tool format (Anthropic `tools`, OpenAI `tools` with `type:
    /// "function"`, Ollama `tools`). If the model decides to call a
    /// tool, the returned `AiResponse.tool_calls` is non-empty and the
    /// caller (usually [`crate::adapter::trait_`] consumers in
    /// `springtale-bot::tool_runner`) executes each call and feeds the
    /// results back in a follow-up message list.
    ///
    /// Default implementation ignores `tools` and delegates to
    /// [`complete`] — used by `NoopAdapter` and any adapter that
    /// hasn't added native tool support yet.
    async fn complete_with_tools(
        &self,
        request: AiRequest,
        options: AiOptions,
        _tools: &[ToolDefinition],
    ) -> Result<AiResponse, AiError> {
        self.complete(request, options).await
    }

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

    /// Structured-extraction capability discovery.
    ///
    /// Adapters that support schema-constrained JSON extraction
    /// (OpenAI strict `json_schema`, Anthropic structured outputs /
    /// forced tool use, Ollama `format: <schema>`) override this to
    /// return `Some(self)`. `NoopAdapter` and adapters without
    /// constrained decoding return `None` — recipes that request
    /// [`springtale_core::rule::action::ExtractKind::LlmSchema`]
    /// fail preflight with a clear "this adapter doesn't do
    /// structured outputs" message rather than silently dropping
    /// to JSON-mode and returning malformed data.
    fn structured_extractor(&self) -> Option<&dyn StructuredExtractor> {
        None
    }
}
