//! Photo folder scanning & indexing (Sprint 2).
//!
//! The core ([run_scan]) is Tauri-free: it takes a `&Db`, a progress
//! callback and a cancel flag, so it is unit- and integration-testable
//! without a webview. The Tauri command layer (`commands/scan.rs`) adapts it
//! to background tasks + IPC events.
//!
//! Ingestion rules:
//! - Recursively walks the root; hidden dot-directories are skipped.
//! - Only recognized photo types are indexed (see classifiers below);
//!   everything else is counted and ignored.
//! - Re-scanning is idempotent: `photos.path` is unique, rows upsert.
//! - Files that vanished between enumerate and index are reported, not fatal.
//! - One session per imported folder (name = folder name).
//!
//! RAW (and HEIC) files ARE indexed — they are the photographer's photos —
//! but pixel dimensions stay NULL until a decode/EXIF provider fills them
//! (EXIF pass is Sprint 5; preview provider isolated per IMAGE_ANALYSIS.md).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::{DateTime, Utc};
use image::image_dimensions;
use walkdir::WalkDir;

use crate::database::{Db, PhotoUpsert};
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

/// Extensions decoded by the `image` crate in this build.
pub const DECODABLE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "tif", "tiff"];
/// RAW formats: indexed now, decode provider arrives later (IMAGE_ANALYSIS.md).
pub const RAW_EXT: &[&str] = &["cr2", "cr3", "nef", "arw", "raf", "dng", "orf", "rw2"];
/// Apple formats: indexed, no local preview in v0.1 (placeholder tile).
pub const HEIC_EXT: &[&str] = &["heic", "heif"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    Decodable,
    Raw,
    Heic,
    Ignored,
}

pub fn classify_extension(ext: &str) -> FileClass {
    let e = ext.to_ascii_lowercase();
    if DECODABLE_EXT.contains(&e.as_str()) {
        FileClass::Decodable
    } else if RAW_EXT.contains(&e.as_str()) {
        FileClass::Raw
    } else if HEIC_EXT.contains(&e.as_str()) {
        FileClass::Heic
    } else {
        FileClass::Ignored
    }
}

pub fn derive_orientation(w: Option<i64>, h: Option<i64>) -> Option<&'static str> {
    match (w, h) {
        (Some(w), Some(h)) if w > h => Some("landscape"),
        (Some(w), Some(h)) if w < h => Some("portrait"),
        (Some(_), Some(_)) => Some("square"),
        _ => None,
    }
}

fn mtime_iso(meta: &std::fs::Metadata) -> Option<String> {
    meta.modified()
        .ok()
        .map(|t| DateTime::<Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanSummary {
    pub session_id: i64,
    pub session_name: String,
    pub total_files: usize,
    pub indexed: usize,
    pub ignored: usize,
    pub errors: Vec<String>,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

/// Run a full scan of `root` into `db`.
///
/// `progress` is invoked at coarse granularity (every 25 files and at pass
/// boundaries) — callers forward it to IPC events. `cancel` checked between
/// files: the scan stops at the next boundary and reports `cancelled`.
pub fn run_scan(
    root: &Path,
    db: &Db,
    progress: &mut dyn FnMut(ProgressPayload),
    cancel: &AtomicBool,
) -> AppResult<ScanSummary> {
    let started = Instant::now();

    if !root.is_dir() {
        return Err(AppError::validation(format!("Folder does not exist: {}", root.display())));
    }

    // One session per imported folder.
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| root.display().to_string());
    let session_id = db.upsert_session(&name, Some(&root.to_string_lossy()))?;

    // ---- Pass 1: enumerate (metadata only) ----
    let mut candidates = Vec::new();
    let mut total_files = 0usize;
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                return Err(AppError::operation(format!(
                    "Could not read folder {}: {e}",
                    root.display()
                )))
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        // Skip hidden dot-directories (bookkeeping, OS metadata, dot-folders
        // the photographer didn't import). The root folder itself is exempt.
        if let Ok(rel) = entry.path().strip_prefix(root) {
            if rel
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            {
                continue;
            }
        }
        total_files += 1;
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(classify_extension(ext), FileClass::Ignored) {
            continue;
        }
        candidates.push(path.to_path_buf());
    }
    let ignored = total_files - candidates.len();
    let total = candidates.len();
    progress(
        ProgressPayload::new(total, 0, "discovering")
            .with_current(root.display().to_string()),
    );

    // ---- Pass 2: index each candidate ----
    let mut indexed = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut cancelled = false;

    for (i, path) in candidates.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let Some(meta) = std::fs::symlink_metadata(path).ok() else {
            push_err(&mut errors, &format!("File no longer exists: {}", path.display()));
            continue;
        };

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let class = classify_extension(&ext);
        // Header-only read: cheap even for 10k folders. Never decode RAW/HEIC.
        let (width, height) = if class == FileClass::Decodable {
            match image_dimensions(path) {
                Ok((w, h)) => (Some(i64::from(w)), Some(i64::from(h))),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "could not read image header");
                    push_err(
                        &mut errors,
                        &format!("Could not read image: {}", path.file_name().unwrap_or_default().to_string_lossy()),
                    );
                    (None, None)
                }
            }
        } else {
            (None, None) // RAW/HEIC: dimensions arrive with the EXIF/decode provider
        };

        db.upsert_photo(&PhotoUpsert {
            path: path.to_string_lossy().into_owned(),
            filename: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            extension: ext,
            size_bytes: Some(meta.len() as i64),
            width,
            height,
            orientation: derive_orientation(width, height).map(str::to_string),
            session_id,
            file_mtime: mtime_iso(&meta),
        })?;
        indexed += 1;

        if i % 25 == 0 || i + 1 == total {
            let current = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned());
            progress(
                ProgressPayload::new(total, i + 1, "indexing")
                    .with_current(current.unwrap_or_default()),
            );
        }
    }

    if !cancelled {
        db.refresh_session_counts(session_id)?;
        progress(ProgressPayload::new(total, total, "done"));
    }

    let summary = ScanSummary {
        session_id,
        session_name: name.clone(),
        total_files,
        indexed,
        ignored,
        errors,
        cancelled,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };
    tracing::info!(
        root = %root.display(),
        session = %name,
        total = summary.total_files,
        indexed = summary.indexed,
        ignored = summary.ignored,
        errors = summary.errors.len(),
        cancelled,
        ms = summary.elapsed_ms,
        "scan finished"
    );
    Ok(summary)
}

fn push_err(errors: &mut Vec<String>, msg: &str) {
    if errors.len() < 20 {
        errors.push(msg.to_string());
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_extensions_case_insensitively() {
        assert_eq!(classify_extension("jpg"), FileClass::Decodable);
        assert_eq!(classify_extension("JPG"), FileClass::Decodable);
        assert_eq!(classify_extension("jpeg"), FileClass::Decodable);
        assert_eq!(classify_extension("png"), FileClass::Decodable);
        assert_eq!(classify_extension("webp"), FileClass::Decodable);
        assert_eq!(classify_extension("tif"), FileClass::Decodable);
        assert_eq!(classify_extension("TIF"), FileClass::Decodable);
        assert_eq!(classify_extension("cr2"), FileClass::Raw);
        assert_eq!(classify_extension("CR3"), FileClass::Raw);
        assert_eq!(classify_extension("nef"), FileClass::Raw);
        assert_eq!(classify_extension("arw"), FileClass::Raw);
        assert_eq!(classify_extension("dng"), FileClass::Raw);
        assert_eq!(classify_extension("heic"), FileClass::Heic);
        assert_eq!(classify_extension("HEIF"), FileClass::Heic);
        assert_eq!(classify_extension("txt"), FileClass::Ignored);
        assert_eq!(classify_extension("mp4"), FileClass::Ignored);
        assert_eq!(classify_extension(""), FileClass::Ignored);
    }

    #[test]
    fn derives_orientation() {
        assert_eq!(derive_orientation(Some(100), Some(80)), Some("landscape"));
        assert_eq!(derive_orientation(Some(80), Some(100)), Some("portrait"));
        assert_eq!(derive_orientation(Some(100), Some(100)), Some("square"));
        assert_eq!(derive_orientation(None, Some(100)), None);
        assert_eq!(derive_orientation(None, None), None);
    }

    #[test]
    fn upsert_photo_is_idempotent_and_safe() {
        let dir = std::env::temp_dir().join(format!("pg_scan_upsert_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let dbp = dir.join("t.sqlite");
        let _ = std::fs::remove_file(&dbp);
        let db = Db::open(&dbp).unwrap();
        db.migrate().unwrap();
        let s = db.upsert_session("Test", Some("/tmp/test")).unwrap();

        let mk = |w: Option<i64>, mtime: &str| PhotoUpsert {
            path: "/tmp/x/IMG_0001.jpg".into(),
            filename: "IMG_0001.jpg".into(),
            extension: "jpg".into(),
            size_bytes: Some(42),
            width: w,
            height: w,
            orientation: derive_orientation(w, w).map(str::to_string),
            session_id: s,
            file_mtime: Some(mtime.into()),
        };

        db.upsert_photo(&mk(Some(640), "2026-08-01T00:00:00Z")).unwrap();
        // Re-scan with no dimensions must not blank the earlier values.
        db.upsert_photo(&mk(None, "2026-08-16T00:00:00Z")).unwrap();

        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM photos WHERE path = ?1", ["/tmp/x/IMG_0001.jpg"], |r| r.get(0)).unwrap();
        let (w, mtime): (Option<i64>, Option<String>) = conn
            .query_row("SELECT width, file_mtime FROM photos WHERE path = ?1", ["/tmp/x/IMG_0001.jpg"], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        drop(conn);

        assert_eq!(count, 1);
        assert_eq!(w, Some(640));
        assert_eq!(mtime.as_deref(), Some("2026-08-16T00:00:00Z"));
        let _ = std::fs::remove_file(&dbp);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn session_upserts_by_root() {
        let dir = std::env::temp_dir().join(format!("pg_scan_sess_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let dbp = dir.join("t.sqlite");
        let _ = std::fs::remove_file(&dbp);
        let db = Db::open(&dbp).unwrap();
        db.migrate().unwrap();

        let a = db.upsert_session("Wedding", Some("/photos/wedding")).unwrap();
        let b = db.upsert_session("Wedding Renamed", Some("/photos/wedding")).unwrap();
        assert_eq!(a, b);
        let row = db.session_by_id(a).unwrap().unwrap();
        assert_eq!(row.name, "Wedding Renamed");
        let manual = db.upsert_session("Manual", None).unwrap();
        assert_ne!(manual, a);
        let _ = std::fs::remove_file(&dbp);
        let _ = std::fs::remove_dir(&dir);
    }
}
