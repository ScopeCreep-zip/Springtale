//! Errors raised by the conversational task-setup engine.

use crate::error::BotError;

/// Failure modes of a conversation turn.
///
/// The engine is deliberately forgiving — a message it can't parse is
/// not an error, it's a `Capability`/`Clarify` reply. `ConversationError`
/// is reserved for genuine infrastructure failures (store unreachable,
/// recipe catalogue unreadable) that the caller surfaces and logs.
#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    /// A bot-subsystem call failed (session load/save, context push).
    #[error(transparent)]
    Bot(#[from] BotError),

    /// Reading the recipe catalogue / running preflight failed.
    #[error("recipe operation failed: {0}")]
    Operation(#[from] springtale_runtime::OperationError),
}
