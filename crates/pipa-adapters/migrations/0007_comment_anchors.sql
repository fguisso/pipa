-- Annotation anchoring: each comment is now tied to a text selection.
-- Existing un-anchored comments are deleted (fresh start).

DELETE FROM comments;

ALTER TABLE comments ADD COLUMN anchor_selector TEXT NOT NULL DEFAULT '';
ALTER TABLE comments ADD COLUMN anchor_text     TEXT NOT NULL DEFAULT '';
ALTER TABLE comments ADD COLUMN anchor_offset   INTEGER NOT NULL DEFAULT 0;
