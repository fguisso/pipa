-- Phase-1 "archive" lifecycle for pages.
--
-- Archived pages are unpublished (the serving layer 404s them) but their
-- files stay on disk so the admin can republish later. Distinct from delete
-- (which removes the row + scrubs the bundle).
--
-- We do NOT mutate `visibility` on archive so an unarchive restores whatever
-- the page was before (public / password / private). Archive is a flag the
-- serving layer checks first.

ALTER TABLE pages ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
