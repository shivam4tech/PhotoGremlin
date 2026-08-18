# Similarity

How PhotoGremlin finds "the same moment" in a shoot — and how it keeps the
language honest: **similar photograph** and **burst**, never "duplicate —
delete this". Deciding stays with the photographer (the Sprint 7 culling +
file-ops pipeline makes the cleanup one click).

Everything here runs on this machine. No AI, no network, no cloud. The whole
core must keep working with local models disabled — similarity is part of the
core, not an AI feature.

## What it finds

Two independent group types:

- **Similar** — photographs whose *perceptual hashes* are close (a
  near-duplicate: re-encode, tiny crop, same shot). "Which of these 40 frames
  are the same moment?"
  - **Within a session** (`SIMILAR_THRESHOLD = 8` bits): the same-moment
    question inside one shoot.
  - **Cross-session** (Sprint 16, `GLOBAL_SIMILAR_THRESHOLD = 4` bits): the
    stricter "was this file imported again?" question. A pair unites only
    when both photos are *in different sessions* (a photo with no session
    never cross-links). Group cards carrying matches over ≥ 2 sessions show a
    "N sessions" chip.
- **Burst** — photographs *captured within seconds of each other*
  (`BURST_WINDOW_SECS = 3`), always within one session. "This run is one
  burst." Time-based, so bursts never span sessions — two shoots at the same
  wall-clock second stay separate.

Both group types need **≥ 2 photos** (`MIN_GROUP_SIZE = 2`); a lone photo
never forms a group. A photo can legitimately appear in both a within-session
and a cross-session group (it *is* both "the same moment" and "imported
twice").

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

## Cross-session similar (Sprint 16)

Same dHash, different (stricter) question:

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
- **No cross-session *auto* anything** — the group appears; deciding stays
  with the photographer.
- **No face/subject-based similarity** (that is the AI pass, Sprint 9, and is
  always optional).
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
