-- Long-lived named API tokens (plan 6.6, finding 109).
--
-- The bearer used to be `HMAC(vault passphrase)`: deterministic, not
-- rotatable, and a passphrase equivalent. It is now a random 32-byte
-- value from the OS CSPRNG, shown to the user exactly once and stored
-- here only as `sha256(token)` — the Home Assistant long-lived-token
-- posture ("the access token string is not saved; you must record it in
-- a secure place"). A stolen database therefore yields no usable bearer.
--
-- `DELETE FROM api_tokens WHERE id = ?` is the revocation: the next
-- request carrying that token fails its hash lookup immediately.
CREATE TABLE IF NOT EXISTS api_tokens (
    id          TEXT NOT NULL PRIMARY KEY,  -- UUID, the revocation handle
    name        TEXT NOT NULL,              -- user-chosen label, e.g. springtale-cli@host
    token_hash  BLOB NOT NULL UNIQUE,       -- sha256(token bytes), 32 bytes
    created_at  INTEGER NOT NULL,           -- unix ms
    last_used   INTEGER                     -- unix ms, NULL until first use
) STRICT;

CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);
