-- Owner sessions for browser-based server claim (Phase 1 redesign).
--
-- Each row = one browser that has proved ownership of this server. The first
-- row is created at first-boot TOFU claim via GET/POST /setup; the cookie set
-- on that browser ties future requests back to its row. Future sessions
-- (second browser, phone) are added via owner-only flows (deferred — Phase 1
-- only seeds the first session).
--
-- The cookie value is `<session_id>.<hmac_hex>` signed with the existing
-- server HMAC key. We only need to look up by id and verify the signature on
-- each request.

CREATE TABLE owner_sessions (
    id           TEXT    PRIMARY KEY,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER,
    user_agent   TEXT,
    ip           TEXT,
    revoked_at   INTEGER
);

CREATE INDEX idx_owner_sessions_active
    ON owner_sessions(revoked_at)
    WHERE revoked_at IS NULL;
