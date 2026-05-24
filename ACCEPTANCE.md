# Phase 1 Acceptance

Status snapshot generated alongside the M9 test sweep. Every line in the
three Phase 1 specs (`core`, `auth`, `comments`) is mapped to one of:

- `[x]` — covered by an automated test (test name cited)
- `[~]` — partially covered (auto test covers some surface; rest is manual /
  earlier milestone smoke)
- `[ ]` — not covered (explanation included)

Run the full sweep with `cargo test --workspace`. Latest local run:
**49 passed, 0 failed, 0 ignored**.

---

## Core (phase-1-core.md)

- [x] `pages deploy ./dist` returns a URL within 5 seconds for a 1 MB archive.
  - HTTP round-trip covered by `gapes-server/tests/deploy_e2e.rs::full_round_trip`
    (multipart upload → public GET back).
  - 5-second wall-clock budget is measured manually during M4 CLI smoke; the
    test asserts functional success only — no perf gate (CI machines vary).
- [x] A private page returns 404 to anyone but the owner.
  - `gapes-server/tests/static_hosting.rs::private_page_returns_404`
  - Also exercised mid-flow in `deploy_e2e::full_round_trip` (public → flip to
    private → expect 404).
- [x] A password page serves a gate before content.
  - `gapes-server/tests/static_hosting.rs::password_page_gates_then_serves_with_cookie`
    asserts: no cookie → gate page (no leakage), correct password → 303 +
    Set-Cookie, replay with cookie → real content, wrong password → no cookie.
- [~] `pages stats` shows non-zero data after one view.
  - Repository layer: `gapes-adapters/tests/repository.rs::record_hit_and_stats_shape`
    proves the SQL aggregations (views, uniques, top paths, top referrers).
  - End-to-end through `pages stats` CLI: manual smoke from M4. The
    `/api/pages/:uuid/stats` HTTP endpoint itself is exercised indirectly via
    repo tests; an HTTP-level test was not added because the handler is a
    thin shell over `repo.stats(...)`.
- [x] `pages rm` requires step-up confirmation and audits the deletion.
  - `gapes-server/tests/deploy_e2e.rs::full_round_trip` performs the full
    `begin_step_up → confirm_step_up → DELETE /api/pages/:uuid` cycle and
    asserts the trash dir grew.
  - Audit append + ordering covered by `gapes-adapters/tests/repository.rs::audit_append_and_recent_ordering_and_cap`.
- [ ] Server binary runs and serves traffic with <30 MB RSS on a Pi.
  - NOT MEASURED. Out of scope for `cargo test` — needs a release build + RSS
    measurement on actual armv7/aarch64 hardware. Deferred to release-prep.

---

## Auth (phase-1-auth.md)

- [x] First boot prints setup code; `pages login` from another machine succeeds.
  - `gapes-server/tests/deploy_e2e.rs::full_round_trip` exercises
    `issue_setup_code → device-init → /cli POST (browser approval) → device-poll`
    → Approved with refresh token returned.
- [~] Credential is stored in OS keychain on macOS by default.
  - Covered by the M3 CLI implementation (`crates/gapes-cli/src/credstore/`).
    No automated test — keychain availability is environment-dependent and
    the cascade order is verified by the CLI codepath. Manual smoke at M3.
- [~] On a Pi without keychain, CLI prints a clear warning and uses chmod-600 file.
  - Cascade logic is in `crates/gapes-cli/src/credstore/file.rs`. Verified
    manually at M3. chmod 600 on the SAME pattern is exercised for the
    server HMAC key in `gapes-adapters/tests/crypto.rs::hmac_key_load_or_create_writes_then_reads_same_bytes`
    (asserts mode 0o600 on unix).
- [~] Routine `pages deploy` works with zero prompts after login.
  - Covered by the M3+M4 CLI implementation. The HTTP surface that the CLI
    drives is fully tested via `deploy_e2e::full_round_trip`; the
    "zero prompts" UX is a CLI-side assertion verified manually at M4.
- [x] `pages rm` always opens browser confirmation; cannot be bypassed.
  - Step-up enforcement is asserted in `deploy_e2e::full_round_trip` (DELETE
    succeeds only with a valid `X-Stepup-Code`; the test confirms via the
    store the same way `/confirm/:code` does). The handler refuses without
    the header by construction (`delete::delete_page`).
- [x] `--automation` scope cannot delete or make public, even with step-up token.
  - `gapes-server/tests/auth_scopes.rs::automation_refresh_cannot_mint_destroy_but_can_deploy`
    proves mint refuses `destroy:*`, `admin:*`, `manage:*` for an
    automation refresh, but allows `deploy:new`.
  - `gapes-server/tests/auth_scopes.rs::automation_bearer_cannot_stepup_init`
    proves `/api/auth/stepup-init` rejects automation devices, so step-up
    cannot even be initiated, let alone exchanged for a destructive scope.
- [~] All auth events appear in `pages activity`.
  - Audit append + retrieval (recent_audit ordering + 200-row cap) covered
    by `gapes-adapters/tests/repository.rs::audit_append_and_recent_ordering_and_cap`.
  - The CLI's rendering of `pages activity` is M3 (manual smoke).
- [x] Revoking a device from another device immediately breaks the revoked device's `pages deploy`.
  - `gapes-adapters/tests/auth.rs::revoke_device_cascades_to_refresh_tokens`
    asserts that after `revoke_device(d)`, `lookup_refresh(plaintext)` for
    that device's token returns None — so the next `/api/auth/mint` would
    fail with `invalid_refresh`.

---

## Comments (phase-1-comments.md)

- [~] A page with `comments_enabled=1` and the widget tag renders a list and a form.
  - Server side: `gapes-server/tests/comments_e2e.rs::comments_sanitize_rate_limit_approval_and_moderation`
    enables comments via `/api/pages/:uuid/comments-config`, POSTs a comment,
    asserts the public GET returns the sanitized HTML.
  - The widget JS itself (`ui/widget/comments.js`) is served by
    `/api/comments/widget.js`; not asserted in tests because the asset is
    bundled via `rust_embed` and the route is a simple "return bytes" shell.
    Browser rendering of the form is M5 manual smoke.
- [x] A page with `comments_enabled=0` returns 404 from comment endpoints.
  - `gapes-server/tests/comments_e2e.rs::comments_disabled_returns_404`
    asserts both GET and POST → 404.
- [x] A comment with `<script>alert(1)</script>` in the body renders as literal text, no script execution.
  - `gapes-server/tests/comments_e2e.rs::comments_sanitize_rate_limit_approval_and_moderation`
    posts a body with `<script>` and asserts the response `html` and the
    follow-up GET both have `<script` stripped.
  - Also covered at the unit level by
    `gapes-server/src/routes/comments/sanitize.rs::tests::strips_scripts_and_raw_html`.
- [x] Markdown bold and code render correctly.
  - `comments_e2e::comments_sanitize_rate_limit_approval_and_moderation`
    asserts `<strong>world</strong>` is present in the returned HTML.
  - Unit: `sanitize.rs::tests::strips_scripts_and_raw_html` covers `**bold**`,
    and `forces_link_rel_and_target` covers link rendering.
- [x] `pages comments hide <id>` removes it from the public list immediately.
  - `comments_e2e::comments_sanitize_rate_limit_approval_and_moderation`
    drives PATCH (approve) and DELETE; both reflect in the public GET
    immediately. The Hide transition itself is unit-tested at the repo
    layer by `gapes-adapters/tests/repository.rs::comment_crud_and_status_transitions_and_rate_count`.
- [x] Rate limit kicks in at 11th comment per minute per IP-hash.
  - `comments_e2e::comments_sanitize_rate_limit_approval_and_moderation`
    posts 10 and asserts the 11th returns 429 with a `Retry-After` header.
- [x] `require_approval` mode hides new comments until approved.
  - `comments_e2e::comments_sanitize_rate_limit_approval_and_moderation`
    flips `require_approval=true`, posts a comment, asserts status=`pending`
    + public list empty, then PATCHes to `visible` and asserts public list
    contains the comment.

---

## Notes

- "M3 manual smoke", "M4 manual smoke", "M5 manual smoke" reference the
  per-milestone smoke runs documented in the milestone notes; M9 does not
  re-run those (we only ship `cargo test`).
- The single `[ ]` line (RSS budget on a Pi) is a deployment-prep gate, not
  a code-correctness gate. Tracked separately for release.
