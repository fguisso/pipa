//! Shared formatting helpers: colors, boxes, tables, byte sizes.

use nu_ansi_term::{Color, Style};

pub fn check() -> String {
    Color::Green.paint("✓").to_string()
}

pub fn warn_mark() -> String {
    Color::Yellow.paint("⚠").to_string()
}

#[allow(dead_code)]
pub fn cross() -> String {
    Color::Red.paint("✗").to_string()
}

pub fn dim(s: &str) -> String {
    Style::new().dimmed().paint(s).to_string()
}

#[allow(dead_code)]
pub fn bold(s: &str) -> String {
    Style::new().bold().paint(s).to_string()
}

pub fn cyan(s: &str) -> String {
    Color::Cyan.paint(s).to_string()
}

/// Render a one-line key/value pair with consistent padding.
pub fn kv(key: &str, val: &str) -> String {
    format!("  {:<10} {val}", dim(key))
}

/// Pretty-print byte sizes — "1.2 MB" style. Stops at GB; for Phase 1 we
/// won't see TB-scale pages.
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Format a unix-seconds timestamp using only stdlib + the `time` crate-style
/// arithmetic — we render as `YYYY-MM-DD HH:MM:SS` UTC. Good enough for CLI
/// tables; no localization in Phase 1.
pub fn fmt_ts(unix: i64) -> String {
    if unix <= 0 {
        return "—".into();
    }
    fmt_unix(unix)
}

fn fmt_unix(unix: i64) -> String {
    let days_per_400y: i64 = 146_097;
    let secs_per_day: i64 = 86_400;

    let total_days = unix.div_euclid(secs_per_day);
    let mut secs = unix.rem_euclid(secs_per_day);
    let hh = secs / 3600;
    secs -= hh * 3600;
    let mm = secs / 60;
    let ss = secs - mm * 60;

    // 1970-01-01 is day 0 in the proleptic Gregorian calendar shifted to that
    // epoch. Algorithm from Howard Hinnant's date library, adapted inline so
    // we don't pull in `chrono`/`time` just for a CLI timestamp.
    let z = total_days + 719_468;
    let era = if z >= 0 { z } else { z - days_per_400y + 1 } / days_per_400y;
    let doe = z - era * days_per_400y;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / (days_per_400y - 1)) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

/// Format `since_ts` -> "Nh", "Nd" relative-to-now. Returns "—" if `ts == 0`.
#[allow(dead_code)]
pub fn rel_ago(ts: i64) -> String {
    if ts <= 0 {
        return "—".into();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let d = (now - ts).max(0);
    if d < 60 {
        format!("{d}s")
    } else if d < 3600 {
        format!("{}m", d / 60)
    } else if d < 86_400 {
        format!("{}h", d / 3600)
    } else {
        format!("{}d", d / 86_400)
    }
}

/// 21-cell unicode bar capped at the given max. `max == 0` renders empty.
pub fn bar(value: u64, max: u64) -> String {
    const WIDTH: u64 = 21;
    if max == 0 {
        return " ".repeat(WIDTH as usize);
    }
    let filled = ((value * WIDTH) / max).min(WIDTH);
    let empty = WIDTH - filled;
    format!(
        "{}{}",
        "█".repeat(filled as usize),
        "░".repeat(empty as usize)
    )
}

/// Draw a single horizontal rule of dashes of the given total width with a
/// short title embedded near the start: `─── title ─────────`.
pub fn rule_titled(title: &str, width: usize) -> String {
    let prefix = "─── ";
    let suffix_start = prefix.chars().count() + title.chars().count() + 1; // " " after title
    let dashes = width.saturating_sub(suffix_start);
    format!("{prefix}{title} {}", "─".repeat(dashes))
}

/// Plain rule of dashes.
pub fn rule(width: usize) -> String {
    "─".repeat(width)
}

/// Box-draw helper used by login + step-up. Pads each line to `inner_w` and
/// frames the result. Lines may contain ANSI escapes; they are *not*
/// width-counted (we trust the caller to compute visible width separately).
pub fn boxed(title: &str, lines: &[String], inner_w: usize) -> String {
    let mut out = String::new();
    let top_dashes = inner_w.saturating_sub(title.chars().count() + 2); // "─ " + " "
    out.push_str(&format!("┌─ {title} {}┐\n", "─".repeat(top_dashes)));
    for l in lines {
        let visible = strip_ansi_len(l);
        let pad = inner_w.saturating_sub(visible);
        out.push_str(&format!("│ {l}{}│\n", " ".repeat(pad)));
    }
    out.push_str(&format!("└{}┘", "─".repeat(inner_w + 1)));
    out
}

/// Visible length, ignoring ANSI escape sequences. Used for box padding.
fn strip_ansi_len(s: &str) -> usize {
    let mut count = 0usize;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip CSI sequence up to and including the final byte (a..z, A..Z).
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            count += 1;
        }
    }
    count
}
