-- Stash the new device's refresh-token plaintext on the pairing row so that
-- `poll_pairing` can hand it back to the CLI once it has been approved on the
-- browser. We can't store plaintext refresh tokens at rest, so we encrypt with
-- ChaCha20-Poly1305 using the pairing secret (which the CLI already knows) as
-- the key. After `poll_pairing` succeeds the row stays around until expiry but
-- the plaintext is single-use because we mark the pairing approved+returned.
--
-- The pairing secret is only ever revealed to the CLI that initiated the
-- pairing (it's printed once, never persisted on the server). So an attacker
-- who reads the DB cannot decrypt the refresh plaintext without also having
-- captured the original CLI session.

ALTER TABLE device_pairings ADD COLUMN refresh_plaintext_enc TEXT;
ALTER TABLE device_pairings ADD COLUMN refresh_plaintext_nonce TEXT;
