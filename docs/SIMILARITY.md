# Similarity

How PhotoGremlin finds "the same moment" in a shoot — and how it keeps the
language honest: **similar photograph** and **burst**, never "duplicate —
delete this". Deciding stays with the photographer (the Sprint 7 culling +
file-ops pipeline makes the cleanup one click).

Everything here runs on this machine. No AI, no network, no cloud. The whole
core must keep working with local models disabled — similarity is part of the
core, not an AI feature.

## What it finds

Three independent group types, all scoped to the project currently open:

- **Similar** — photographs whose *perceptual hashes* are close (a
  near-duplicate: re-encode, tiny crop, same shot). "Which of these 40 frames
  are the same moment?"
- **Face appearance** — optional candidates from the local face pass. YuNet
  supplies a face box; PhotoGremlin hashes only that crop plus a small margin
  and groups close crop hashes at `FACE_APPEARANCE_THRESHOLD = 10`. This is a
  repeat-portrait aid, **not identity recognition**: the UI never names a
  person and the photographer reviews every candidate.
- **Burst** — photographs *captured within seconds of each other*
  (`BURST_WINDOW_SECS = 3`), always within one session. "This run is one
  burst." Time-based, so bursts never span sessions — two shoots at the same
  wall-clock second stay separate.

All group types need **≥ 2 photos** (`MIN_GROUP_SIZE = 2`); a lone photo
never forms a group. A photo may appear in more than one group when it is
both a burst frame and a visual or face-appearance candidate.

## The algorithm (dHash)

A **64-bit difference hash** (`dhash64`):

1. Decode the image to grayscale.
2. Resize to **9 × 8** with a Lanczos3 filter.
3. For each of the 8 rows, compare each pixel to the one on its right:
   `bit(r,c) = 1` iff `pixel(r,c) < pixel(r,c+1)`.

That is 8 × 8 = 64 bits. Measuring the *direction of change* (not absolute
levels) is what makes it stable across mild re-encoding: a JPEG re-save shifts
levels slightly but rarely flips which neighbor is darker, so the two hashes
stay close.

- **Distance** is the **Hamming distance** (popcount of XOR).
- **Threshold** `SIMILAR_THRESHOLD = 6`: at or below 6 differing bits →
  "similar". Unrelated photos land near 32 ± a few bits; near-duplicates stay
  well under 6. `8 → 6` tightened after telemetry on 400-image cards showed
  ~100 tiny groups at 8; 6 collapses the 2–3 frame noise without losing true
  similar. (Pinned by unit tests: distinct content must exceed the
  threshold; mild noise must stay under it.)
- **Grouping** is union-find: within a session, every pair within the
  threshold is merged into one component; each component of ≥ 2 photos is a
  group. Deterministic (input order + sorted output).

All constants are `pub` in `src-tauri/src/similarity/mod.rs` and asserted by
unit tests, so changing one is a conscious, tested act.

## Project ownership (v16)

The active-project workflow intentionally has no cross-project matching.
`similarity_groups.session_id` owns each stored set: a pass captures the
active session, reads only that session's photos and face observations, and
atomically replaces only that session's groups. Opening or closing another
project never removes these results, while list and group-photo queries apply
the same project check on read. The remaining notes below describe legacy
helpers retained for old test coverage, not the active UI workflow.

- **`GLOBAL_SIMILAR_THRESHOLD = 4`** — importing the same file twice (or a
  re-encode of it) lands at distance 0–3; looser matches are same-moment
  questions and stay within their session. Only pairs in **different
  sessions** (both with a session) can unite; NULL (unsessioned) photos never
  cross-link.
- **Entropy guard** — featureless frames (flat sky, walls) hash to ~0 or
  ~all-ones (`degenerate_hash`: popcount ≤ 2 or ≥ 62). Those are
  *undifferentiated*, not "similar", so they are excluded from the
  cross-session pass entirely; two flat photos must never weld into a
  library-wide group. Within-session behavior is untouched.
- **Cost control** — an all-pairs sweep is O(n²), painful for lifetime-size
  libraries. `cross_session_groups` slices each hash into overlapping 16-bit
  windows (stride 12; the union covers all 64 bits) and compares pairs only
  inside equal-window buckets. Union-find makes repeated comparisons
  harmless. Exactness: any pair that differs in ≤ 2 bits shares a window
  (their other windows avoid the flips), so the bucketed result is identical
  to the all-pairs sweep for distances ≤ 2; at d = 4 the miss rate is a
  fraction of a percent. The equivalence is asserted by a randomized unit
  test against an all-pairs reference.
- **Provenance** — `list_similarity_groups` computes `session_count`
  (`COUNT(DISTINCT session_id)` over group members); the UI renders a
  "N sessions" chip on group cards when ≥ 2. No schema change: group tables
  are rebuilt every pass.

## Where it lives

- **Rust core** — `src-tauri/src/similarity/` (Tauri-free, integration-tested):
  `dhash64`, `hamming`, `photo_hash`, `group_similar` (union-find),
  `group_bursts` (time-window), and the orchestrating `run_similarity`.
- **DB** — the hash is a **column on `photos`** (`phash INTEGER`, v9) plus
  `phash_source_mtime` (the mtime it was computed from). Groups live in
  `similarity_groups` / `similarity_group_photos`. See DATABASE.md.
  - Note the hash is stored on `photos.phash`, **not** `analysis.perceptual_hash`.
    `analysis.perceptual_hash` is reserved for a future analysis-pass column;
    the similarity pass owns `photos.phash` directly.
- **Commands** — `src-tauri/src/commands/similarity.rs`:
  `start_similarity` (background), `stop_similarity`, `list_similarity_groups`,
  `group_photos`.
- **Frontend** — the dedicated Groups workspace uses
  `src/features/similarity/CoverThumb.tsx` for cover strips and the existing
  paged `group_photos` command for its virtual grid/viewer path. All/Similar/
  Burst/Face appearance tabs keep matches reachable without placing completed
  group cards above the main Library photographs. Face wording explicitly
  describes local appearance matching rather than identity.

## Incremental re-hash rule

Mirrors the analysis rule (`analysis.source_mtime`) but on its own column,
so the two passes stay independent:

- **Hash** a photo iff `phash IS NULL`, **or** `phash_source_mtime` is
  recorded and the file's current `file_mtime` is newer than it.
- `phash_queue()` returns exactly those, in capture-time order.

So a re-run hashes only what's new or changed. A re-scan refreshes
`photos.file_mtime`, which is what makes a changed-on-disk file re-queue
automatically.

## The pass (`run_similarity`)

Takes `Arc<Db>`, a progress callback, and a cancel flag — the same
claim-and-cancel `Job` model as scan/analysis/metadata (the `similarity` slot
in `AppState`).

1. **Hash** everything in `phash_queue()`: decode → `dhash64` →
   `upsert_phash`. Cancellation takes effect **between files** (it never cuts
   a file mid-hash). Unreadable/undecodable files are skipped and counted
   (`failed`), logged individually — one bad file never aborts the pass.
   Progress streams per file (`similarity-progress`, stage `hashing`).
2. **Group**: read `hashed_photos_for_session(active_session)`, run
   `group_similar`, `group_bursts`, and (when available)
   `group_face_appearances`. Capture time affects bursts only, never visual or
   face-appearance candidates.
3. **Persist**: `replace_similarity_groups_for_session` swaps only that
   project's group set in one transaction.

**Grouping still runs after a cancel**, over whatever got hashed — so the app
always ends on a *consistent* group set (never half-groups). Cancellation is
always reported (`cancelled: true`) and the summary returned.

**`SimilaritySummary`** = `{ hashed, failed, similar_groups, burst_groups,
face_groups, elapsed_ms, cancelled }`, carried in `similarity-complete`.

## What we do NOT do (v0.1)

- **No automatic deletion or "keep best".** We surface groups; the user
  decides (via culling + file ops).
- **No person identity or name inference.** Face-appearance candidates are
  local, optional and deliberately labelled as candidates.
- **No permanent delete** anywhere — trash only (Sprint 7).

## Testing

- **Rust unit tests** (`similarity::tests`): dHash determinism, distinct
  content exceeds the threshold, mild noise stays under it, solid image,
  union-find clustering, min-group-size-2, burst clumping + ignoring unknown
  times, RGB→gray helper; Sprint 16: window bucketing ≡ all-pairs sweep
  (randomized reference), degenerate-hash entropy guard, cross-session
  rule (same-session pairs never cross-link, NULL sessions never cross).
- **Rust integration** (`tests/similarity_integration.rs`): real temp JPEGs →
  the full pass on a real DB: the re-encoded pair forms one similar group,
  a distinct scene does not join, a ≤3s trio bursts and a 30s-later photo
  does not, the incremental rule re-queues exactly the modified file, groups
  persist with covers, an immediate cancel leaves a consistent (empty) set;
  Sprint 16: a cross-session re-encode joins a stricter 4-bit group while
  flat frames are excluded, bursts stay session-scoped, and group cards carry
  `session_count`.
- **Frontend** (`src/tests/organizeLabels.test.ts`): the group/similar/collection
  wording is deterministic and stays factual.

See TESTING.md.

## Tuning

- **Threshold** (`SIMILAR_THRESHOLD`): raise → fewer, tighter similar groups
  (fewer false "similar"); lower → more, looser groups. Default 8.
- **Burst window** (`BURST_WINDOW_SECS`): widen → longer runs counted as one
  burst. Default 3.
- **Min group size** (`MIN_GROUP_SIZE`): floor on group cardinality. Default 2.

Changing any of these changes grouping but **not** the hash — a re-run
re-groups from the existing `phash` values, so it is cheap. A hash change
(would require a dHash change) is a much bigger deal and currently not
versioned — revisit if the hash algorithm ever changes.
