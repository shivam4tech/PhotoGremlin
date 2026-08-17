# Image Analysis

All core analysis is **deterministic, local, AI-free**. Each measurement is a
technical estimate, presented as such — never as "image quality".

Pipeline (Sprint 4): `decode (bounded workers) → measure → store row
(algorithm_version) → progress event`. Decoding is the memory hotspot: at most
a small fixed number of full-res images in RAM at once, thumbnails generated
from the same decode pass (Sprint 3).

## Scores: normalized 0–100

Every continuous score is scaled to 0–100 so the filter engine and UI stay
stable. The scale is documented per metric below; a future algorithm bump
bumps `ANALYSIS_ALGORITHM_VERSION` and offers re-analysis (DATABASE.md).

## Sharpness (Sprint 4)

Method: **variance of the Laplacian** of the grayscale image, computed at a
bounded working resolution (downscaled so a 100 MP file doesn't explode
memory).

- `score = 100 · sigmoid(log10(var) − μ) / k` — the sigmoid maps the
  right-tailed Laplacian-variance distribution onto 0–100; `μ`, `k` chosen
  from measured distributions of typical photo material (values pinned in
  code with the calibration data noted in a comment).
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
  companion against outliers.
- `contrast` = blend: `0.6 · scale(σ) + 0.4 · scale(p95 − p5)` → 0–100,
  saturating at σ ≈ 70 (visually maxed contrast for photos).

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
