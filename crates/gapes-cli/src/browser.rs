//! Best-effort "open this URL in the user's default browser." Falls through
//! silently when no opener is available — callers must always also print the
//! URL so the user can copy/paste it (this is a UX nicety, not a guarantee).

use std::process::{Command, Stdio};

/// Try to open `url` in the platform's default browser. Returns `true` if the
/// spawn succeeded; the spawned process is detached and we do not wait for it.
/// macOS: `open`. Linux/BSD: `xdg-open`. Windows: `cmd /C start ""`.
pub fn try_open(url: &str) -> bool {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };

    Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}
