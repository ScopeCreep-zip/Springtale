//! Tool-call execution loop for AI adapters.
//!
//! When the bot's AI fallback path runs, the adapter is allowed to emit
//! tool calls against any enabled connector's actions. This module
//! handles the full loop:
//!
//! 1. Enumerate callable tools from the connector registry.
//! 2. Call the adapter's `complete_with_tools`.
//! 3. If the model emitted tool calls, execute each via
//!    `registry.execute()` (which runs the existing capability checks
//!    and sandbox enforcement).
//! 4. Feed the results back to the model as `tool` role messages.
//! 5. Loop until the model produces a plain-text response or we hit
//!    the iteration cap.
//!
//! # Security model
//!
//! - Tool names are `{connector_name}__{action}` (double underscore).
//!   Splitting on `__` gives back the connector name and the action.
//! - **All tool execution runs through `ConnectorRegistry::execute`**
//!   — the same path the rule engine uses. That means capability
//!   enforcement, rate limiting, and sandbox isolation apply to AI
//!   tool calls automatically. We never hand the model a backdoor.
//! - Only **enabled** connectors are exposed. A disabled connector
//!   cannot be called even if the model already saw it in a prior turn.
//! - Iteration cap defaults to `5`: enough for two-hop "look up then
//!   send" flows, tight enough to make runaway loops visible.
//! - Tool outputs are truncated to 8 KiB before being fed back to the
//!   model so an oversized payload can't push the conversation past
//!   context limits.

pub mod builder;
pub mod loop_;

pub use builder::{TOOL_NAME_SEPARATOR, collect_tools, split_tool_name};
pub use loop_::{run_with_tools, ToolRunnerCall, ToolRunnerDeps, ToolRunnerError};
