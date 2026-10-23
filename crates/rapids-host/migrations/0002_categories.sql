-- RAPIDS queue store, schema version 2.
--
-- Documents and archives are first-class categories. SQLite cannot widen a
-- CHECK constraint in place, so the tasks table is rebuilt without changing
-- any stored row or the queue's ordering rules.

CREATE TABLE tasks_next (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    url              TEXT    NOT NULL,
    final_url        TEXT,
    dest_path        TEXT    NOT NULL,
    file_name        TEXT    NOT NULL,
    category         TEXT    NOT NULL
                     CHECK (category IN ('apps', 'movies', 'music', 'images',
                                         'documents', 'archives', 'other')),
    status           TEXT    NOT NULL
                     CHECK (status IN ('queued', 'scheduled', 'downloading',
                                       'paused', 'completed', 'error')),
    priority         INTEGER NOT NULL DEFAULT 0,
    sequence         INTEGER NOT NULL,
    total_bytes      INTEGER,
    downloaded       INTEGER NOT NULL DEFAULT 0,
    speed_bps        INTEGER NOT NULL DEFAULT 0,
    eta_seconds      INTEGER,
    connections      INTEGER,
    speed_limit_bps  INTEGER,
    expected_sha256  TEXT,
    observed_sha256  TEXT,
    error_message    TEXT,
    attempts         INTEGER NOT NULL DEFAULT 0,
    next_attempt_at  INTEGER,
    schedule_id      INTEGER REFERENCES schedules(id) ON DELETE SET NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    started_at       INTEGER,
    completed_at     INTEGER,
    CHECK (total_bytes IS NULL OR total_bytes >= 0),
    CHECK (downloaded >= 0),
    CHECK (attempts >= 0)
);

INSERT INTO tasks_next (
    id, url, final_url, dest_path, file_name, category, status, priority,
    sequence, total_bytes, downloaded, speed_bps, eta_seconds, connections,
    speed_limit_bps, expected_sha256, observed_sha256, error_message, attempts,
    next_attempt_at, schedule_id, created_at, updated_at, started_at,
    completed_at
)
SELECT
    id, url, final_url, dest_path, file_name, category, status, priority,
    sequence, total_bytes, downloaded, speed_bps, eta_seconds, connections,
    speed_limit_bps, expected_sha256, observed_sha256, error_message, attempts,
    next_attempt_at, schedule_id, created_at, updated_at, started_at,
    completed_at
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_next RENAME TO tasks;

CREATE INDEX tasks_queue ON tasks (status, priority DESC, sequence)
    WHERE status IN ('queued', 'scheduled');

CREATE INDEX tasks_category ON tasks (category);

CREATE INDEX tasks_created ON tasks (created_at DESC);

CREATE INDEX tasks_due ON tasks (next_attempt_at)
    WHERE next_attempt_at IS NOT NULL;
