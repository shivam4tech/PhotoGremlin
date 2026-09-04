# File Operations

Rename, move, copy, trash, and explicitly requested permanent deletion. The
safety rules are the product: **no silent overwrites, no destructive action
without a preview and confirmation.** Trash remains the recommended removal
path because it is recoverable.

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
   (trash, permanent deletion, and overwrite-adjacent choices) require an
   explicit confirmation;
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

## Trash and permanent deletion

**Trash is the default.** On Linux it uses the freedesktop trash location
under `~/.local/share/Trash` (or `$XDG_DATA_HOME/Trash`) and retains recovery
metadata. The corresponding catalog row is removed only after the filesystem
action succeeds.

Sprint 32 adds permanent deletion as a separate, visibly dangerous action for
photographers who explicitly choose it. It accepts indexed photo IDs only,
shows the exact local path in a preview, states that the file cannot be
restored, and requires a second native confirmation immediately before start.
Execution re-resolves the IDs and paths, rejects missing paths and
directories, removes only the planned file, then removes its catalog row and
writes the audit entry. It never runs as a consequence of Reject, a filter, a
collection change, or a trash action.

## Progress & errors

`operation-progress` events drive the progress UI; per-item errors surface in
a results dialog (item, attempted action, reason). The log carries details.

## Implementation (Sprint 7)

Everything above lives in `src-tauri/src/filesystem/` (Tauri-independent: it
takes a `&Db` + paths and returns result structs, so the exact pipeline is
integration-tested on real temp directories in `tests/fileops_integration.rs`).
Commands in `commands/fileops.rs` split each operation into a cheap,
synchronous **plan** (preview) and a background **start** (execute):

### IPC surface

| Command | Kind | Notes |
| --- | --- | --- |
| `plan_group_rename(photo_ids, template, group_name)` | sync | returns `FileOpPlan` |
| `start_group_rename(photo_ids, template, group_name)` | bg | re-plans, then executes; aborts if in-plan collision |
| `plan_move_copy(photo_ids, dest_dir, op, on_collision)` | sync | `op` `"move"\|"copy"`, `on_collision` `"skip"\|"avoid-by-renaming"` |
| `start_move_copy(photo_ids, dest_dir, op, on_collision)` | bg | |
| `plan_trash(photo_ids)` | sync | `destructive: true` |
| `start_trash(photo_ids)` | bg | |
| `plan_permanent_delete(photo_ids)` | sync | `destructive: true`; exact source paths and irreversible note |
| `start_permanent_delete(photo_ids)` | bg | re-plans, confirms separately in the UI, then deletes files |
| `stop_operation()` | sync | cooperative cancel between items |
| `set_selection` / `set_selections` / `clear_selection` / `clear_selections` | sync | culling state |
| `list_selections()` | sync | current culling map |
| `recent_file_ops(limit)` | sync | audit log, newest first (capped 500) |

Background execution runs in a single `operation` slot (same claim-and-cancel
model as scan/analysis/metadata) and streams `operation-progress`
(reusing `ProgressPayload`) then `operation-complete`
(`{ summary: OperationSummary | null, error: string | null }`). The UI keeps
the slots mutually exclusive (buttons disable each other).

### Rename engine

`expand_template` is a **single-pass** scan: at each `{` it tries the longest
token first and replaces with `value(token)`; inserted text is never
re-scanned, so an original filename that literally contains `{date}` cannot
double-expand. Tokens: `{name} {original} {date} {time} {camera} {lens}
{focal} {iso} {sequence}`. `{sequence}` is zero-padded to `max(3,
digits(count))`. Missing values expand to empty. The result is run through
`sanitize_name` (path separators, control chars, spaces, literal braces →
collapsed `-`; dangling separators trimmed; empty → `renamed`; capped at 150
chars). **The extension is always the file's own**: any trailing `.ext` in the
template is stripped and the original's re-attached, so the result is always a
scannable image filename.

Planned deterministically in `COALESCE(capture_datetime, indexed_at)` order so
a sequence reads like the shelf. **In-plan collisions** (two sources onto one
destination) abort the whole plan with an itemized `note` per item; **on-disk
collisions** block just that item with `note: "ALREADY EXISTS"`. Rename is a
single atomic `std::fs::rename` (in-directory).

### Move / copy

`std::fs::rename` first (same filesystem, atomic); on
`ErrorKind::CrossesDevices` falls back to staged **copy → size-verify →
delete-source**, deleting the source only after the copy verifies (a mismatch
removes the partial destination and reports a failure). Copy never touches the
original; it verifies the copied byte count, indexes the copy as a new photo
(inheriting the source's dimensions, session = the active library's when the
copy lands inside it, else NULL), and the original row is untouched. A missing
destination directory is reported in the preview (`will_create_dir`) and
created at execution. Collisions follow the user's policy: `skip` (block the
item) or `avoid-by-renaming` (`IMG_0001-1.jpg`, first free suffix).

### Trash

Linux uses the freedesktop **XDG trash**: `~/.local/share/Trash/{files,info}`
(or `$XDG_DATA_HOME/Trash`), with a `.trashinfo` sidecar recording the original
absolute path + deletion date. Name collisions in the trash get `-n` suffixes.
The source is moved (cross-device staged copy→verify→delete if needed). Only
after the filesystem action succeeds is the DB row deleted (FK cascade removes
its analysis + selection rows) and an audit row written. On non-Linux
platforms v0.1 returns a friendly per-item failure ("OS trash is only
available on Linux in v0.1 — use Move instead").

### Permanent deletion (Sprint 32)

`plan_permanent_delete` resolves every requested photo through the catalog and
returns the source path with no destination. `start_permanent_delete` builds a
fresh plan so a stale preview cannot substitute another path. Execution uses
filesystem metadata to require a file (never a directory), calls
`remove_file`, and only then deletes the photo row (FK cascade removes its
analysis, membership, and selection rows) and appends a
`delete-permanently` audit record. A missing or changed source is an itemized
failure; other items may still complete.

### Bookkeeping & safety

- Every successfully executed item appends to `file_operations`
  (`op_type, source_path, dest_path, status ∈ done|failed, detail, created_at`).
- Rename/move update `photos.path/filename/size_bytes/file_mtime` per item in
  the same window; pixels are unchanged so the analysis row stays valid.
- Execution **re-checks each destination right before acting** (the
  preview→confirm gap is not atomic), so a file appearing in that window is a
  per-item failure, never an overwrite.
- Per-item success/failure; partial failure keeps the successful items and
  reports the rest with reasons. Nothing is half-renamed.
- `OperationSummary.items` is capped at 500 for IPC; the full detail is in the
  log and the `file_operations` table.

### Frontend

`features/fileops/FileOpsPanel.tsx` (driven by the photographs marked
"selected" in culling mode): pick rename pattern / move or copy destination +
collision policy / trash / permanent delete, **Preview** → inspect the
per-item plan (blocked items flagged, `will_create_dir` noted, aborted plans
shown red) → confirm (trash and permanent deletion use distinct native warning
dialogs) → execute, with a live progress line, a Stop button, and a results
summary of anything not `done`. The Library, similarity/burst groups,
collections, and viewer also expose per-photo Trash and Delete permanently
actions; each opens the same preview-first dialog rather than touching the
filesystem directly. Culling (keep/reject per tile) is in
`components/PhotoTile.tsx` + `stores/appStore.ts`
(`selections`, `selectionMode`, persisted to the `selections` table).
