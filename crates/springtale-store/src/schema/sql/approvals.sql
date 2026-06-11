-- W2 — approval-over-chat durable state.
--
-- `pending_approvals` backs the ChatApprovalGate: a row exists while a
-- destructive action waits for the owner's decision. Durable so a pending
-- approval survives daemon restart (2026 HITL interrupt pattern: pause =
-- persisted state + stable thread id, never an in-memory oneshot alone).
-- Rows are deleted on decision; rows past `expires_at` are denied at boot
-- and on read (deny-by-default — a dropped phone never silently grants).
--
-- `tool_loop_checkpoints` persists the chat tool-loop's conversation state
-- while it blocks on an approval, keyed by the approval id. On
-- resolve-after-restart the resumer rebuilds the loop from this row.
-- `messages_json` carries the model conversation; `pending_tool_json` the
-- tool call awaiting the verdict. Content is operator-owned chat data on a
-- 0o600 local store (same posture as session context).

CREATE TABLE IF NOT EXISTS pending_approvals (
    id              TEXT    NOT NULL PRIMARY KEY,   -- ApprovalRequestId UUID
    connector_name  TEXT    NOT NULL,
    capability_json TEXT    NOT NULL,               -- serialized Capability
    agent_id        TEXT,                           -- firing bot, NULL for chat-direct
    summary         TEXT    NOT NULL,               -- human-readable card body
    requested_at    INTEGER NOT NULL,               -- unix ms
    expires_at      INTEGER NOT NULL,               -- unix ms (deny after)
    decision_json   TEXT                            -- NULL while pending
) STRICT;

CREATE INDEX IF NOT EXISTS idx_pending_approvals_expiry
    ON pending_approvals(expires_at);

-- Keyed by SESSION (the LangGraph thread_id pattern — 2026 durable-resume
-- standard): one paused loop per conversation. `approval_id` is a
-- correlation column so the boot resumer can join a surviving (unexpired)
-- pending approval back to its conversation. On resume the bound
-- `pending_tool_json` is executed EXACTLY as persisted (OWASP Agentic 2026:
-- bind approval to the exact action; expiry + single-use resolve give
-- replay protection).
CREATE TABLE IF NOT EXISTS tool_loop_checkpoints (
    session_key        TEXT    NOT NULL PRIMARY KEY, -- thread id (resume target)
    approval_id        TEXT,                         -- correlation → pending_approvals.id
    origin_connector   TEXT    NOT NULL,             -- where to deliver the result
    origin_channel     TEXT    NOT NULL,
    messages_json      TEXT    NOT NULL,             -- ChatMessage[] at pause time
    pending_tool_json  TEXT    NOT NULL,             -- the gated ToolCall (bound action)
    created_at         INTEGER NOT NULL              -- unix ms
) STRICT;

CREATE INDEX IF NOT EXISTS idx_tool_loop_checkpoints_approval
    ON tool_loop_checkpoints(approval_id);
