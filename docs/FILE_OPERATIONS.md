# File Operations

Rename, move, copy, trash. The safety rules are the product: **no silent
overwrites, no destructive action without confirmation.**

## Universal protocol (Sprint 7)

Every operation follows the same pipeline in Rust (`filesystem/`):

1. **Verify source** — every selected path exists and is readable. Missing
   files are reported individually ("file no longer exists"), not as a
   blanket failure.
2. **Resolve destination** — for each source, compute the destination path.
3. **Collision detection** — if a destination path already exists, the
   operation is blocked for that item. Never overwrite. The UI shows a
   preview: `IMG_0001.jpg → target/IMG_0001.jpg (ALREADY EXISTS)`.
   Available resolutions: skip item, or a safe suffix (`IMG_0001-1.jpg`) only
   if the user explicitly chose "avoid by renaming".
4. **Preview + confirm** — before anything touches disk, the UI shows the
   full plan (N items, per-item destination). Destructive operations
   (trash, and overwrite-adjacent choices) require an explicit confirmation;
   non-destructive operations (copy) may proceed after preview.
5. **Execute with transactional bookkeeping** — per-item success/failure; a
   partial failure keeps successful items and reports the rest with
   reasons. Nothing is half-renamed: a rename is a single atomic `rename` per
   file (or copy+verify+del for cross-device moves).
6. **Audit log** — every executed operation appends to `file_operations`
   (op_type, source, destination, status, detail, timestamp).

## Rename (group)

Template engine with tokens:

```
{name}        selected group/session name (user-supplied in the dialog)
{original}    original filename without extension
{date}        capture date YYYY-MM-DD (fallback: file date)
{time}        capture time HH-MM-SS
{camera}      camera model, filesystem-safe
{lens}        lens, filesystem-safe
{focal}       rounded mm
{iso}         ISO
{sequence}    zero-padded ascending counter (width auto-grows)
```

Examples: `{date}_{name}_{sequence}.jpg` → `2026-08-16_Wedding_001.jpg`.

Rules:
- Filenames are sanitized (path separators, invalid chars per OS).
- The full mapping is computed first; **collisions inside the rename plan
  itself** (e.g. two sources mapping to the same name) abort the plan with an
  itemized report.
- Rename is done as move within the directory (atomic per file).
- The DB (`photos.path/filename`) is updated in the same transaction window;
  re-scan stays idempotent.

## Move / Copy

- Move: `std::fs::rename` when same filesystem; otherwise staged
  copy→verify (size + optional hash)→delete-source, with the source deleted
  only after verification passes.
- Copy: `std::fs::copy`-style staged copy; original is never modified.
- Destination is created if missing (explicitly shown in the preview).

## Trash — never permanent delete

V0.1 sends files to the **OS trash** (platform trash API / rfd-style helper;
Linux: `trash` target directory under `~/.local/share/Trash`). A hard delete
does not exist in v0.1 at all — the audit log is the only "removal" concept
besides trash. DB rows are marked/removed only after the filesystem action
succeeds (or on a later rescan discovering the file is gone — the scanner
reconciles and flags missing files to the user rather than silently dropping
rows).

## Progress & errors

`operation-progress` events drive the progress UI; per-item errors surface in
a results dialog (item, attempted action, reason). The log carries details.
