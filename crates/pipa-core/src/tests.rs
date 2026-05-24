//! Unit tests for the pure-data enums and error types defined in this
//! crate. Strictly no I/O — anything async/storage-backed lives in the
//! adapters' integration tests.

use std::str::FromStr;

use crate::audit::AuditAction;
use crate::comment::CommentStatus;
use crate::device::Scope;
use crate::error::CoreError;
use crate::page::{Mode, Visibility};

#[test]
fn mode_round_trip() {
    for m in [Mode::Static, Mode::Spa] {
        let s = m.as_str();
        let parsed: Mode = s.parse().expect("parse mode");
        assert_eq!(parsed, m, "round trip {s}");
    }
}

#[test]
fn mode_rejects_garbage() {
    for bad in ["", "STATIC", "Spa", "html", "  static", "static\n"] {
        let err = Mode::from_str(bad).expect_err(&format!("garbage {bad:?}"));
        assert!(matches!(err, CoreError::InvalidInput(_)), "{bad:?} -> {err:?}");
    }
}

#[test]
fn visibility_round_trip() {
    for v in [Visibility::Private, Visibility::Public, Visibility::Password] {
        let s = v.as_str();
        let parsed: Visibility = s.parse().expect("parse visibility");
        assert_eq!(parsed, v, "round trip {s}");
    }
}

#[test]
fn visibility_rejects_garbage() {
    for bad in ["", "PRIVATE", "Public", "secret", "pass word"] {
        let err = Visibility::from_str(bad).expect_err(&format!("garbage {bad:?}"));
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }
}

#[test]
fn scope_round_trip() {
    for s in [Scope::Interactive, Scope::Automation] {
        let label = s.as_str();
        let parsed: Scope = label.parse().expect("parse scope");
        assert_eq!(parsed, s);
    }
}

#[test]
fn scope_rejects_garbage() {
    for bad in ["", "INTERACTIVE", "Auto", "admin", "interactive,automation"] {
        let err = Scope::from_str(bad).expect_err(&format!("garbage {bad:?}"));
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }
}

#[test]
fn comment_status_round_trip() {
    for s in [
        CommentStatus::Visible,
        CommentStatus::Pending,
        CommentStatus::Hidden,
    ] {
        let label = s.as_str();
        let parsed: CommentStatus = label.parse().expect("parse status");
        assert_eq!(parsed, s);
    }
}

#[test]
fn comment_status_rejects_garbage() {
    for bad in ["", "VISIBLE", "shown", "Pending", "deleted"] {
        let err = CommentStatus::from_str(bad).expect_err(&format!("garbage {bad:?}"));
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }
}

#[test]
fn audit_action_round_trip_every_variant() {
    // Covers every variant. If a new variant is added, this test must be
    // updated, which is the intent.
    let cases: &[AuditAction] = &[
        AuditAction::AuthLogin,
        AuditAction::AuthRefresh,
        AuditAction::AuthRevoke,
        AuditAction::PageCreate,
        AuditAction::PageUpdate,
        AuditAction::PageDelete,
        AuditAction::PageVisibilityChange,
        AuditAction::DeviceRevoke,
        AuditAction::CommentCreate,
        AuditAction::CommentApprove,
        AuditAction::CommentHide,
        AuditAction::CommentDelete,
    ];
    for a in cases {
        let label = a.as_str();
        let parsed: AuditAction = label.parse().expect("parse audit action");
        assert_eq!(parsed, *a, "round trip {label}");
    }
    // Sanity: wire labels use a dot-separator (not snake_case) and we
    // intentionally have no overlapping prefixes that would parse ambiguously.
    let labels: Vec<&'static str> = cases.iter().map(|a| a.as_str()).collect();
    let mut sorted = labels.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), labels.len(), "duplicate labels");
}

#[test]
fn audit_action_rejects_garbage() {
    for bad in [
        "",
        "AUTH.LOGIN",
        "auth_login",
        "AuthLogin",
        "auth.login ",
        "page.create2",
        "comment.flag",
    ] {
        let err = AuditAction::from_str(bad).expect_err(&format!("garbage {bad:?}"));
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }
}

#[test]
fn core_error_display_strings() {
    // We don't lock down exact wording, but every variant should produce a
    // non-empty, recognizable string. This catches accidental Debug-only
    // variants.
    let cases: Vec<CoreError> = vec![
        CoreError::NotFound,
        CoreError::AlreadyExists,
        CoreError::InvalidInput("a thing".into()),
        CoreError::Unauthorized,
        CoreError::StorageFailure("disk full".into()),
        CoreError::RepositoryFailure("db down".into()),
    ];
    for e in cases {
        let s = format!("{e}");
        assert!(!s.is_empty(), "empty display for {e:?}");
    }
    // Spot-check the messages we care about.
    assert_eq!(format!("{}", CoreError::NotFound), "not found");
    assert_eq!(format!("{}", CoreError::AlreadyExists), "already exists");
    assert_eq!(format!("{}", CoreError::Unauthorized), "unauthorized");
    assert!(
        format!("{}", CoreError::InvalidInput("bad".into())).contains("bad"),
        "InvalidInput should embed the detail",
    );
    assert!(
        format!("{}", CoreError::StorageFailure("disk".into())).contains("disk"),
    );
    assert!(
        format!("{}", CoreError::RepositoryFailure("db".into())).contains("db"),
    );
}
