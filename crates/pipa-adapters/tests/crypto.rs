//! Integration tests for the crypto helpers.
//!
//! Argon2 verify/mismatch and HMAC-key persistence are simple but
//! security-critical — exercising them through the public surface protects
//! against accidental regressions when a future maintainer fiddles with the
//! parameters or the file IO.

use std::fs;

use pipa_adapters::{
    crypto::hmac_key::load_or_create, hash_password, verify_password,
};
use tempfile::TempDir;

#[test]
fn hash_then_verify_succeeds() {
    let h = hash_password("hunter2").expect("hash");
    let ok = verify_password(&h, "hunter2").expect("verify ok");
    assert!(ok, "matching password should verify");
}

#[test]
fn verify_wrong_password_fails() {
    let h = hash_password("hunter2").expect("hash");
    let ok = verify_password(&h, "hunter3").expect("verify");
    assert!(!ok, "non-matching password must fail verification");
}

#[test]
fn hashing_same_plaintext_twice_yields_different_hashes() {
    // Argon2 salts each hash, so two calls with the same plaintext should
    // never produce the same string. This is what makes the stored hash
    // useless as a credential by itself.
    let a = hash_password("same").expect("hash a");
    let b = hash_password("same").expect("hash b");
    assert_ne!(a, b, "hashes must differ thanks to per-call salt");

    // Both still verify correctly.
    assert!(verify_password(&a, "same").expect("verify a"));
    assert!(verify_password(&b, "same").expect("verify b"));
}

#[test]
fn hmac_key_load_or_create_writes_then_reads_same_bytes() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("hmac.key");

    // First call creates the file.
    let k1 = load_or_create(&path).expect("first load");
    assert_eq!(k1.as_bytes().len(), 32, "key must be 32 bytes");
    let meta = fs::metadata(&path).expect("metadata");
    assert_eq!(meta.len(), 32, "file must be exactly 32 bytes");

    // Second call reads the same bytes back.
    let k2 = load_or_create(&path).expect("second load");
    assert_eq!(
        k1.as_bytes(),
        k2.as_bytes(),
        "second load_or_create must return the persisted key"
    );

    // On unix, the file should be 0600.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "key file must be chmod 600 (got {mode:o})");
    }
}
