//! `gapes concepts` — explain the access / zone / step-up model offline.
//! No network call; safe to run before login.

use anyhow::Result;

pub async fn run(json: bool) -> Result<()> {
    if json {
        let v = serde_json::json!({
            "access": {
                "axis": "who can open the page",
                "default": "password",
                "values": {
                    "password": "shared-password gate (needs --password)",
                    "noauth": "no gate; anyone who can reach it"
                }
            },
            "zone": {
                "axis": "which network the page is reachable on",
                "match": "exact — a page serves on exactly one channel",
                "values": {
                    "private": "served ONLY over the internal (LAN) channel",
                    "public": "served ONLY over the external (internet) channel"
                },
                "note": "only enforced when the server has the `zone` feature (see `gapes server`)"
            },
            "step_up": "loosening security (access=noauth or zone=public) needs a browser confirmation; tightening (password / private) does not"
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    println!("gapes access / zone model\n");
    println!("access — who can open the page:");
    println!("  password   shared-password gate (needs --password)   [default, secure]");
    println!("  noauth     no gate; anyone who can reach it\n");
    println!("zone — which network the page is reachable on (EXACT match — one channel each):");
    println!("  private    served ONLY over the internal (LAN) channel   [secure default]");
    println!("  public     served ONLY over the external (internet) channel");
    println!("  (only enforced when the server has the `zone` feature — see `gapes server`)\n");
    println!("step-up — loosening security (access=noauth or zone=public) requires a browser");
    println!("confirmation; tightening (password / private) does not.");
    Ok(())
}
