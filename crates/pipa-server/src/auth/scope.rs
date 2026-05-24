//! Scope helpers. The wire format is `verb:target` where `verb` is one of
//! `read | deploy | admin | destroy | manage` and `target` is `*`, `new`, or
//! a ULID.
//!
//! `check_scope(claims, "deploy", Some("01HXY..."))` answers "does this
//! claims string permit `deploy:<that ULID>`?" — wildcards and `*` widen the
//! match, otherwise the verb + target must agree literally.

use pipa_core::device::AccessTokenClaims;

/// Verbs the server recognizes. Anything else is a hard reject — we want a
/// closed enum so a typo cannot silently grant unintended access.
const VALID_VERBS: &[&str] = &["read", "deploy", "admin", "destroy", "manage"];

#[derive(Debug, Clone)]
pub struct ScopeRef<'a> {
    pub verb: &'a str,
    pub target: &'a str,
}

/// Parse `verb:target`. Returns `None` on missing colon or unknown verb.
pub fn parse_scope(s: &str) -> Option<ScopeRef<'_>> {
    let (verb, target) = s.split_once(':')?;
    if !VALID_VERBS.contains(&verb) {
        return None;
    }
    if target.is_empty() {
        return None;
    }
    Some(ScopeRef { verb, target })
}

/// Returns true when the token's scope string permits `<verb>:<target>`.
/// A token scope of `read:*` permits `read:<anything>`. `deploy:new` only
/// matches `deploy:new` literally. We never expand `*` for write verbs to a
/// specific ULID unless the holder asked for `verb:*` explicitly — that's a
/// deliberate decision so refresh-token holders cannot retroactively gain
/// access to a page they did not yet know about.
pub fn check_scope(claims: &AccessTokenClaims, verb: &str, target: Option<&str>) -> bool {
    let Some(token_scope) = parse_scope(&claims.scope) else {
        return false;
    };
    if token_scope.verb != verb {
        return false;
    }
    match target {
        None => true,
        Some(t) => token_scope.target == "*" || token_scope.target == t,
    }
}
