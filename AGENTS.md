# AGENTS.md — PhotoGremlin

**Read this file completely before writing any code in this repository.**

PhotoGremlin is a privacy-first, local-only desktop application that helps
photographers explore, analyze, filter, organize and understand their own photo
collections. It is built with Tauri 2 (Rust core) + React/TypeScript (UI) +
SQLite.

## Hard rules (non-negotiable)

1. **Local-first. No cloud, ever.** The app must make zero network requests at
   runtime. No accounts, no login, no backend, no telemetry, no analytics, no
   external AI APIs.
2. **No external AI APIs. No remote inference.** Optional visual intelligence
   (faces, smiles) must use small local models only, and the entire core
   product must work with it disabled.
3. **No Python runtime dependency.** Python may be used for scripts and
   benchmarks during development, but never shipped or required by the app.
4. **Minimal dependencies.** If you can solve it with ~50 lines, do not add a
   crate. Every dependency must earn its place.
5. **Core features must not depend on AI.** Scanning, thumbnails, EXIF,
   sharpness, brightness, contrast, saturation, clipping, monochrome
   detection, filtering, similarity, statistics, rename/move/copy/trash and
   collections all run on deterministic local algorithms.
6. **Rust owns heavy work.** The React frontend never manipulates files, never
   contains image-analysis algorithms, and never contains database
   implementation. It talks to Rust only through typed Tauri commands
   (`src/lib/ipc.ts` is the single funnel).
7. **Never block the UI.** Long-running work (scans, analysis, file
   operations) runs in background tasks and reports progress via events
   (`src-tauri/src/events.rs`).
8. **Never silently overwrite files.** Every file operation verifies sources
   and destinations, detects collisions, and requires explicit confirmation
   for destructive operations.
9. **Never permanently delete without explicit user confirmation.** Prefer OS
   trash/recycle bin.
10. **Never make aesthetic judgments in the UI.** Report measurable
    characteristics ("sharpness 62", "high highlight clipping"), never
    verdicts ("bad photo", "delete this", "you improved"). Data in, decisions
    stay with the photographer.
11. **Cross-platform.** Windows, macOS, Linux. One codebase via Tauri. No
    platform-specific branching unless unavoidable, and never at the
    architecture level.
12. **Run tests after every modification** (`cargo test` in `src-tauri`,
    `npm test` at the root). **Build before finishing a sprint**
    (`npm run build:app` — debug bundle; release build at the final sprint).
13. **Update documentation** in `docs/` when you change architecture, schema,
    algorithms, or the IPC surface. Documentation must describe the
    implementation, not ideals.
14. **Do not rewrite working architecture unnecessarily. Do not expand scope
    without strong justification.** The sprint plan is fixed; defer risky
    features instead of destabilizing the core.
15. **Git discipline:** meaningful conventional commits after each sprint
    (`feat: add photo scanner`). Never `git reset --hard` to "fix" problems;
    never discard working code.

## Environment notes (this machine)

- Rust is at `~/.cargo/bin` (add to PATH). The Linux build needs Tauri's
  `-dev` system libraries, which are installed in a **user-space sysroot** at
  `~/pgsysroot` (no root on this box).
- **Before any cargo/tauri build, source the env first:**
  `source /home/shivam/pg-env.sh`
  (exports `PATH`, `PKG_CONFIG_PATH`, `LD_LIBRARY_PATH`, include paths).
- Frontend toolchain: Node 22 + npm.

## Repository layout

```
src/                  React + TypeScript UI
  components/         Shared UI components
  views/              One file per top-level view (library, dashboard, ...)
  features/           Feature-specific logic (library, filters, viewer, ...)
  stores/             Zustand state
  types/              IPC type mirrors (keep in sync with Rust)
  lib/ipc.ts          Typed IPC client — the ONLY place that calls invoke()
src-tauri/src/
  commands/           Tauri IPC commands (grouped by domain)
  database.rs         SQLite access + versioned migrations
  scanner/            Recursive folder scanning & indexing
  analysis/           Image measurement algorithms + pipeline
  metadata/           EXIF extraction
  similarity/         Perceptual hashing & grouping
  filesystem/         Rename/move/copy/trash with safety rules
  statistics/         Statistics engine (period-scoped aggregation)
  ml/                 Optional local models (face/smile) — isolated
docs/                 Engineering documentation (source of truth for design)
```

## Sprint workflow

1. Read this file and the relevant `docs/` pages.
2. Inspect existing code before modifying it.
3. Implement the sprint's scope only.
4. `cargo test` (in `src-tauri`) and `npm test` (root).
5. `npm run typecheck` and `npm run build` (frontend).
6. `npm run build:app` (full Tauri bundle — verify it produces an app).
7. Fix failures; do not move on with a red build.
8. Update `docs/` where behavior changed.
9. `git add` / `git commit` with a meaningful message.

## Definition of done (any change)

- Compiles (Rust + TypeScript), tests pass, bundle builds.
- No new network access. No new runtime dependency without justification.
- Errors surface as friendly messages (raw stack traces belong in the log
  only).
- Docs updated where architecture/schema/IPC changed.
