# Testing

Strategy: fast, offline, deterministic. No network, no real photo sets, no
flaky timing.

## Rust unit tests (`cargo test` in src-tauri)

Per-sprint coverage (✓ = in place today):

- Database: schema migration creates expected tables; migration idempotent ✓
- Analysis (Sprint 4): each metric on **synthetic images** —
  - generated in temp dirs with the `image` crate (gradients, noise,
    patterns), never shipping real photos in the repo.
  - bright gradient → high brightness; dark → low; saturated magenta grid →
    high sat; gray → monochrome + low sat; white patch → highlight clipping
    > 0; black patch → shadow clipping.
- Sharpness (Sprint 4): sharp synthetic pattern > blurred pattern, both in
  0–100 range and monotonic in expected direction.
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
