# Testing

Strategy: fast, offline, deterministic. No network, no real photo sets, no
flaky timing.

## Rust unit tests (`cargo test` in src-tauri)

Per-sprint coverage (✓ = in place today):

- Database: schema migration creates expected tables; migration idempotent ✓
- Photo upserts: idempotent by path; dimensions never blanked on rescan;
  mtime refreshed; session upsert keyed on root_path ✓
- Scanner (Sprint 2): extension classification (all §12 formats) ✓;
  orientation derivation ✓; full pipeline on synthetic shoots — decodables
  get dimensions+orientation, RAW stays null, hidden dirs skipped,
  non-photos ignored, re-scan idempotent, cancel semantics, 1,000-file scale
  test with a time ceiling ✓
- Thumbnailer (Sprint 3) ✓: FNV-1a is stable + distinct; base64 round-trips
  (incl. empty/1/2/3-byte and binary edges); cache key reacts to
  path/size/mtime/target-width and is deterministic; generate produces a
  valid decodable JPEG at the expected aspect (landscape 640×480→256×192,
  portrait 480×640→256×341); full `get()` lifecycle — miss→generate→hit with
  identical bytes, unknown id → friendly "no longer in the library" error;
  missing file → friendly error.
- Analysis metrics (Sprint 4): each metric on **synthetic images** built
  in-memory with the `image` crate (never shipping real photos in the repo)
  ✓:
  - Rec.709 luma sanity (pure R/G/B/white/black);
  - solid grays → expected brightness (±0.5) and the 35/65 dark/bright
    flags;
  - sat extremes: magenta → 100, gray → 0; monochrome gating: pure gray and
    near-gray flagged, color not, and the faded case that passes the sat
    gate but fails the channel-similarity gate;
  - clipping: half-white image ≈ 50% highlight, half-black ≈ 50% shadow,
    mid-gray → 0/0;
  - sharpness: 8×8 checkerboard ≫ linear ramp (Laplacian of a ramp ≈ 0),
    both in 0–100; sigmoid scale sane + monotonic;
  - contrast: striped high-contrast ≫ 4-level band, both in 0–100;
  - degenerate 1×1/2×2 images score 0 sharpness without panicking.
- Analysis pipeline (Sprint 4, integration `tests/analysis_integration.rs`)
  ✓: scan a synthetic shoot → every decodable gets a version-1 row with
  `source_mtime`, RAW gets none, values order sensibly (bright > dark,
  sharp ≫ smooth); a second run is a byte-identical no-op; touching one
  file's mtime + re-scan re-analyzes exactly that one; a deleted file is a
  friendly per-file failure that names it; cancel-before-start measures
  nothing and reports `cancelled`.
- Filters (Sprint 5): each operator against in-memory rows; unknown field →
  validation error; SQL values always bound (no injection path).
- Statistics (Sprint 6): period resolution (today/week/month/year/custom/all
  on a fixed "now"); average over analyzed-only subset; distribution binning
  edges (ISO 400 → first of the two boundary bins, rule documented).
- Rename templates (Sprint 7): token expansion, sanitization, collision
  detection within a plan, sequence width growth.
- File ops (Sprint 7): collision detection, cross-device move staging,
  trash target resolution, partial-failure reporting — all against temp dirs.
- Similarity (Sprint 8): identical image → hash distance 0; perturbed →
  small distance; unrelated → large; clustering groups as expected.

## Frontend tests (`npm test`, Vitest)

- Store behavior (view switching, progress payloads) ✓
- Virtual grid math (Sprint 3): `computeVisibleRange` window/overscan/
  clamping (never past item count, start≤end, zero-item safe, abnormal col
  count) and `computeColumns` fit + min-one-column. The pure math is tested
  so the scroll handler stays a thin wrapper. ✓
- Filter draft composition (Sprint 5): adding/removing conditions keeps
  empty-filter semantics.
- Pure presentation helpers (formatting: ISO→"1/125", focal→"50mm",
  percentages) as they land.

## Integration (Rust, `tests/`)

End-to-end over a temp library:

```
write synthetic folder (N images + EXIF via a tiny writer or sidecar JPEG)
  → scan_folder → assert counts/duplicates
  → analyze → assert rows + version
  → apply filter → assert result set
  → statistics → assert aggregates
```

Run with `cargo test` (same command, `tests/` dir) so one command validates
the pipeline.

## What we do NOT test here

- No cloud, no accounts → nothing to mock.
- No timing-based assertions; background work is verified by final state +
  progress counters, not wall clock.
- GUI pixel tests: out of scope for v0.1 (manual smoke checklist in
  ROADMAP.md §v0.1).

## Manual smoke checklist (release day, per platform)

small folder · large folder (multi-thousand) · missing file mid-session ·
unsupported format · duplicate import · rename collision · move · copy ·
trash · filter · saved view · dashboard · session comparison · offline mode
(network disabled).
