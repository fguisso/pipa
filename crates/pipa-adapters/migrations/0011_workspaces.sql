-- Phase 4: workspaces. Pages move from user-ownership to workspace-ownership;
-- `owner_kind` gains 'workspace'. Every user gets a personal workspace and
-- their existing pages migrate into it. The single-owner 'local' identity stays
-- outside the workspace model (superuser).

CREATE TABLE workspaces (
    id         TEXT    PRIMARY KEY,          -- ULID, or 'ws-<userid>' for personal
    name       TEXT    NOT NULL,
    kind       TEXT    NOT NULL DEFAULT 'team',   -- 'personal' | 'team'
    max_pages  INTEGER,                            -- NULL = unlimited
    max_bytes  INTEGER,                            -- NULL = unlimited
    created_at INTEGER NOT NULL
);

CREATE TABLE workspace_members (
    workspace_id TEXT    NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         TEXT    NOT NULL,                 -- owner|admin|editor|viewer
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, user_id)
);

CREATE INDEX idx_ws_members_user ON workspace_members(user_id);

-- Data migration: one personal workspace per existing user (deterministic
-- 'ws-<userid>' id so this stays pure SQL), owner membership, and move their
-- Phase-3 user-owned pages into it.
INSERT INTO workspaces (id, name, kind, created_at)
    SELECT 'ws-' || id, username || '''s workspace', 'personal', created_at FROM users;

INSERT INTO workspace_members (workspace_id, user_id, role, created_at)
    SELECT 'ws-' || id, id, 'owner', created_at FROM users;

UPDATE pages SET owner_kind = 'workspace', owner_id = 'ws-' || owner_id
    WHERE owner_kind = 'user';
