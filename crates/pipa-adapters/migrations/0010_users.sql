-- Phase 3: multi-user.
--
-- Regular self-service accounts (username + password). Pages they deploy are
-- owned under `owner_kind = 'user', owner_id = <users.id>`; the Phase-1
-- single-owner rows (`owner_kind = 'local'`) keep working alongside.

CREATE TABLE users (
    id            TEXT    PRIMARY KEY,          -- ULID
    username      TEXT    NOT NULL UNIQUE,
    email         TEXT,                          -- optional; for future OAuth/identity
    password_hash TEXT    NOT NULL,              -- argon2id
    created_at    INTEGER NOT NULL,
    disabled_at   INTEGER                        -- soft-disable
);

CREATE UNIQUE INDEX idx_users_username ON users(username);

-- Browser sessions for signed-in users (parallel to owner_sessions).
CREATE TABLE user_sessions (
    id           TEXT    PRIMARY KEY,
    user_id      TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER,
    user_agent   TEXT,
    ip           TEXT,
    revoked_at   INTEGER
);

CREATE INDEX idx_user_sessions_user ON user_sessions(user_id);

-- Tie CLI devices to a user. NULL = a legacy / single-owner ('local') device.
ALTER TABLE devices ADD COLUMN user_id TEXT REFERENCES users(id) ON DELETE CASCADE;

CREATE INDEX idx_devices_user ON devices(user_id);

-- OAuth scaffold: no live provider flow this phase, but the table + links exist
-- so Phase 5 can wire GitHub/Google without another migration.
CREATE TABLE oauth_identities (
    id         TEXT    PRIMARY KEY,
    user_id    TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider   TEXT    NOT NULL,   -- 'github' | 'google'
    subject    TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(provider, subject)
);
