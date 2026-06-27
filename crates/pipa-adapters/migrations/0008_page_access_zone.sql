-- Split the legacy single `visibility` axis into two orthogonal axes:
--   * access — how a visitor authenticates: `password` (secure default) or
--     `noauth`. Future methods (sso/social/link) slot in here.
--   * zone   — which network the page is reachable on: `public` (internet) or
--     `private` (LAN). Enforced behind the `zone` feature in pipa-server.
--
-- The legacy `private` visibility meant "404 for everyone" (owner browsing was
-- never implemented) — that role now belongs to `archived`, so legacy private
-- rows are archived on the way through and mapped to the secure access default.
--
-- No CHECK constraints on either column on purpose: both axes are designed to
-- grow more values later without a table rebuild.

ALTER TABLE pages ADD COLUMN access TEXT NOT NULL DEFAULT 'password';
ALTER TABLE pages ADD COLUMN zone   TEXT NOT NULL DEFAULT 'private';

-- access: public -> noauth; password/private -> password
UPDATE pages SET access = 'noauth'   WHERE visibility = 'public';
UPDATE pages SET access = 'password' WHERE visibility IN ('password', 'private');

-- zone: public -> public; everything else -> private (secure)
UPDATE pages SET zone = 'public'  WHERE visibility = 'public';
UPDATE pages SET zone = 'private' WHERE visibility IN ('password', 'private');

-- legacy "private == hidden from everyone" becomes the archive lifecycle state
UPDATE pages SET archived = 1 WHERE visibility = 'private';

-- drop the now-unused legacy column (SQLite >= 3.35; not indexed, safe to drop)
ALTER TABLE pages DROP COLUMN visibility;
