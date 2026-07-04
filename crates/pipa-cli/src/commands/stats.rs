//! `pipa stats <uuid>` — render the ASCII analytics block specced in
//! `phase-1-core.md`.

use anyhow::Result;

use crate::cli::StatsArgs;
use crate::commands::client_with_access;
use crate::output::{bar, rule, rule_titled};

const WIDTH: usize = 58;

pub async fn run(args: StatsArgs, json: bool) -> Result<()> {
    let scope = format!("read:{}", args.uuid);
    let (client, _server, access) = client_with_access(&scope).await?;
    let resp = client.stats(&access, &args.uuid, &args.range).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    let label = pretty_range(&resp.range);
    println!("{}", rule_titled(&format!("last {label}"), WIDTH));

    let views = resp.stats.views;
    let uniques = resp.stats.uniques;
    // Bar denominator: scale the views bar against itself for now — gives a
    // full bar when there are any views. A second bar normalizing across days
    // is overkill for Phase 1.
    let bar_str = if views > 0 { bar(views, views) } else { bar(0, 1) };
    println!(
        "views      {:>6}    {}  uniques  {}",
        views, bar_str, uniques
    );

    println!("top paths");
    if resp.stats.top_paths.is_empty() {
        println!("  (none)");
    } else {
        for (path, count) in &resp.stats.top_paths {
            println!("  {:<26} {:>6}", trim(path, 26), count);
        }
    }

    println!("top referrers");
    if resp.stats.top_referrers.is_empty() {
        println!("  (none)");
    } else {
        for (ref_, count) in &resp.stats.top_referrers {
            let r = if ref_.is_empty() { "(direct)" } else { ref_.as_str() };
            println!("  {:<26} {:>6}", trim(r, 26), count);
        }
    }

    println!("{}", rule(WIDTH));
    Ok(())
}

fn pretty_range(r: &str) -> &str {
    match r {
        "24h" => "24 hours",
        "7d" => "7 days",
        "30d" => "30 days",
        "all" => "all time",
        other => other,
    }
}

fn trim(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}
