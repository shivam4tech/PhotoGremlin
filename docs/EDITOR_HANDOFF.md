# Local editing-application handoff

Status: **Sprint 28 shipped.** PhotoGremlin can hand a photographer's kept
source files to a desktop editing application chosen in Settings. This is a
local process launch, not an import plugin or catalog integration.

## User flow

1. In Settings, choose an editing application's executable (or a macOS app
   bundle). The native picker is asynchronous and can be cancelled safely.
2. Complete a shoot review or select kept photographs in the cull workspace.
3. Use **Open kept in …** from the finish screen or Export menu.
4. Rust validates the saved application again, resolves the requested photo
   IDs in the active catalog, skips source paths that no longer exist, and
   starts the application with the available paths.

The completion message states how many files were handed off and whether any
missing sources were skipped. Validation failures stay beside the action that
failed and use friendly text; detailed process information remains in the
local log.

## Safety boundary

- The user selects the executable; PhotoGremlin does not discover or download
  applications.
- The executable is canonicalized and must be an executable file on Unix. A
  macOS `.app` bundle is launched through the operating-system `open -a`
  mechanism.
- Process arguments are passed directly through `std::process::Command`. No
  shell is involved, so filenames cannot be interpreted as shell syntax.
- Photo IDs are deduplicated and resolved through the active project catalog.
  Arbitrary caller-supplied filesystem paths are never accepted by the launch
  command.
- Missing source files are skipped and reported. If every source is missing,
  no application is started.
- A single direct launch accepts at most 500 photographs. Larger selections
  use the existing preview-and-confirm **Export originals** copy operation.
- Source photographs, selections, ratings, labels, sidecars, and third-party
  catalogs are never modified by the handoff.
- The command makes no network request and has no telemetry.

## Why catalog mutation is deferred

Lightroom Classic and Capture One catalogs are private, stateful databases.
Writing them without a vendor-supported interchange contract can corrupt a
working catalog or silently detach edits. Sprint 28 therefore uses the stable
operating-system contract—open these ordinary files in this application.
Catalog-specific plugins, watched-folder export, or sidecar interchange can be
considered after beta feedback, with round-trip fixtures and an explicit
backup/rollback design.

## IPC surface

The typed frontend methods in `src/lib/ipc.ts` map to commands in
`src-tauri/src/commands/app.rs`:

- `pick_editor_application` — native application/file picker;
- `get_editor_config` / `set_editor_config` / `clear_editor_config` — global
  machine preference;
- `launch_in_editor(photoIds)` — validated, bounded kept-set handoff returning
  `{ application, requested, launched, skippedMissing }`.

No schema migration is needed. The configuration is a JSON value in the
existing global settings table, separate from every project catalog.
