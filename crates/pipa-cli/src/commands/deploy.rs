//! `pipa deploy <dir>` — zip the directory and POST it.
//!
//! Zipping happens in `spawn_blocking` so the runtime isn't starved by IO.
//! A spinner indicates progress (drawn to stderr); for directories with many
//! files we switch to a counted progress bar. With --json, only the final
//! result object is written to stdout.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use pipa_sdk::DeployParams;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

use crate::cli::DeployArgs;
use crate::commands::{client_with_access, ensure_feature};
use crate::manifest;
use crate::output::{check, human_bytes, kv};

const PROGRESS_THRESHOLD_FILES: usize = 100;

pub async fn run(args: DeployArgs, json: bool) -> Result<()> {
    let dir = args.dir.canonicalize().with_context(|| {
        format!("resolving deploy directory `{}`", args.dir.display())
    })?;
    if !dir.is_dir() {
        bail!("`{}` is not a directory", dir.display());
    }

    let entries = collect_entries(&dir)?;
    if entries.is_empty() {
        bail!("`{}` contains no files to deploy", dir.display());
    }

    // Resolve which page this deploy targets. Precedence: explicit --uuid, then
    // the page remembered for this directory (unless --new forces fresh), else
    // a brand-new page.
    let mut manifest = manifest::load();
    let (target_uuid, from_manifest) = if let Some(u) = args.uuid.clone() {
        (Some(u), false)
    } else if args.new {
        (None, false)
    } else if let Some(entry) = manifest.get(&dir) {
        (Some(entry.uuid.clone()), true)
    } else {
        (None, false)
    };

    if from_manifest && !json {
        // `from_manifest` implies a resolved uuid; `unwrap_or_default` is just
        // belt-and-suspenders.
        let u = target_uuid.as_deref().unwrap_or_default();
        println!("  updating remembered page {u} (pass --new to create a fresh page)");
    }

    let scope = match target_uuid.as_deref() {
        Some(u) => format!("deploy:{u}"),
        None => "deploy:new".into(),
    };
    let (client, _server, access) = client_with_access(&scope).await?;

    // `--zone` only matters if the server enforces the `zone` feature; refuse
    // up front otherwise (unless --force) so a value the server would ignore
    // can't give a false sense of security.
    if args.zone.is_some() {
        ensure_feature(&client, &access, "zone", "--zone", args.force).await?;
    }

    let pb = if json {
        None
    } else if entries.len() >= PROGRESS_THRESHOLD_FILES {
        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("  zipping {bar:32.cyan/blue} {pos}/{len} {wide_msg}")
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏ "),
        );
        Some(pb)
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("  {spinner} {wide_msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message("zipping…");
        Some(pb)
    };

    let archive = {
        let dir = dir.clone();
        let entries = entries.clone();
        let pb = pb.clone();
        tokio::task::spawn_blocking(move || build_zip(&dir, &entries, pb.as_ref()))
            .await
            .context("zip task join")??
    };
    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    if !json {
        println!(
            "  archive ready: {} ({} files)",
            human_bytes(archive.len() as u64),
            entries.len()
        );
    }

    // Active workspace for new pages: explicit --workspace wins, else the one
    // set via `pipa workspace use`. Ignored server-side when updating.
    let workspace = args
        .workspace
        .clone()
        .or_else(|| crate::config::load().active_workspace);

    let params = DeployParams {
        uuid: target_uuid.clone(),
        mode: args.mode.clone(),
        name: args.name.clone(),
        access: args.access.clone(),
        zone: args.zone.clone(),
        password: args.password.clone(),
        csp: args.csp.clone(),
        workspace,
    };

    let upload_pb = if json {
        None
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("  {spinner} {wide_msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message("uploading…");
        Some(pb)
    };

    // Keep a copy for the auto-retry path only when the target came from the
    // manifest — the remembered page may have been deleted server-side since.
    // Explicit --uuid / --new deploys don't pay this clone.
    let archive_backup = if from_manifest {
        Some(archive.clone())
    } else {
        None
    };

    let resp = match client.deploy_archive(&access, archive, params.clone()).await {
        Ok(r) => r,
        Err(e) if e.is_code("page_not_found") => {
            match (archive_backup, target_uuid.as_deref()) {
                (Some(backup), Some(stale)) => {
                    // Stale manifest entry: the remembered page is gone. Forget
                    // it and transparently deploy a fresh page instead of
                    // dumping an error the user has to decode.
                    manifest.forget(&dir);
                    let _ = manifest.save();
                    if !json {
                        println!(
                            "  remembered page {stale} no longer exists on the server — creating a new page"
                        );
                    }
                    let (client2, _s2, access2) = client_with_access("deploy:new").await?;
                    let mut fresh = params.clone();
                    fresh.uuid = None;
                    client2
                        .deploy_archive(&access2, backup, fresh)
                        .await
                        .context("deploy (new page after stale manifest)")?
                }
                _ => {
                    if let Some(pb) = upload_pb {
                        pb.finish_and_clear();
                    }
                    bail!(
                        "no page `{}` on the server.\n  \
                         → run `pipa deploy {}` (without --uuid) to create a new page\n  \
                         → run `pipa ls` to see the pages that exist",
                        target_uuid.as_deref().unwrap_or(""),
                        dir.display()
                    );
                }
            }
        }
        Err(e) => return Err(anyhow::Error::new(e).context("deploy failed")),
    };

    if let Some(pb) = upload_pb {
        pb.finish_and_clear();
    }

    // Remember this directory → page so the next `pipa deploy <dir>` updates it
    // instead of creating a duplicate. Non-fatal if it can't be persisted.
    manifest.remember(&dir, resp.uuid.clone(), resp.url.clone());
    if let (Err(e), false) = (manifest.save(), json) {
        eprintln!("  (note: couldn't update deploy manifest: {e})");
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }

    println!("{} deployed", check());
    println!("{}", kv("uuid", &resp.uuid));
    println!("{}", kv("url", &resp.url));
    println!("{}", kv("size", &human_bytes(resp.size_bytes)));
    println!("{}", kv("files", &resp.file_count.to_string()));
    println!("{}", kv("mode", &resp.mode));
    println!("{}", kv("access", &resp.access));
    println!("{}", kv("zone", &resp.zone));
    println!("{}", kv("csp", &resp.csp));
    Ok(())
}

fn collect_entries(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.context("walking deploy dir")?;
        if entry.file_type().is_file() {
            out.push(entry.into_path());
        }
    }
    Ok(out)
}

fn build_zip(root: &Path, entries: &[PathBuf], pb: Option<&ProgressBar>) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024 * 64);
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let opts: SimpleFileOptions =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut file_buf: Vec<u8> = Vec::with_capacity(64 * 1024);
        for path in entries {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| anyhow::anyhow!("path outside root: {}", path.display()))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.is_empty() {
                continue;
            }
            zip.start_file(&rel_str, opts)?;
            let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
            file_buf.clear();
            f.read_to_end(&mut file_buf)?;
            zip.write_all(&file_buf)?;
            if let Some(pb) = pb {
                pb.inc(1);
                pb.set_message(rel_str);
            }
        }
        zip.finish()?;
    }
    Ok(buf)
}
