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
 - EXIF extraction (Sprint 5, `metadata/exif.rs`): a JPEG with a real APP1
   EXIF segment (built with `kamadak-exif`'s `Writer`) round-trips — camera
   make/model/lens, f-number, exposure seconds, ISO, focal length (1/100 mm→mm),
   capture datetime (zone-less EXIF → UTC RFC3339), dimensions, and a
   **presence-only** GPS bit (no coordinate field exists in the record). A file
   with no EXIF yields an empty record (not an error); blank ("0000:00:00" /
   empty) values are treated as absent; the datetime parser handles the
   `[,frac]` suffix and rejects garbage.
 - Metadata pass (Sprint 5, integration `tests/metadata_integration.rs`) ✓:
   scan a synthetic shoot → `exif_queue` holds every photo → `run_metadata`
   reads EXIF, stores camera fields + GPS presence, stamps `exif_at`, and a
   re-run is a no-op; a file without EXIF is stamped (processed) but keeps
   NULL camera fields; a deleted file is a friendly per-file failure;
   cancel-before-start processes nothing and reports `cancelled`.
  - Filters (Sprint 5, `filters/mod.rs` + integration
    `tests/filters_integration.rs`): every operator lowered and run against a
    seeded DB — boundary `>=`/`<`, AND composition, `in` placeholders,
    `between` (datetime string order == time order), flag semantics
    (unanalyzed photos never match a flag; `color` = inverse of
    `is_monochrome`), null-ops, unknown field/op/bad value-type → friendly
    validation errors, and a SQL-injection-looking value is bound, not spliced
    (the table survives). Empty filter returns everything, paginated.
  - Statistics engine (Sprint 6, integration `tests/statistics_integration.rs`)
    ✓: a two-session, four-photo seed (two analyzed+EXIF, one unanalyzed
    EXIF, one photo with no EXIF at all — its `indexed_at` pinned so the
    `COALESCE` fallback is deterministic) proves: totals + photos/session;
    analyzed-only averages with the denominator; mono/color and AI face/smile
    shares (present vs `None`); all four fixed-bin histograms; camera/lens
    usage incl. the "Unknown camera/lens" grouping and analyzed-only group
    averages; the monthly trend (only months with data, chronological,
    per-month analyzed-only averages); custom-period scoping incl. scoped
    selection counts vs the global trash count; the empty-period honest zero
    (every average `None`, selection section present-with-zero when a signal
    exists); selection hidden when no signal exists; session summary scoping
    + duration; side-by-side comparison (same metric rows, per-session
    averages); compare size/unknown-id validation; and
    `refresh_all_sessions_times` deriving start/end from the photos.
  - Statistics (Sprint 6, `statistics/bins.rs` + `statistics/mod.rs`) ✓:
    binning edges for all four distributions (ISO 400 → "400–800" boundary
    documented, below-range clips into the first bin, focal nearest-of-set,
    shutter overflow bucket), `bin_counts` keeps zero bins in fixed label
    order; period resolution on a pinned "now" (today/this-week
    Monday-based/this-month + year boundary/this-year + year boundary/custom
    bare-date end-of-day extension/all) and the period JSON parse (invalid
    kinds → friendly error).
- Rename templates (Sprint 7, `filesystem/mod.rs`) ✓: `expand_template` is
  single-pass (an original name containing a literal `{token}` is inserted
  verbatim, then sanitized once — no double expansion); `{sequence}` zero-pads
  to the given width; missing values expand to empty; the extension is always
  the file's own (template `.ext` stripped + original re-attached);
  `sanitize_name` collapses separators/spaces/braces to `-`, trims dangling
  separators, maps empty → `"renamed"`, caps at 150, keeps underscores;
  `suffixed_path` finds the first free `-n` suffix; op/policy string parsing
  rejects unknown values with a friendly error.
- File ops (Sprint 7) ✓:
  - Unit: the template engine + `suffixed_path` + op/policy parsing above.
  - Integration (`tests/fileops_integration.rs`), all on real temp dirs with
    real bytes: group rename actually moves bytes on disk, updates
    `photos.path` (no dangling rows), and writes a `done` audit row per file;
    a no-distinguishing-token template maps two sources to one name → the plan
    **aborts** with an itemized note and executes nothing; a rename onto an
    existing file blocks just that item with `ALREADY EXISTS`; move removes the
    source, lands the dest, and syncs the DB path + audit; copy leaves the
    original and indexes the copy as a second row; `skip` blocks a collision
    while `avoid-by-renaming` resolves to the first free suffix; deleting one
    source before execution makes it a **skipped** item while the rest still
    move (partial failure keeps successes); trash (Linux) moves the file into
    the OS trash `files` dir and removes the DB row + audit; a pre-set cancel
    processes zero items and reports `cancelled`.
- Similarity (Sprint 8): identical image → hash distance 0; perturbed →
  small distance; unrelated → large; clustering groups as expected.

## Frontend tests (`npm test`, Vitest)

- Store behavior (view switching, progress payloads) ✓
- Virtual grid math (Sprint 3): `computeVisibleRange` window/overscan/
  clamping (never past item count, start≤end, zero-item safe, abnormal col
  count) and `computeColumns` fit + min-one-column. The pure math is tested
  so the scroll handler stays a thin wrapper. ✓
  - Filter registry + composition (Sprint 5, `src/tests/filterFields.test.ts`)
    ✓: the TS registry mirrors the Rust field registry (names/kinds/areas);
    operators are constrained per kind; `buildCondition` produces well-typed
    values and rejects empty/invalid input (keeps ISO whole, honors the
    orientation value set, auto-extends a datetime `between` upper bound to
    end-of-day); `chipLabel` uses neutral technical language (no verdicts);
    `draftToFilter` emits the exact wire object the Rust engine parses
    (round-trip JSON asserted).
  - Statistics formatting (Sprint 6, `src/tests/statsFormat.test.ts`) ✓: the
    honest-data rendering — every formatter returns "unavailable" (never
    "0"/"0%") for `null`; metrics to one decimal; shares as undecimaled
    percent; 0–1 kept-ratio → percent; durations (days vs hours vs unknown);
    EXIF formatters (ISO int, `f/2.8`, `1/125` vs `0.600s` fallback);
     `monthLabel` for `YYYY-MM` (+ passthrough of garbage); `periodJson` emits
     the exact wire object the Rust `Period` parses.
   - File-ops wording (Sprint 7, `src/tests/fileopsFormat.test.ts`) ✓: the
     factual operation language — `opVerb` maps each op to "be renamed/moved /
     copied / trashed"; `previewHeadline` counts only the items that will run
     and reports an aborted plan distinctly; `resultHeadline` states "N of M
     &lt;op&gt; complete"; `progressLabel` prefers live progress, then the final
     summary, then a neutral "working…" (never invents a total);
     `flaggedResults` surfaces everything not `done`; `fileBase` is the preview
     path label (null-safe, handles `/` and `\`-separated names).

## Integration (Rust, `tests/`)

End-to-end over a temp library:

```
write synthetic folder (N images + EXIF via a tiny writer or sidecar JPEG)
  → scan_folder → assert counts/duplicates
  → analyze → assert rows + version
  → apply filter → assert result set
  → statistics → assert aggregates
  → plan + execute rename/move/copy/trash (temp dirs) → assert FS, DB sync,
    audit log, collisions, partial failure   (Sprint 7)
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
