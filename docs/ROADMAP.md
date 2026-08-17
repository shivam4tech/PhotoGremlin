# Roadmap

## v0.1 — the one-day build (10 sprints)

| # | sprint | status |
|---|---|---|
| 1 | Foundation — Tauri+React+Rust+SQLite shell, nav, theme, IPC, DB init, logging, errors | **done** (2026-08-17) |
| 2 | Photo scanner + database ingestion (recursive walk, upsert, sessions, progress) | **done** (2026-08-17) |
| 3 | Thumbnails + library grid (cache, lazy, virtualized) + viewer + metadata panel | |
| 4 | Local image analysis (sharpness, brightness, contrast, saturation, mono, clipping) + progress | |
| 5 | EXIF + filter engine (all filter areas, AND composition) | |
| 6 | Dashboard + statistics engine (periods, session stats, comparison, trends, camera/lens, distributions, selection infra) | |
| 7 | Selection + file operations (group rename, move/copy/trash, collisions, confirmations) | |
| 8 | Saved views + collections + similarity (perceptual hash, groups, bursts) + session detail | |
| 9 | Local intelligence (face detection first; smile only if stable — otherwise documented as v0.2) | |
| 10 | Polish + release (shortcuts, empty/loading/error states, settings, onboarding, privacy messaging, release builds, smoke test) | |

Priority order under time pressure (never sacrifice 1–10 for 11–14):
launch → scan → grid → analysis → EXIF → filters → dashboard → selection →
file ops → saved views → similarity → local AI → contact sheets → polish.

## v0.2

Stronger RAW support (decode provider), better similarity, contact sheets,
ratings/flags/color labels, GPS removal (careful, destructive metadata
edit), metadata editing, smile detection (if deferred from v0.1).

## v0.3

Face grouping (identity via local embeddings), eyes-open detection, people
count, subject classification, semantic local search.

## v0.4

Advanced culling, side-by-side comparison with synchronized zoom, better
burst analysis, customizable shortcuts, project management.

## v0.5

Plugin architecture, extensible analysis providers, richer reporting,
exportable statistics, photographer "performance" reports (numbers only —
see STATISTICS.md language discipline).

## Explicitly never

Cloud accounts, auth, backend, online DB, social/sharing, full photo
editing/color grading, video, mobile, subscriptions, ads, telemetry,
external AI APIs. (See PRODUCT_SPEC.md.)
