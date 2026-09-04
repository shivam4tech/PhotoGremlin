//! File operations (Sprint 7): group rename with a template engine, move,
//! copy, trash, and explicitly-confirmed permanent deletion — behind the
//! universal safety protocol (FILE_OPERATIONS.md):
//! verify sources, resolve destinations, detect collisions, preview before
//! touching disk, execute item-by-item with per-item results, audit everything.
//!
//! This module is Tauri-independent: it takes a `&Db` plus plain paths and
//! reports results, so the exact pipeline is integration-tested on real temp
//! directories.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::{DateTime, Utc};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

pub const OP_RENAME: &str = "rename";
pub const OP_MOVE: &str = "move";
pub const OP_COPY: &str = "copy";
pub const OP_TRASH: &str = "trash";
pub const OP_DELETE_PERMANENTLY: &str = "delete-permanently";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Move,
    Copy,
}

impl OpKind {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "move" => Ok(OpKind::Move),
            "copy" => Ok(OpKind::Copy),
            other => Err(AppError::validation(format!(
                "Unknown operation: {other} (use \"move\" or \"copy\")"
            ))),
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            OpKind::Move => OP_MOVE,
            OpKind::Copy => OP_COPY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// Block the item, never overwrite.
    Skip,
    /// User explicitly chose "avoid by renaming": `IMG_0001-1.jpg`, `-2`, …
    AvoidByRenaming,
}

impl CollisionPolicy {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "skip" => Ok(CollisionPolicy::Skip),
            "avoid-by-renaming" => Ok(CollisionPolicy::AvoidByRenaming),
            other => Err(AppError::validation(format!(
                "Unknown collision policy: {other}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Preview / result types (serialized over IPC)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanItem {
    pub photo_id: i64,
    pub source: String,
    /// Where the file would go. `None` for trash or permanent deletion.
    pub destination: Option<String>,
    /// Per-item note: "ALREADY EXISTS", "file no longer exists", why a plan
    /// was aborted, …
    pub note: Option<String>,
    pub ok: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileOpPlan {
    /// "rename" | "move" | "copy" | "trash" | "delete-permanently"
    pub op: &'static str,
    pub items: Vec<PlanItem>,
    /// True when an in-plan collision (two sources onto one name) aborts the
    /// whole plan; the `items` notes carry the itemized report.
    pub aborted: bool,
    /// Destination directory that does not exist yet (move/copy): the UI
    /// shows it in the preview before anything is created.
    pub will_create_dir: Option<String>,
    /// Destructive operations require an explicit confirmation in the UI.
    /// Everything else proceeds after the preview.
    pub destructive: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OperationItemResult {
    pub source: String,
    pub destination: Option<String>,
    /// "done" | "failed" | "skipped" | "cancelled"
    pub status: String,
    pub detail: Option<String>,
}

/// Payload for the `operation-complete` event.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OperationSummary {
    pub op: &'static str,
    pub total: usize,
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// Per-item outcomes; capped so a giant operation cannot ship an
    /// unbounded payload (the full detail lives in the local log).
    pub items: Vec<OperationItemResult>,
}

const MAX_REPORTED_ITEMS: usize = 500;

// ---------------------------------------------------------------------------
// Rename template engine (pure — unit tested)
// ---------------------------------------------------------------------------

/// Filesystem-safe name. Path separators, control chars, spaces, literal
/// braces and the characters reserved by Win/mac/Linux all collapse to a
/// single `-`; dangling separators at either end are trimmed; empty results
/// become "renamed"; length is capped so paths stay sane. Underscores are
/// kept (they are the norm in camera filenames).
pub fn sanitize_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_sep = true;
    for c in s.chars() {
        let is_sep = c.is_control()
            || c == ' '
            || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '{' | '}');
        if is_sep {
            if !last_sep {
                out.push('-');
            }
            last_sep = true;
        } else {
            out.push(c);
            last_sep = false;
        }
    }
    let trimmed = out.trim_matches(|c| matches!(c, '.' | ' ' | '-' | '_'));
    if trimmed.is_empty() {
        "renamed".to_string()
    } else {
        trimmed.chars().take(150).collect()
    }
}

const TOKENS: [&str; 9] = [
    "{sequence}",
    "{original}",
    "{camera}",
    "{lens}",
    "{focal}",
    "{name}",
    "{date}",
    "{time}",
    "{iso}",
];

/// Single-pass token expansion: each `{token}` is replaced by `value(token)`.
/// Text coming from earlier values is never re-scanned, so an original name
/// that literally contains `{date}` cannot double-expand. Unknown tokens stay
/// as-is (they then get sanitized).
pub fn expand_template(template: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(idx) = rest.find('{') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 1..];
        let mut matched = None;
        for t in TOKENS {
            // t is of the form "{name}"; match the inner name then a '}'.
            let inner = &t[1..t.len() - 1];
            if let Some(tail) = after.strip_prefix(inner) {
                if let Some(closing) = tail.strip_prefix('}') {
                    matched = Some((t, closing));
                    break;
                }
            }
        }
        match matched {
            Some((token, tail)) => {
                if let Some(v) = value(token) {
                    out.push_str(&v);
                }
                rest = tail;
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Best-known time of a photo for date/time tokens: capture datetime, then
/// file mtime, then the index time (always present).
fn source_time(
    capture: Option<&str>,
    mtime: Option<&str>,
    indexed_at: &str,
) -> Option<DateTime<Utc>> {
    capture
        .and_then(crate::time::parse_opt)
        .or_else(|| mtime.and_then(crate::time::parse_opt))
        .or_else(|| crate::time::parse_opt(indexed_at))
}

/// Rename target for one photo: the template expanded with that photo's
/// values, sanitized, re-attached with its original extension, in the
/// photo's own directory.
fn rename_target(
    dir: &Path,
    template: &str,
    group_name: &str,
    extension: &str,
    original: &str,
    capture: Option<&str>,
    mtime: Option<&str>,
    indexed_at: &str,
    camera_model: Option<&str>,
    lens: Option<&str>,
    focal: Option<f64>,
    iso: Option<i64>,
    sequence: u32,
    sequence_width: usize,
) -> String {
    // `{original}` is the filename *without* its extension.
    let original_stem = if extension.is_empty() || !original.ends_with(&format!(".{extension}")) {
        original.to_string()
    } else {
        original[..original.len() - extension.len() - 1].to_string()
    };
    let lookup = |token: &str| -> Option<String> {
        match token {
            "{name}" => Some(sanitize_name(group_name)),
            "{original}" => Some(original_stem.clone()),
            "{camera}" => camera_model.map(sanitize_name),
            "{lens}" => lens.map(sanitize_name),
            "{focal}" => focal.map(|f| format!("{}", f.round() as i64)),
            "{iso}" => iso.map(|i| i.to_string()),
            "{sequence}" => Some(format!("{sequence:0width$}", width = sequence_width)),
            "{date}" => source_time(capture, mtime, indexed_at).map(|d| d.format("%Y-%m-%d").to_string()),
            "{time}" => source_time(capture, mtime, indexed_at).map(|d| d.format("%H-%M-%S").to_string()),
            _ => None,
        }
    };
    let expanded = sanitize_name(&expand_template(template, &lookup));
    // The extension is always the file's own: strip any trailing ".ext" the
    // user typed into the template and re-attach the original's. This keeps
    // the result a valid, scannable image filename no matter what the
    // template says.
    let stem = match expanded.rfind('.') {
        Some(pos) if pos > 0 => &expanded[..pos],
        _ => expanded.as_str(),
    };
    let stem = if stem.is_empty() { "renamed" } else { stem };
    let filename = if extension.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{extension}")
    };
    dir.join(filename).display().to_string()
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

struct PhotoRow {
    id: i64,
    path: String,
    filename: String,
    extension: String,
    width: Option<i64>,
    height: Option<i64>,
    orientation: Option<String>,
    capture_datetime: Option<String>,
    indexed_at: String,
    file_mtime: Option<String>,
    camera_model: Option<String>,
    lens: Option<String>,
    focal_length: Option<f64>,
    iso: Option<i64>,
}

/// Deterministic order: capture time (unknowns first), filename, id — the
/// same order the grid shows, so a sequence reads like the shelf.
fn load_photos(db: &Db, ids: &[i64]) -> AppResult<Vec<PhotoRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db.lock()?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let params: Vec<Box<dyn rusqlite::ToSql>> =
        ids.iter().map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>).collect();
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, path, filename, extension, width, height, orientation,
                    capture_datetime, indexed_at, file_mtime, camera_model, lens,
                    focal_length, iso
             FROM photos
             WHERE id IN ({placeholders})
             ORDER BY (capture_datetime IS NOT NULL), COALESCE(capture_datetime, indexed_at),
                      filename, id"
        ))
        .map_err(db_err("prepare photo rows"))?;
    let rows = stmt
        .query_map(refs.as_slice(), |r| {
            Ok(PhotoRow {
                id: r.get(0)?,
                path: r.get(1)?,
                filename: r.get(2)?,
                extension: r.get(3)?,
                width: r.get(4)?,
                height: r.get(5)?,
                orientation: r.get(6)?,
                capture_datetime: r.get(7)?,
                indexed_at: r.get(8)?,
                file_mtime: r.get(9)?,
                camera_model: r.get(10)?,
                lens: r.get(11)?,
                focal_length: r.get(12)?,
                iso: r.get(13)?,
            })
        })
        .map_err(db_err("query photo rows"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(db_err("read photo row"))?);
    }
    Ok(out)
}

fn missing(source: &str) -> PlanItem {
    PlanItem {
        photo_id: 0,
        source: source.to_string(),
        destination: None,
        note: Some("file no longer exists".to_string()),
        ok: false,
    }
}

fn with_id(mut item: PlanItem, id: i64) -> PlanItem {
    item.photo_id = id;
    item
}

/// Group rename plan. In-plan collisions (two sources mapping to the same
/// destination) abort the whole plan with an itemized report; on-disk
/// collisions block the individual item.
pub fn plan_rename(
    db: &Db,
    photo_ids: &[i64],
    template: &str,
    group_name: &str,
) -> AppResult<FileOpPlan> {
    if photo_ids.is_empty() {
        return Err(AppError::validation("Nothing selected to rename".to_string()));
    }
    if template.trim().is_empty() {
        return Err(AppError::validation("The rename template is empty".to_string()));
    }
    let photos = load_photos(db, photo_ids)?;
    let total = photos.len() as u32;
    let width = 3.max(total.to_string().len());

    let mut items: Vec<PlanItem> = Vec::with_capacity(photos.len());
    for (idx, p) in photos.iter().enumerate() {
        if !Path::new(&p.path).exists() {
            items.push(with_id(missing(&p.path), p.id));
            continue;
        }
        let dest = rename_target(
            Path::new(&p.path).parent().unwrap_or_else(|| Path::new(".")),
            template,
            group_name,
            &p.extension,
            &p.filename,
            p.capture_datetime.as_deref(),
            p.file_mtime.as_deref(),
            &p.indexed_at,
            p.camera_model.as_deref(),
            p.lens.as_deref(),
            p.focal_length,
            p.iso,
            idx as u32 + 1,
            width,
        );
        items.push(PlanItem {
            photo_id: p.id,
            source: p.path.clone(),
            destination: Some(dest),
            note: None,
            ok: true,
        });
    }

    // In-plan collision: two items onto one destination aborts the plan.
    let mut aborted = false;
    let mut by_dest: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        if it.ok {
            if let Some(d) = &it.destination {
                by_dest.entry(d.clone()).or_default().push(i);
            }
        }
    }
    for (dest, idxs) in &by_dest {
        if idxs.len() > 1 {
            aborted = true;
            for &i in idxs {
                items[i].ok = false;
                items[i].note = Some(format!(
                    "maps to the same name as another selected item ({dest}) — plan aborted"
                ));
            }
        }
    }

    // On-disk collisions block the item (rename has no safe-suffix option).
    if !aborted {
        for it in items.iter_mut().filter(|i| i.ok) {
            let src = Path::new(&it.source);
            let dst = it.destination.as_ref().map(Path::new);
            if let Some(d) = dst {
                if d.exists() && d != src {
                    it.ok = false;
                    it.note = Some("ALREADY EXISTS".to_string());
                }
            }
        }
    }

    Ok(FileOpPlan {
        op: OP_RENAME,
        items,
        aborted,
        will_create_dir: None,
        destructive: false,
    })
}

/// Move/copy plan. A missing destination directory is reported (the executor
/// creates it); collisions follow the chosen policy.
pub fn plan_move_copy(
    db: &Db,
    photo_ids: &[i64],
    dest_dir: &Path,
    op: OpKind,
    policy: CollisionPolicy,
) -> AppResult<FileOpPlan> {
    if photo_ids.is_empty() {
        return Err(AppError::validation("Nothing selected".to_string()));
    }
    let will_create_dir = if dest_dir.is_dir() {
        None
    } else if dest_dir.parent().map(|p| p.is_dir()).unwrap_or(false) {
        Some(dest_dir.display().to_string())
    } else {
        return Err(AppError::validation(format!(
            "Destination does not exist and its parent folder is missing: {}",
            dest_dir.display()
        )));
    };
    let photos = load_photos(db, photo_ids)?;
    let mut items = Vec::with_capacity(photos.len());
    for p in photos.iter() {
        if !Path::new(&p.path).exists() {
            items.push(with_id(missing(&p.path), p.id));
            continue;
        }
        let mut dest = dest_dir.join(&p.filename);
        let mut note = None;
        let mut ok = true;
        if dest.exists() {
            match policy {
                CollisionPolicy::Skip => {
                    ok = false;
                    note = Some("ALREADY EXISTS — skipped".to_string());
                }
                CollisionPolicy::AvoidByRenaming => {
                    dest = suffixed_path(&dest);
                    note = Some("renamed to avoid a collision".to_string());
                }
            }
        }
        items.push(PlanItem {
            photo_id: p.id,
            source: p.path.clone(),
            destination: Some(dest.display().to_string()),
            note,
            ok,
        });
    }
    Ok(FileOpPlan {
        op: op.tag(),
        items,
        aborted: false,
        will_create_dir,
        destructive: false,
    })
}

/// Trash plan: sources only; the destination is the OS trash, resolved per
/// item at execution time. Always destructive (requires confirmation).
pub fn plan_trash(db: &Db, photo_ids: &[i64]) -> AppResult<FileOpPlan> {
    if photo_ids.is_empty() {
        return Err(AppError::validation("Nothing selected to trash".to_string()));
    }
    let photos = load_photos(db, photo_ids)?;
    let mut items = Vec::with_capacity(photos.len());
    for p in photos.iter() {
        if !Path::new(&p.path).exists() {
            items.push(with_id(missing(&p.path), p.id));
        } else {
            items.push(PlanItem {
                photo_id: p.id,
                source: p.path.clone(),
                destination: None,
                note: None,
                ok: true,
            });
        }
    }
    Ok(FileOpPlan {
        op: OP_TRASH,
        items,
        aborted: false,
        will_create_dir: None,
        destructive: true,
    })
}

/// Permanent-delete plan: sources only, with no recoverable destination.
/// The frontend must present this plan and obtain explicit confirmation; the
/// execution command re-plans from database-backed photo IDs before acting.
pub fn plan_permanent_delete(db: &Db, photo_ids: &[i64]) -> AppResult<FileOpPlan> {
    if photo_ids.is_empty() {
        return Err(AppError::validation(
            "Nothing selected to delete permanently".to_string(),
        ));
    }
    let photos = load_photos(db, photo_ids)?;
    let mut items = Vec::with_capacity(photos.len());
    for p in photos.iter() {
        let source = Path::new(&p.path);
        match fs::metadata(source) {
            Ok(metadata) if metadata.is_file() => items.push(PlanItem {
                photo_id: p.id,
                source: p.path.clone(),
                destination: None,
                note: Some("cannot be restored from system trash".to_string()),
                ok: true,
            }),
            Ok(_) => items.push(PlanItem {
                photo_id: p.id,
                source: p.path.clone(),
                destination: None,
                note: Some("not a photo file; directories cannot be deleted".to_string()),
                ok: false,
            }),
            Err(_) => items.push(with_id(missing(&p.path), p.id)),
        }
    }
    Ok(FileOpPlan {
        op: OP_DELETE_PERMANENTLY,
        items,
        aborted: false,
        will_create_dir: None,
        destructive: true,
    })
}

/// `file.txt` → `file-1.txt`, taking the first free suffix.
fn suffixed_path(p: &Path) -> PathBuf {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let stem = if extension.is_empty() {
        name.clone()
    } else {
        name[..name.len() - extension.len()].to_string()
    };
    let mut n = 1u32;
    loop {
        let candidate = p.with_file_name(format!("{stem}-{n}{extension}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
        if n > 10_000 {
            return candidate;
        }
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute a plan item-by-item. The plan is the *preview*; execution
/// re-checks each destination right before acting, so a race (a file
/// appearing between preview and confirm) becomes a per-item failure instead
/// of an overwrite. DB writes + audit happen per successful item.
pub fn run_operation(
    db: &Db,
    plan: &FileOpPlan,
    progress: &mut impl FnMut(ProgressPayload),
    cancel: &AtomicBool,
) -> AppResult<OperationSummary> {
    let started = Instant::now();
    let total = plan.items.len();
    let mut results: Vec<OperationItemResult> = Vec::with_capacity(total);
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut processed = 0usize;
    let mut cancelled = false;

    for (i, item) in plan.items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            results.push(OperationItemResult {
                source: item.source.clone(),
                destination: item.destination.clone(),
                status: "cancelled".into(),
                detail: None,
            });
            continue;
        }
        if !item.ok {
            results.push(OperationItemResult {
                source: item.source.clone(),
                destination: item.destination.clone(),
                status: "skipped".into(),
                detail: item.note.clone(),
            });
            processed += 1;
            continue;
        }
        let outcome = match plan.op {
            OP_RENAME => exec_rename(db, item),
            OP_MOVE => exec_move_copy(db, item, OpKind::Move),
            OP_COPY => exec_move_copy(db, item, OpKind::Copy),
            OP_TRASH => exec_trash(db, item),
            OP_DELETE_PERMANENTLY => exec_permanent_delete(db, item),
            _ => Err(AppError::operation(format!("unknown operation {}", plan.op))),
        };
        processed += 1;
        let (status, detail) = match outcome {
            Ok(dest) => {
                succeeded += 1;
                ("done", dest)
            }
            Err(e) => {
                failed += 1;
                ("failed", Some(e.to_string()))
            }
        };
        results.push(OperationItemResult {
            source: item.source.clone(),
            destination: item.destination.clone(),
            status: status.into(),
            detail,
        });
        progress(
            ProgressPayload::new(total, i + 1, plan.op)
                .with_current(Path::new(&item.source).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| item.source.clone())),
        );
    }

    let summary = OperationSummary {
        op: plan.op,
        total,
        processed,
        succeeded,
        failed,
        cancelled,
        elapsed_ms: started.elapsed().as_millis() as u64,
        items: {
            let mut v = results;
            v.truncate(MAX_REPORTED_ITEMS);
            v
        },
    };
    Ok(summary)
}

fn friendly_io(e: std::io::Error, target: &str) -> AppError {
    match e.kind() {
        std::io::ErrorKind::NotFound => AppError::FileMissing {
            target: target.to_string(),
            reason: "the file no longer exists".into(),
        },
        std::io::ErrorKind::PermissionDenied => AppError::PermissionDenied {
            target: target.to_string(),
            reason: "read or write was denied".into(),
        },
        _ => AppError::operation(format!("Could not access {target}. {e}")),
    }
}

fn path_str(p: &Path) -> String {
    p.display().to_string()
}

/// stat → (size, rfc3339 mtime) for DB bookkeeping.
fn stat_info(p: &Path) -> Option<(Option<i64>, Option<String>)> {
    let meta = fs::metadata(p).ok()?;
    let mtime = meta.modified().ok().map(|t| {
        DateTime::<Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    });
    Some((Some(meta.len() as i64), mtime))
}

fn exec_rename(db: &Db, item: &PlanItem) -> AppResult<Option<String>> {
    let dest = item
        .destination
        .as_ref()
        .ok_or_else(|| AppError::operation("rename item has no destination"))?;
    let src = Path::new(&item.source);
    let dst = Path::new(dest);
    // Re-check right before acting (preview → confirm is not atomic).
    if src.exists() && dst.exists() && src != dst {
        return Err(AppError::validation(format!(
            "Destination appears since the preview: {dest} — nothing was overwritten"
        )));
    }
    fs::rename(src, dst).map_err(|e| friendly_io(e, &item.source))?;
    let (size, mtime) = stat_info(dst).unwrap_or((None, None));
    db.update_photo_path(item.photo_id, dest, &dst.file_name().unwrap_or_default().to_string_lossy(), size, mtime.as_deref())?;
    db.record_file_op(OP_RENAME, &item.source, Some(dest), "done", None)?;
    tracing::info!(op = OP_RENAME, src = %item.source, dst = %dest, "renamed");
    Ok(None)
}

fn exec_move_copy(db: &Db, item: &PlanItem, op: OpKind) -> AppResult<Option<String>> {
    let dest = item
        .destination
        .as_ref()
        .ok_or_else(|| AppError::operation("item has no destination"))?;
    let src = Path::new(&item.source);
    let dst = Path::new(dest);
    if src.exists() && dst.exists() {
        return Err(AppError::validation(format!(
            "Destination appears since the preview: {dest} — nothing was overwritten"
        )));
    }
    if let Some(parent) = dst.parent() {
        let parent_str = path_str(parent);
        fs::create_dir_all(parent).map_err(|e| friendly_io(e, &parent_str))?;
    }
    match op {
        OpKind::Move => {
            move_one(src, dst).map_err(|e| friendly_io(e, &item.source))?;
            let (size, mtime) = stat_info(dst).unwrap_or((None, None));
            db.update_photo_path(
                item.photo_id,
                dest,
                &dst.file_name().unwrap_or_default().to_string_lossy(),
                size,
                mtime.as_deref(),
            )?;
        }
        OpKind::Copy => {
            let written = fs::copy(src, dst).map_err(|e| friendly_io(e, &item.source))?;
            let expected = fs::metadata(src).map_err(|e| friendly_io(e, &item.source))?.len();
            if written != expected {
                let _ = fs::remove_file(dst);
                return Err(AppError::operation(format!(
                    "Copy of {} did not verify (size mismatch) — partial file removed",
                    item.source
                )));
            }
            // The copy is a new library item. It inherits the source's known
            // dimensions; the session is the active library's when the copy
            // lands inside it, otherwise unset.
            let (size, mtime) = stat_info(dst).unwrap_or((Some(expected as i64), None));
            let session_id = match db.get_setting("active_folder")? {
                Some(folder) if dst.parent().map(|d| d.starts_with(&*folder)).unwrap_or(false) => {
                    let f = Path::new(&folder);
                    let name = f
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "library".to_string());
                    db.upsert_session(&name, Some(&folder)).ok()
                }
                _ => None,
            };
            let row = load_photos(db, &[item.photo_id])?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::operation("source row vanished mid-copy"))?;
            db.upsert_photo(&crate::database::PhotoUpsert {
                path: dest.to_string(),
                filename: dst.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                extension: row.extension,
                size_bytes: size,
                width: row.width,
                height: row.height,
                orientation: row.orientation,
                session_id: session_id,
                file_mtime: mtime,
            })?;
        }
    }
    db.record_file_op(op.tag(), &item.source, Some(dest), "done", None)?;
    tracing::info!(op = op.tag(), src = %item.source, dst = %dest, "moved/copied");
    Ok(None)
}

/// Same-filesystem `rename` first (atomic); otherwise staged
/// copy → size verification → delete source. The source is deleted only
/// after the copy verified.
fn move_one(src: &Path, dst: &Path) -> std::io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            let written = fs::copy(src, dst)?;
            let expected = fs::metadata(src)?.len();
            if written != expected {
                let _ = fs::remove_file(dst);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "cross-device move did not verify (size mismatch)",
                ));
            }
            fs::remove_file(src)
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Trash (recoverable OS trash)
// ---------------------------------------------------------------------------

/// The freedesktop XDG trash directories: `<data>/Trash/files` + `info`.
#[cfg(target_os = "linux")]
fn trash_dirs() -> AppResult<(PathBuf, PathBuf)> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))
        .ok_or_else(|| AppError::operation("No home directory for the trash".to_string()))?;
    Ok((data_home.join("Trash/files"), data_home.join("Trash/info")))
}

#[cfg(not(target_os = "linux"))]
fn trash_dirs() -> AppResult<(PathBuf, PathBuf)> {
    Err(AppError::operation(
        "OS trash is only available on Linux in v0.1 — use Move instead".to_string(),
    ))
}

#[cfg(target_os = "linux")]
fn trash_file_linux(src: &Path) -> AppResult<()> {
    let (files, info) = trash_dirs()?;
    fs::create_dir_all(&files).map_err(|e| friendly_io(e, "trash"))?;
    fs::create_dir_all(&info).map_err(|e| friendly_io(e, "trash"))?;

    let original = src
        .canonicalize()
        .map_err(|e| friendly_io(e, &src.display().to_string()))?;
    let name = original
        .file_name()
        .ok_or_else(|| AppError::operation("Cannot trash a root entry".to_string()))?
        .to_os_string();
    let name_s = name.to_string_lossy().into_owned();
    let extension = original
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let stem = if extension.is_empty() {
        name_s.clone()
    } else {
        name_s[..name_s.len() - extension.len()].to_string()
    };

    let mut target = files.join(&name);
    let mut n = 1u32;
    while target.exists() {
        n += 1;
        target = files.join(format!("{stem}-{n}{extension}"));
    }
    let mut info_file = info.join(format!("{name_s}.trashinfo"));
    while info_file.exists() {
        n += 1;
        info_file = info.join(format!("{stem}-{n}{extension}.trashinfo"));
    }

    let moved = move_one(&original, &target);
    if let Err(e) = moved {
        return Err(friendly_io(e, &original.display().to_string()));
    }
    let deletion_date = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    fs::write(
        &info_file,
        format!("[Trash Info]\nPath={}\nDeletionDate={deletion_date}\n", original.display()),
    )
    .map_err(|e| {
        // The file is in the trash; only its metadata failed. Keep the file
        // (it IS restored from the OS trash by name), report the detail.
        tracing::error!(?info_file, error = %e, "trash metadata write failed");
        AppError::operation("File is in the trash; its trash metadata could not be written".to_string())
    })?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn trash_file_linux(src: &Path) -> AppResult<()> {
    let _ = src;
    Err(AppError::operation(
        "OS trash is only available on Linux in v0.1 — use Move instead".to_string(),
    ))
}

fn exec_trash(db: &Db, item: &PlanItem) -> AppResult<Option<String>> {
    let src = Path::new(&item.source);
    if !src.exists() {
        return Err(AppError::FileMissing {
            target: item.source.clone(),
            reason: "the file no longer exists".into(),
        });
    }
    trash_file_linux(src)?;
    db.delete_photos(vec![item.photo_id])?;
    db.record_file_op(OP_TRASH, &item.source, None, "done", None)?;
    tracing::info!(op = OP_TRASH, src = %item.source, "trashed");
    Ok(None)
}

fn exec_permanent_delete(db: &Db, item: &PlanItem) -> AppResult<Option<String>> {
    let src = Path::new(&item.source);
    let metadata = fs::metadata(src).map_err(|e| AppError::io(e, item.source.clone()))?;
    if !metadata.is_file() {
        return Err(AppError::validation(format!(
            "Permanent deletion only accepts indexed photo files: {}",
            item.source
        )));
    }
    fs::remove_file(src).map_err(|e| AppError::io(e, item.source.clone()))?;
    db.delete_photos(vec![item.photo_id])?;
    db.record_file_op(
        OP_DELETE_PERMANENTLY,
        &item.source,
        None,
        "done",
        None,
    )?;
    tracing::info!(op = OP_DELETE_PERMANENTLY, src = %item.source, "permanently deleted");
    Ok(None)
}

fn db_err(context: &str) -> impl FnOnce(rusqlite::Error) -> AppError {
    let context = context.to_string();
    move |e| {
        tracing::error!(%context, error = %e, "sqlite error");
        AppError::Database(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(template: &str, seq: u32, seq_w: usize) -> String {
        rename_target(
            Path::new("/shoot"),
            template,
            "Wedding",
            "jpg",
            "IMG_0001.jpg",
            Some("2026-08-16T14:05:09Z"),
            None,
            "2026-08-17T00:00:00Z",
            Some("Sony A7 IV"),
            Some("50mm F1.4"),
            Some(52.0),
            Some(400),
            seq,
            seq_w,
        )
    }

    #[test]
    fn tokens_expand() {
        assert_eq!(
            target("{date}_{name}_{sequence}.jpg", 1, 3),
            "/shoot/2026-08-16_Wedding_001.jpg"
        );
        assert_eq!(
            target("{camera}-{focal}-{iso}.jpg", 1, 3),
            "/shoot/Sony-A7-IV-52-400.jpg"
        );
        assert_eq!(
            target("{time}.jpg", 1, 3),
            "/shoot/14-05-09.jpg"
        );
        assert_eq!(target("{original}.jpg", 1, 3), "/shoot/IMG_0001.jpg");
    }

    #[test]
    fn sequence_zero_pads_to_the_given_width() {
        let seq = |n: u32, w: usize| rename_target(
            Path::new("/s"), "{sequence}", "n", "jpg", "a.jpg",
            None, None, "2026-08-17T00:00:00Z",
            None, None, None, None, n, w,
        );
        assert_eq!(seq(12, 3), "/s/012.jpg");
        assert_eq!(seq(999, 4), "/s/0999.jpg");
        assert_eq!(seq(7, 3), "/s/007.jpg");
    }

    #[test]
    fn missing_tokens_expand_to_empty_and_are_sanitized() {
        // No camera/lens/focal/iso on this photo.
        let t = rename_target(
            Path::new("/s"),
            "{date}_{camera}_{lens}_{focal}_{iso}",
            "n",
            "jpg",
            "a.jpg",
            Some("2026-08-16T00:00:00Z"),
            None,
            "2026-08-17T00:00:00Z",
            None, None, None, None,
            1, 3,
        );
        assert_eq!(t, "/s/2026-08-16.jpg");
    }

    #[test]
    fn original_name_containing_a_token_does_not_double_expand() {
        // Original filename literally contains "{date}". It must be inserted
        // verbatim (sanitized), not re-expanded.
        let t = rename_target(
            Path::new("/s"),
            "{date}_{original}",
            "n",
            "jpg",
            "x_{date}_y.jpg",
            Some("2026-08-16T00:00:00Z"),
            None,
            "2026-08-17T00:00:00Z",
            None, None, None, None,
            1, 3,
        );
        // Literal braces collapse to dashes (underscores are kept) — proof
        // the original was inserted verbatim, then sanitized once.
        assert_eq!(t, "/s/2026-08-16_x_-date-_y.jpg");
    }

    #[test]
    fn sanitize_strips_reserved_and_collapses() {
        assert_eq!(sanitize_name("a/b\\c:d*e?\"f<g>h|"), "a-b-c-d-e-f-g-h");
        assert_eq!(sanitize_name("   spaced   "), "spaced");
        assert_eq!(sanitize_name("..."), "renamed");
        assert_eq!(sanitize_name("a  b"), "a-b"); // spaces collapse to one dash
        assert_eq!(sanitize_name("{raw}"), "raw");
        assert_eq!(sanitize_name("Sony A7 IV"), "Sony-A7-IV");
    }

    #[test]
    fn sanitize_caps_length() {
        let long = "a".repeat(300);
        assert_eq!(sanitize_name(&long).chars().count(), 150);
    }

    #[test]
    fn suffixed_path_finds_free_name() {
        let dir = std::env::temp_dir().join(format!("pg_fs_suffix_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("IMG.jpg");
        std::fs::write(&base, b"x").unwrap();
        std::fs::write(&dir.join("IMG-1.jpg"), b"x").unwrap();
        let got = suffixed_path(&base);
        assert_eq!(got.file_name().unwrap(), "IMG-2.jpg");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn op_and_policy_parsing() {
        assert!(matches!(OpKind::parse("move"), Ok(OpKind::Move)));
        assert!(matches!(OpKind::parse("copy"), Ok(OpKind::Copy)));
        assert!(OpKind::parse("delete").is_err());
        assert!(matches!(
            CollisionPolicy::parse("skip"),
            Ok(CollisionPolicy::Skip)
        ));
        assert!(matches!(
            CollisionPolicy::parse("avoid-by-renaming"),
            Ok(CollisionPolicy::AvoidByRenaming)
        ));
        assert!(CollisionPolicy::parse("overwrite").is_err());
    }
}
