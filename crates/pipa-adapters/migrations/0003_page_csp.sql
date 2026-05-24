-- Per-page CSP knob. Sites that legitimately need to load CDN assets (React,
-- Babel, icon fonts, etc.) can opt out of the default strict CSP by setting
-- csp='off'; the page-level <meta http-equiv="Content-Security-Policy"> then
-- takes over. Existing rows keep the secure default.

ALTER TABLE pages ADD COLUMN csp TEXT NOT NULL DEFAULT 'strict'
    CHECK (csp IN ('strict', 'off'));
