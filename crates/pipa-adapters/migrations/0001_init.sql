-- pipa Phase 1 initial schema.
--
-- All timestamps are INTEGER unix seconds. Booleans are INTEGER (0/1).
-- IDs that come from the application are ULIDs stored as TEXT (Crockford base32).
--
-- Password-protected pages do NOT have a server-side cookie table: access is
-- proved by a signed cookie (HMAC over `<page_uuid>|<expires>`) that the
-- server verifies on each request. See `phase-1-core.md` §HTTP API.
--
-- `PRAGMA foreign_keys = ON;` here is informational — sqlx enables FKs per
-- connection, so this PRAGMA is a no-op when run via the migrator but makes
-- the intent obvious to anyone reading the schema with `sqlite3`.

PRAGMA foreign_keys = ON;

CREATE TABLE pages (
    uuid                       TEXT    PRIMARY KEY,
    name                       TEXT,
    mode                       TEXT    NOT NULL DEFAULT 'spa',
    visibility                 TEXT    NOT NULL DEFAULT 'private',
    password_hash              TEXT,
    owner_kind                 TEXT    NOT NULL DEFAULT 'local',
    owner_id                   TEXT    NOT NULL DEFAULT 'local',
    size_bytes                 INTEGER NOT NULL DEFAULT 0,
    file_count                 INTEGER NOT NULL DEFAULT 0,
    comments_enabled           INTEGER NOT NULL DEFAULT 0,
    comments_require_approval  INTEGER NOT NULL DEFAULT 0,
    created_at                 INTEGER NOT NULL,
    updated_at                 INTEGER NOT NULL
);

CREATE INDEX idx_pages_owner ON pages(owner_kind, owner_id);

CREATE TABLE hits (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    page_uuid   TEXT    NOT NULL REFERENCES pages(uuid) ON DELETE CASCADE,
    ts          INTEGER NOT NULL,
    ip_hash     TEXT    NOT NULL,
    ua_hash     TEXT,
    path        TEXT    NOT NULL,
    referrer    TEXT,
    status      INTEGER NOT NULL
);

CREATE INDEX idx_hits_page_ts ON hits(page_uuid, ts);

CREATE TABLE audit_events (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        INTEGER NOT NULL,
    actor     TEXT    NOT NULL,
    ip_hash   TEXT,
    scope     TEXT,
    action    TEXT    NOT NULL,
    target    TEXT,
    success   INTEGER NOT NULL,
    details   TEXT
);

CREATE INDEX idx_audit_ts ON audit_events(ts);

CREATE TABLE comments (
    id          TEXT    PRIMARY KEY,
    page_uuid   TEXT    NOT NULL REFERENCES pages(uuid) ON DELETE CASCADE,
    parent_id   TEXT    REFERENCES comments(id),
    author      TEXT    NOT NULL,
    body_md     TEXT    NOT NULL,
    body_html   TEXT    NOT NULL,
    contact     TEXT,
    ts          INTEGER NOT NULL,
    ip_hash     TEXT    NOT NULL,
    status      TEXT    NOT NULL DEFAULT 'visible',
    user_agent  TEXT
);

CREATE INDEX idx_comments_page_ts ON comments(page_uuid, ts);

CREATE TABLE devices (
    id            TEXT    PRIMARY KEY,
    label         TEXT    NOT NULL,
    scope         TEXT    NOT NULL CHECK (scope IN ('interactive', 'automation')),
    created_at    INTEGER NOT NULL,
    last_seen_at  INTEGER,
    revoked_at    INTEGER
);

CREATE TABLE refresh_tokens (
    id          TEXT    PRIMARY KEY,
    device_id   TEXT    NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    token_hash  TEXT    NOT NULL,
    scope       TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    rotated_to  TEXT,
    revoked_at  INTEGER
);

CREATE INDEX idx_refresh_tokens_device ON refresh_tokens(device_id);
CREATE UNIQUE INDEX idx_refresh_tokens_hash ON refresh_tokens(token_hash);

CREATE TABLE device_pairings (
    code                TEXT    PRIMARY KEY,
    secret_hash         TEXT    NOT NULL,
    created_at          INTEGER NOT NULL,
    expires_at          INTEGER NOT NULL,
    approved_device_id  TEXT    REFERENCES devices(id),
    approved_at         INTEGER,
    refresh_token_id    TEXT    REFERENCES refresh_tokens(id)
);

CREATE TABLE step_up_tokens (
    code                TEXT    PRIMARY KEY,
    device_id           TEXT    NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    operation           TEXT    NOT NULL,
    target              TEXT,
    requesting_ip_hash  TEXT,
    created_at          INTEGER NOT NULL,
    expires_at          INTEGER NOT NULL,
    consumed_at         INTEGER,
    confirmed_at        INTEGER
);

CREATE TABLE setup_codes (
    code         TEXT    PRIMARY KEY,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    consumed_at  INTEGER
);
