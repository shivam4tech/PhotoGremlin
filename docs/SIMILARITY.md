# Similarity

How PhotoGremlin finds "the same moment" in a shoot — and how it keeps the
language honest: **similar photograph** and **burst**, never "duplicate —
delete this". Deciding stays with the photographer (the Sprint 7 culling +
file-ops pipeline makes the cleanup one click).

Everything here runs on this machine. No AI, no network, no cloud. The whole
core must keep working with local models disabled — similarity is part of the
core, not an AI feature.

## What it finds

Two independent group types, both scoped to **one session** (a shoot):

- **Similar** — photographs whose *perceptual hashes* are close (a
  near-duplicate: re-encode, tiny crop, same shot). "Which of these 40 frames
  are the same moment?"
- **Burst** — photographs *captured within seconds of each other*
  (`BURST_WINDOW_SECS = 3`). "This run is one burst."

Cross-session duplicate detection is a **v0.2 concern** (deferred, not
dropped). Within a session is where the question actually happens.

Both group types need **≥ 2 photos** (`MIN_GROUP_SIZE = 2`); a lone photo
never forms a group.

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
- **Threshold** `SIMILAR_THRESHOLD = 8`: at or below 8 differing bits →
  "similar". Unrelated photos land near 32 ± a few bits; near-duplicates stay
  well under 8. (Pinned by unit tests: distinct content must exceed the
  threshold; mild noise must stay under it.)
- **Grouping** is union-find: within a session, every pair within the
  threshold is merged into one component; each component of ≥ 2 photos is a
  group. Deterministic (input order + sorted output).

All constants are `pub` in `src-tauri/src/similarity/mod.rs` and asserted by
unit tests, so changing one is a conscious, tested act.

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
- **Frontend** — `src/features/similarity/CoverThumb.tsx` + the "Similar
  groups" panel in LibraryView (cards with cover strips; click a card to open
  the group's photographs in the same grid/viewer path). The language is kept
  factual there.

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
2. **Group**: read `hashed_photos()`, bucket by session (NULL session = one
   bucket). Per bucket, run `group_similar` (similar) and `group_bursts`
   (bursts). A burst is labelled `burst:<earliest capture epoch>` (a stable id);
   a similar group is labelled with the hex dHash of its first photo.
3. **Persist**: `replace_similarity_groups` swaps the whole group set in one
   transaction.

**Grouping still runs after a cancel**, over whatever got hashed — so the app
always ends on a *consistent* group set (never half-groups). Cancellation is
always reported (`cancelled: true`) and the summary returned.

**`SimilaritySummary`** = `{ hashed, failed, similar_groups, burst_groups,
elapsed_ms, cancelled }`, carried in `similarity-complete`.

## What we do NOT do (v0.1)

- **No automatic deletion or "keep best".** We surface groups; the user
  decides (via culling + file ops).
- **No cross-session duplicate detection** (v0.2).
- **No face/subject-based similarity** (that is the AI pass, Sprint 9, and is
  always optional).
- **No permanent delete** anywhere — trash only (Sprint 7).

## Testing

- **Rust unit tests** (`similarity::tests`): dHash determinism, distinct
  content exceeds the threshold, mild noise stays under it, solid image,
  union-find clustering, min-group-size-2, burst clumping + ignoring unknown
  times, RGB→gray helper.
- **Rust integration** (`tests/similarity_integration.rs`): real temp JPEGs →
  the full pass on a real DB: the re-encoded pair forms one similar group,
  a distinct scene does not join, a ≤3s trio bursts and a 30s-later photo
  does not, the incremental rule re-queues exactly the modified file, groups
  persist with covers, an immediate cancel leaves a consistent (empty) set.
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
