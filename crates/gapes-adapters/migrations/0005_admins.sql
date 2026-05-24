-- Admin user — single-row table for the server's owner identity.
--
-- Phase 1 is single-owner: at most one row exists, enforced at the
-- application layer (`create_admin` errors when `count_admins() > 0`).
-- The `synthetic_device_id` references a row in `devices` created at the
-- same time as the admin; it exists purely so admin web-UI handlers can
-- mint access tokens with a real device id as `sub` without inventing a
-- second identity model.
--
-- Password is argon2id (see `crates/gapes-adapters/src/crypto/passwords.rs`).

CREATE TABLE admins (
    id                   TEXT PRIMARY KEY,
    username             TEXT NOT NULL UNIQUE,
    password_hash        TEXT NOT NULL,
    synthetic_device_id  TEXT NOT NULL REFERENCES devices(id) ON DELETE RESTRICT,
    created_at           INTEGER NOT NULL
);
