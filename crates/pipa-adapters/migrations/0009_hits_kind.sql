-- Distinguish page views from sub-resource (asset) fetches.
--
-- Before this, `gapes stats` counted every HTTP request as a "view", so a
-- single browser navigation inflated the count by one-per-asset (CSS, JS,
-- fonts, images). Analytics now records a `kind` on each hit and the headline
-- views/uniques count `kind = 'page'` only.
--
-- Backfill existing rows with the same heuristic the serving layer applies at
-- record time: an HTML document, the SPA/index route, or an extensionless path
-- is a page; anything with a non-HTML file extension is an asset.

ALTER TABLE hits ADD COLUMN kind TEXT NOT NULL DEFAULT 'asset';

UPDATE hits SET kind = 'page'
WHERE path = ''
   OR path = '/'
   OR path = 'index.html'
   OR path LIKE '%.html'
   OR path LIKE '%.htm'
   OR path NOT LIKE '%.%';

CREATE INDEX idx_hits_page_kind_ts ON hits(page_uuid, kind, ts);
