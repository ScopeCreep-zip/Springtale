//! AI adapter guardrails — OWASP LLM Top-10 2025 hardening middleware.
//!
//! `GuardrailAdapter<A>` wraps any `AiAdapter` with a composable layer of
//! safeguards orthogonal to what each adapter itself does:
//!
//! * **Wall-clock timeout (LLM10 — Unbounded Consumption).** Every
//!   call is wrapped in a `tokio::time::timeout` fence on top of the
//!   adapter's transport-layer timeout. Belt-and-brace against a
//!   provider that holds the connection open without ever returning.
//!
//! * **Output size cap (LLM10).** Truncates `AiResponse::content` past
//!   a configurable threshold so a runaway provider can't pipe an
//!   unbounded body into the next step of a chain.
//!
//! * **Refusal-rate metric.** Process-local atomic counter
//!   (`total_calls` + `total_refusals`) — surfaceable via the admin
//!   API for OWASP LLM07 visibility. Refusals here are AI-layer
//!   sanitiser blocks, not provider-side refusals; the latter surface
//!   as adapter `InferenceFailed` errors and are not counted.
//!
//! * **Per-bot daily token quota (LLM10).** Pluggable backend via the
//!   `TokenQuota` trait. The in-process [`InMemoryTokenQuota`] ships
//!   today; a SQLite-backed impl in `springtale-store` will plug in
//!   without changing this module's surface — the trait is the
//!   stable contract.
//!
//! The adapter input/output sanitiser (`sanitize::Sanitizer`) is
//! already wired at the individual adapter level — see
//! `anthropic/adapter.rs`, `openai/adapter.rs`, `ollama/adapter.rs`.
//! This middleware composes ON TOP of those, not in place of them.

mod adapter;
mod output_cap;
mod quota;
mod refusal;

pub use adapter::GuardrailAdapter;
pub use output_cap::DEFAULT_OUTPUT_CAP_BYTES;
pub use quota::{InMemoryTokenQuota, QuotaCheck, TokenQuota};
pub use refusal::{RefusalCounter, RefusalStats};
