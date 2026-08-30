# Roadmap

## v0.1 — the one-day build (10 sprints)

| # | sprint | status |
|---|---|---|
| 1 | Foundation — Tauri+React+Rust+SQLite shell, nav, theme, IPC, DB init, logging, errors | **done** (2026-08-17) |
| 2 | Photo scanner + database ingestion (recursive walk, upsert, sessions, progress) | **done** (2026-08-17) |
| 3 | Thumbnails + library grid (cache, lazy, virtualized) + viewer + metadata panel | **done** (2026-08-17) |
| 4 | Local image analysis (sharpness, brightness, contrast, saturation, mono, clipping) + progress | **done** (2026-08-17) |
| 5 | EXIF + filter engine (all filter areas, AND composition) | **done** (2026-08-17) |
| 6 | Dashboard + statistics engine (periods, session stats, comparison, trends, camera/lens, distributions, selection infra) | **done** (2026-08-17) |
| 7 | Selection + file operations (group rename, move/copy/trash, collisions, confirmations) | **done** (2026-08-17) |
| 8 | Saved views + collections + similarity (perceptual hash, groups, bursts) + session detail | **done** (2026-08-17) |
| 9 | Local intelligence (face detection first; smile only if stable — otherwise documented as v0.2) | **done** (2026-08-17) — YuNet 2023mar local face detection (face_count); smile detection documented as v0.2 in LOCAL_AI.md |
| 10 | Polish + release (shortcuts, empty/loading/error states, settings, onboarding, privacy messaging, release builds, smoke test) | **done** (2026-08-17) — global shortcuts + Settings card, success-notice toast, empty-folder states, release bundle + smoke (see DEVELOPMENT.md §Release) |

Priority order under time pressure (never sacrifice 1–10 for 11–14):
launch → scan → grid → analysis → EXIF → filters → dashboard → selection →
file ops → saved views → similarity → local AI → contact sheets → polish.

## v0.2

**Stronger RAW support (Sprint 15 — done 2026-08-18: rawler-based decode
provider, previews for CR2/CR3/NEF/ARW/RAF/DNG/ORF/RW2 via tiles, viewer
and contact sheets, graceful placeholder fallback, see RAW_PREVIEWS.md)**,
**cross-session similarity
(Sprint 16 — done 2026-08-18: stricter 4-bit cross-session pairs with an
entropy guard against flat frames, windowed bucketing to keep the sweep
cheap, "N sessions" chips on group cards, see SIMILARITY.md)**,
**contact sheets
(Sprint 14 — done 2026-08-18: 12-tile A4-landscape PNG sheets from the
selection, see CONTACT_SHEETS.md)**,
**ratings/flags/color labels (Sprint 13 — done 2026-08-18: photos columns
`rating`/`flag`/`color_label`, Marking filter area, viewer + tile + bulk
selection controls, `update_marks` IPC)**, GPS removal (careful,
destructive metadata edit), metadata editing, **smile detection (deferred
from v0.1 — Sprint 9 shipped face detection; the smile model is the v0.2
follow-up, see LOCAL_AI.md)**.

## v0.3

Face grouping (identity via local embeddings), eyes-open detection, people
count, **subject/scene classification (Sprint 17–18, planned — see
SCENE_CLASSIFICATION.md: MobileNetV3-Large trained on a CC-BY Open Images
corpus, two-tier coarse ≥95% / fine top-1, shipped as a small local ONNX
model)**, **scenario culling, Aftershoot-style (Sprint 19–21, planned — see
CULLING.md: genre profiles over measurable features + blur-type/eye-state
models, ranked buckets, non-destructive)**, **filter quick controls (Sprint
22 — done 2026-08-28: measured visual bands, standard-stop exposure ranges,
and explicit unmeasured handling; see FILTER_ENGINE.md)**, semantic local
search.

## v0.4

Advanced culling, better burst analysis, customizable shortcuts, project
management.

## Beta-readiness sprints

| # | sprint | status |
|---|---|---|
| 27 | Catalog integrity, bounded caches, resumable review and large-shoot paging | **done** (2026-08-30) |
| 28 | Professional editor handoff — configured local editing application, safe kept-set launch and a visible review finish state | **done** (2026-08-30) |
| 29 | Photographer comparison + beta finish — on-demand side-by-side sequence comparison, premium interaction/accessibility cleanup and release verification | **done** (2026-08-30) |
| 30 | Premium theme and installed-build refresh — graphite/champagne system, semantic kept-state color, contrast/performance cleanup and fresh local release installation | **done** (2026-08-30) |
| 31 | Platform-native UI system — neutral graphite/silver surfaces, restrained cool-blue accent, unified measurement sliders, consistent controls and fresh local release installation | **done** (2026-08-30) |

Editor handoff deliberately does not write Lightroom or Capture One catalogs.
Those formats are private, stateful databases whose direct mutation would put
the photographer's catalog at risk. PhotoGremlin hands normal source files to
a user-selected desktop application without modifying those files; copying and
sidecar-preserving interchange can be expanded after beta feedback.

## v0.5

Plugin architecture, extensible analysis providers, richer reporting,
exportable statistics, photographer "performance" reports (numbers only —
see STATISTICS.md language discipline).

## Explicitly never

Cloud accounts, auth, backend, online DB, social/sharing, full photo
editing/color grading, video, mobile, subscriptions, ads, telemetry,
external AI APIs. (See PRODUCT_SPEC.md.)
