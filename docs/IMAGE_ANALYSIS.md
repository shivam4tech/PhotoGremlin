# Image Analysis

All core analysis is **deterministic, local, AI-free**. Each measurement is a
technical estimate, presented as such — never as "image quality".

Pipeline (Sprint 4, implemented): `queue → decode (bounded workers) →
measure → store row (algorithm_version, source_mtime) → progress event`.
`run_analysis` (`src-tauri/src/analysis/mod.rs`) is Tauri-free and
integration-tested; `commands/analysis.rs` adapts it to a background job with
a claim-and-cancel slot (like scans) and streams `analysis-progress` /
`analysis-complete` events.

**Pinned implementation constants:**

- `WORKING_MAX_SIDE = 2048` — images are decoded and downscaled (long side)
  before measuring, so a 100 MP file cannot explode memory. Both the luma
  histogram pass and the Laplacian sharpness pass run at this working
  resolution.
- `ANALYSIS_WORKERS = 3` — at most 3 images are decoded/measured concurrently;
  the queue is de-interleaved round-robin into one slice per worker
  (no channels; item *i* → worker *i* mod N).
- `MAX_PIXELS ≈ 500 MP` — bigger files are reported as a friendly failure,
  never decoded.
- Progress is emitted after every completed or failed item (the scan pattern).
- **Incremental by design:** a photo is (re)measured only if it has no row,
  its row's `algorithm_version` is older, or `analysis.source_mtime`
  differs from `photos.file_mtime` (a re-scan refreshes that after the file
  changed on disk). A no-op re-run writes nothing.
- Thumbnails (Sprint 3) are generated lazily on demand by their own engine;
  analysis does its own bounded decode pass. Both are small and cached.

## Scores: normalized 0–100

Every continuous score is scaled to 0–100 so the filter engine and UI stay
stable. The scale is documented per metric below; a future algorithm bump
bumps `ANALYSIS_ALGORITHM_VERSION` and offers re-analysis (DATABASE.md).

## Sharpness (Sprint 4)

Method: **variance of the Laplacian** of the grayscale image, computed at a
bounded working resolution (downscaled so a 100 MP file doesn't explode
memory).

- `score = 100 / (1 + exp(−(log10(var) − μ) / k))` — the sigmoid maps the
  right-tailed Laplacian-variance distribution onto 0–100. Pinned v0.1
  calibration (in `analysis/metrics.rs` with a comment): at the 2048 px
  working resolution, sharp photo material measures log10(var) ≈ 3.5–4.5 and
  visibly soft material below ≈ 2.5, so **`μ = 3.5`, `k = 0.6`**. Recalculate
  when the working resolution or kernel changes and bump
  `ANALYSIS_ALGORITHM_VERSION`.
- Kernel: 4-neighbor Laplacian (`[0 1 0; 1 −4 1; 0 1 0]`) over the image
  interior; the variance of those values is the edge-energy statistic.
  Images with fewer than 3 px on a side score 0 (no interior).
- Output example: `sharpness = 87`.
- UI language: "sharpness 87", "potentially blurry (sharpness < 40 when
  filtered)". Never "unsharp/bad".

## Brightness (Sprint 4)

- Grayscale = Rec. 709 luma: `0.2126·R + 0.7152·G + 0.0722·B`.
- `brightness` = mean luma rescaled 0–100.
- Categories (drives `is_dark` / `is_bright` flags and filters):
  - Very Dark < 15 · Dark < 35 · Normal 35–65 · Bright 65–85 · Very Bright ≥ 85

## Contrast (Sprint 4)

- Luma standard deviation (σ) plus percentile spread (p95 − p5) as a robust
  companion against outliers. Percentiles come from the same 256-bin luma
  histogram used for brightness/clipping (one pass, no sort).
- `contrast` = blend: `0.6 · scale(σ) + 0.4 · scale(p95 − p5)` → 0–100,
  saturating at σ ≈ 70 (visually maxed contrast for photos) and spread ≈
  180 gray levels (`CONTRAST_SIGMA_SATURATION` /
  `CONTRAST_SPREAD_SATURATION` in `analysis/metrics.rs`).

## Saturation (Sprint 4)

- Per pixel: `sat = (max(R,G,B) − min(R,G,B)) / max(R,G,B)` when max > 0.
- `saturation` = mean sat × 255 → clamped 0–100 (mean sat of typical color
  photos ≈ 0.2–0.5).
- Feeds the monochrome check as well as the "color/monochrome" filter.

## Monochrome detection (Sprint 4)

No ML. Likely monochrome when:
- mean per-pixel sat **< 0.06** (≈ 15/255), **and**
- channel similarity: `|mean(R) − mean(G)|, |mean(G) − mean(B)|, |mean(R) − mean(B)|`
  all < 8 gray levels.
Both conditions avoid false positives on foggy/low-saturation color scenes
(these still fail the similarity gate only when truly achromatic; thresholds
are deliberately conservative and flagged in code).
Output: `is_monochrome ∈ {0, 1}`.

## Highlight clipping (Sprint 4)

- Percentage of pixels with luma ≥ 250 (≈98%).
- UI: "highlight clipping 2.4%". Filters: "clipping above X%".

## Shadow clipping (Sprint 4)

- Percentage of pixels with luma ≤ 5 (≈2%).
- UI: "shadow clipping 5.7%".

## Dark/bright flags (Sprint 4)

`is_dark` when brightness < 35; `is_bright` when brightness > 65 (same
boundaries as the categories above). Filters can use flags or the numeric
score.

## Perceptual hash (Sprint 8)

aHash/dHash family: downscale to 8×8 grayscale, diff of adjacent pixels →
64-bit hash stored as 16 hex chars. Grouping: exact/hamming-distance-≤ 8
clusters → `similarity_groups`. Burst grouping adds the timestamp dimension
(photos within ~3 s with similar hashes). Suggestions within a group
(“sharpest in group”) are computed but always presented as suggestions —
nothing is auto-deleted.

## Date estimation (Sprint 12)

Photos without an EXIF date get a **labelled estimate** of their capture
time, so browsing, filters and statistics can treat a whole library as dated
even when the camera wrote no date. One resolution per photo, dominance
order **exif > filename > mtime**; estimates never override a real EXIF
date, and every estimate carries its provenance
(`photos.capture_datetime_source`: `'exif' | 'filename' | 'mtime'`).

1. **EXIF** — `DateTimeOriginal` (primary IFD, offset 0x9003). If present,
   the file is done; nothing below runs.
2. **Filename** — `src-tauri/src/metadata/estimate.rs`. Exact camera-roll
   patterns first (`IMG_yyyymmdd_hhmmss`, `PXL_…`, `VID_…`, `Screenshot_…`,
   `Screenshot_yyyy-mm-dd at hh.mm.ss`, `screen_…` variants, macOS
   `yyyy-mm-dd at hh.mm.ss` + Camera Upload `yyyy-mm-dd`), then a loose
   scan for a well-formed date: if the day parses as
   `yyyy-mm-dd`/`yyyymmdd` (with or without time and timezone suffix), it
   wins with the *day*; only a full `hh:mm(:ss)` time following the date
   yields hour precision. A bare time alone or an unparseable name yields
   nothing.
3. **mtime** — the file's modification time (UTC), always present as a
   last resort; the user can see the provenance in the viewer
   ("Filename (estimated)" / "File modified (estimated)").

Integration coverage (`tests/date_estimation_integration.rs`) runs the real
pipeline — scanner → metadata pass → sessions — against real files,
including the Unsplash-style unparseable-name case.

## RAW formats

Initial MVP decodes: **JPG, JPEG, PNG, WebP, TIFF/TIF** via the `image` crate.
RAW (CR2/CR3/NEF/ARW/RAF/DNG/ORF/RW2) is *architecturally ready*: the scanner
indexes RAW files (metadata from EXIF sidecar), but pixel decode is isolated
behind a provider concept so a later `libraw`-based provider can be added
without touching callers. When undecodable: the photo still gets thumbnails
from a sidecar JPG when one exists, otherwise it shows an
"unsupported format" placeholder — never a crash.

## What we do NOT compute

No "image quality", no noise estimation in v0.1, no aesthetic verdicts.
Sharpness here measures edge energy, full stop.
