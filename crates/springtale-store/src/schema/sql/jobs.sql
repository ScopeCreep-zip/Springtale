-- Background job queue. attempts/max_attempts implement bounded
-- retry; status moves pending → in_progress → completed/failed.
CREATE TABLE IF NOT EXISTS jobs (
    id           TEXT    PRIMARY KEY,
    payload      TEXT    NOT NULL,
    status       TEXT    NOT NULL DEFAULT 'pending',
    attempts     INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at   TEXT    NOT NULL,
    started_at   TEXT,
    last_error   TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
