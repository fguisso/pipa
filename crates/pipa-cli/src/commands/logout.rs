//! `pipa logout` — mint a short access, call `/api/auth/logout`, then wipe
//! the local credstore entry. On error we still wipe locally — a stuck token
//! shouldn't trap the user in a bad state.

use anyhow::Result;

use crate::commands::client_with_access;
use crate::config;
use crate::credstore;
use crate::output::{check, dim, warn_mark};

pub async fn run() -> Result<()> {
    let cfg = config::load();
    let Some(server) = cfg.server.clone() else {
        println!("{} not logged in", dim("·"));
        return Ok(());
    };

    let server_remote_ok = match client_with_access("read:*").await {
        Ok((client, _, access)) => match client.logout(&access).await {
            Ok(()) => true,
            Err(e) => {
                eprintln!("{} server-side logout failed: {e}", warn_mark());
                false
            }
        },
        Err(e) => {
            eprintln!("{} could not mint access to call logout: {e}", warn_mark());
            false
        }
    };

    let store = credstore::pick_best();
    let _ = store.delete(&server);

    // Forget the server too so the next `whoami` doesn't pretend we're logged in.
    let mut cfg = cfg;
    cfg.server = None;
    cfg.device_id = None;
    let _ = config::save(&cfg);

    if server_remote_ok {
        println!("{} logged out and credential wiped", check());
    } else {
        println!(
            "{} local credential wiped (server may still consider this device active until you revoke it)",
            check()
        );
    }
    Ok(())
}
