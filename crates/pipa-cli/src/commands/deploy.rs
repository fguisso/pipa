//! `pipa deploy <dir>` — zip the directory and POST it.
//!
//! Zipping happens in `spawn_blocking` so the runtime isn't starved by IO.
//! A spinner indicates progress; for directories with many files we switch
//! to a counted progress bar.

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
use crate::commands::client_with_access;
use crate::output::{check, human_bytes, kv};

const PROGRESS_THRESHOLD_FILES: usize = 100;

pub async fn run(args: DeployArgs) -> Result<()> {
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

    let scope = if let Some(u) = args.uuid.as_deref() {
        format!("deploy:{u}")
    } else {
        "deploy:new".into()
    };
    let (client, _server, access) = client_with_access(&scope).await?;

    let pb = if entries.len() >= PROGRESS_THRESHOLD_FILES {
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

    println!("  archive ready: {} ({} files)", human_bytes(archive.len() as u64), entries.len());

    let params = DeployParams {
        uuid: args.uuid.clone(),
        mode: args.mode.clone(),
        name: args.name.clone(),
        access: args.access.clone(),
        zone: args.zone.clone(),
        password: args.password.clone(),
        csp: args.csp.clone(),
    };

    let upload_pb = ProgressBar::new_spinner();
    upload_pb.set_style(
        ProgressStyle::with_template("  {spinner} {wide_msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    upload_pb.enable_steady_tick(Duration::from_millis(80));
    upload_pb.set_message("uploading…");

    let resp = client
        .deploy_archive(&access, archive, params)
        .await
        .context("deploy POST")?;

    upload_pb.finish_and_clear();

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
