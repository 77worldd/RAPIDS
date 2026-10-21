-- RAPIDS queue store, schema version 1.
--
-- Forward-only: once this file has shipped it is never edited. A mistake is
-- corrected by the next migration file, so every existing database and every
-- fresh one end up with the same schema.
--
-- Two rules shape the tables below.
--
-- 1. The database holds metadata and display state. It never decides which
--    bytes are already on disk: that is the sidecar manifest's job, and its
--    three-proof gate is the only resume authority in RAPIDS.
-- 2. There is no separate queue table. The queue is a projection of `tasks`
--    (`status` in 'queued' / 'scheduled', ordered by priority then arrival),
--    so a task cannot be in two states at once or be queued twice.

CREATE TABLE schedules (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    label          TEXT    NOT NULL,
    -- 'once' fires at start_at only; 'daily' repeats at start_at's wall
    -- clock time; 'weekly' repeats on the days in weekdays.
    kind           TEXT    NOT NULL CHECK (kind IN ('once', 'daily', 'weekly')),
    -- Anchor instant, epoch milliseconds. For repeating kinds this carries
    -- the wall clock time of day the window opens and the first occurrence.
    start_at       INTEGER NOT NULL,
    -- Length of the open window in minutes. 0 means a task that starts keeps
    -- running; a non-zero value pauses it again when the window closes.
    window_minutes INTEGER NOT NULL DEFAULT 0 CHECK (window_minutes >= 0),
    -- Bit 0 is Monday through bit 6 is Sunday; only read for 'weekly'.
    weekdays       INTEGER NOT NULL DEFAULT 0 CHECK (weekdays BETWEEN 0 AND 127),
    enabled        INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at     INTEGER NOT NULL
);

CREATE TABLE tasks (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    url              TEXT    NOT NULL,
    final_url        TEXT,
    dest_path        TEXT    NOT NULL,
    file_name        TEXT    NOT NULL,
    category         TEXT    NOT NULL
                     CHECK (category IN ('apps', 'movies', 'music', 'images', 'other')),
    status           TEXT    NOT NULL
                     CHECK (status IN ('queued', 'scheduled', 'downloading',
                                       'paused', 'completed', 'error')),
    -- Higher runs first; sequence is the arrival order and breaks ties, so
    -- equal priorities stay first-in-first-out.
    priority         INTEGER NOT NULL DEFAULT 0,
    sequence         INTEGER NOT NULL,
    total_bytes      INTEGER,
    downloaded       INTEGER NOT NULL DEFAULT 0,
    speed_bps        INTEGER NOT NULL DEFAULT 0,
    eta_seconds      INTEGER,
    -- NULL means the engine's own default connection plan (8 rising to 32).
    connections      INTEGER,
    -- NULL means this task has no ceiling of its own; a global one may still
    -- apply.
    speed_limit_bps  INTEGER,
    expected_sha256  TEXT,
    observed_sha256  TEXT,
    error_message    TEXT,
    -- Automatic retries already spent, and when the next one is due.
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

-- The queue projection: only waiting work is indexed, and it is already in
-- the order the scheduler consumes it.
CREATE INDEX tasks_queue ON tasks (status, priority DESC, sequence)
    WHERE status IN ('queued', 'scheduled');

CREATE INDEX tasks_category ON tasks (category);

CREATE INDEX tasks_created ON tasks (created_at DESC);

CREATE INDEX tasks_due ON tasks (next_attempt_at)
    WHERE next_attempt_at IS NOT NULL;

-- Outcomes outlive the task row. There is deliberately no foreign key here:
-- deleting a task must not erase the record of what happened to it.
CREATE TABLE history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id         INTEGER NOT NULL,
    url             TEXT    NOT NULL,
    file_name       TEXT    NOT NULL,
    dest_path       TEXT    NOT NULL,
    outcome         TEXT    NOT NULL CHECK (outcome IN ('completed', 'failed', 'cancelled')),
    total_bytes     INTEGER,
    observed_sha256 TEXT,
    error_message   TEXT,
    started_at      INTEGER,
    ended_at        INTEGER NOT NULL
);

CREATE INDEX history_ended ON history (ended_at DESC);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Seeds. Values are JSON scalars so a reader can parse without guessing the
-- type from the text.
INSERT INTO settings (key, value, updated_at) VALUES
    ('max_parallel',          '3',      CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('max_connections_total', '48',     CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('global_speed_limit',    'null',   CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    -- Rows left in 'downloading' when the app starts are returned to the
    -- queue. The sidecar manifest's three-proof gate still decides how many
    -- bytes survive; this setting only says whether to pick the work back up.
    ('resume_on_start',       'true',   CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('retry_max_attempts',    '3',      CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('retry_base_ms',         '5000',   CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    ('retry_cap_ms',          '300000', CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    -- Filled in by the host on first run with the person's downloads folder.
    ('default_dest_dir',      '""',     CAST(strftime('%s', 'now') AS INTEGER) * 1000);
