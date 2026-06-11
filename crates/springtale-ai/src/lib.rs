#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod adapter;
pub mod anthropic;
pub mod config;
pub mod error;
pub mod extractor;
pub mod factory;
pub mod guardrail;
pub mod noop;
pub mod ollama;
pub mod openai;
pub mod parser;
pub mod sanitize;
pub mod validate;

pub use adapter::{
    AiAdapter, AiOptions, AiRequest, AiResponse, AiStream, ChatMessage, ConnectorInfo,
    DisclosureLevel, MAX_TOOLS_HARD_CAP, StreamChunk, ToolCall, ToolDefinition, ToolPolicy,
    ToolResult, schema_has_secret_fields,
};
pub use anthropic::AnthropicAdapter;
pub use anthropic::adapter::AnthropicConfig;
pub use error::AiError;
pub use extractor::{ExtractOptions, ExtractOutcome, ExtractorError, StructuredExtractor};
pub use factory::{create_adapter, verify_model_pin};
pub use guardrail::{
    DEFAULT_OUTPUT_CAP_BYTES, GuardrailAdapter, InMemoryTokenQuota, QuotaCheck, RefusalCounter,
    RefusalStats, TokenQuota,
};
pub use noop::NoopAdapter;
pub use ollama::OllamaAdapter;
pub use ollama::types::OllamaConfig;
pub use openai::OpenAiCompatAdapter;
pub use openai::adapter::OpenAiConfig;
pub use parser::NlRuleParser;
pub use sanitize::{SanitizePolicy, Sanitizer};
