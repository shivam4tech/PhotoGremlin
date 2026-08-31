# Local AI

Philosophy first: **AI is the smallest possible layer of PhotoGremlin.**

- Optional. The entire core product — scan, thumbnails, EXIF, sharpness,
  brightness/contrast/saturation, clipping, monochrome, filtering,
  similarity, statistics, rename/move/copy/trash/permanent delete, collections, saved views —
  must work with AI completely disabled. That is a hard product requirement,
  not a preference.
- Local only. No OpenAI API. No RunInfra API. No cloud vision. No remote
  inference. No model download at runtime (models are bundled or explicitly
  user-installed files, with size and hash disclosed in Settings).
- Lightweight. No multi-gigabyte downloads. Small ONNX-class models (ONNX
  Runtime or equivalent) inside the Rust backend under an isolation boundary.

## Isolation boundary

`src-tauri/src/ml/` is the only module allowed to load models. It exposes
plain results to the rest of the app:

```
ml::runtime_status()        -> Result<(), friendly reason>
ml::run_faces_pass(db, progress, cancel) -> AppResult<FaceSummary>
```

If the module is absent/disabled, `analysis.face_count` and `smile_count`
remain `NULL`, face/smile statistics stay honest-"unavailable" (not 0,
not "none"), and everything else is unaffected. The rest of the codebase
never names `ort` and never knows the model exists.

## Scope by release

| feature | release | status |
|---|---|---|
| face detection / face_count / faces_present | v0.1 (Sprint 9) | **shipped** — YuNet 2023mar (below) |
| smile detection / smile_count | v0.1 → **v0.2 if not stable** | **deferred to v0.2**: no small, stable, permissively-licensed local smile model was available; the plan explicitly defers it |
| eyes-open detection | v0.3 | planned |
| face-appearance candidates | v0.1 | **shipped** — local detected-face crop dHash; review candidates only, never identity |
| face grouping (identity) | v0.3 | planned; requires a separate local embedding model and explicit privacy review |
| scene classification / scene_group filter | v0.2 (Sprint 17–18) | **shipped** — MobileNetV3-Large two-head ONNX trained on CC-BY Open Images corpus; see SCENE_CLASSIFICATION.md and UI_GUIDELINES.md for the honest-confidence display rules |
| semantic classification / local text search | v0.3 | planned |

If any model proves to destabilize the app, or the build/size cost is not
justified, it is deferred — never at the expense of core stability.

## Presenting results

Faces and smiles are **technical measurements** shown alongside sharpness
and clipping: "faces: 2", "smiling: 1". Filters: "contains faces",
"smiling photographs". No "happy photo", no "good portrait".

---

# Implementation (Sprint 9)

## Model

**YuNet 2023mar** — the face detector from the OpenCV Zoo (`zoo/model/
face_detection_yunet/face_detection_yunet_2023mar.onnx`), Apache-2.0
licensed. It is a ~232 KB single-file ONNX model — small enough to embed in
the binary without changing its size character.

- Embedded at `src-tauri/src/ml/mod.rs` via `include_bytes!` from
  `src-tauri/models/face_detection_yunet_2023mar.onnx`.
- Size: **232,589 bytes**.
- SHA-256: `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`
- Settings discloses the name, license and size (the "Local intelligence"
  card). The hash is pinned here as the integrity record.

## Runtime: ONNX Runtime via `libloading`

The ONNX Runtime C library is **not** bundled; it is dlopened at runtime
(ort's `load-dynamic` mode). Why this combination of crates is pinned so
hard (all of it is non-obvious and cost real time):

- `ort` (Rust bindings): the entire 1.x series and 2.0.0-alpha are
  **yanked** on crates.io (invisible to new dependencies), and the
  `load-dynamic` feature was **removed after 2.0.0-rc.9** (later releases
  link via pkg-config at build time, which would force shipping/embedding
  the runtime — the opposite of this design). Hence
  `ort = "=2.0.0-rc.9", default-features = false, features = ["load-dynamic"]`.
- `ort-sys` (FFI layer): pinned `=2.0.0-rc.9` as well — rc.10 changed the
  generated `OrtApiBase` bindings from `Option<fn>` fields to raw function
  pointers, and ort rc.9 does not compile against rc.10+. An unpinned
  requirement would silently break on the next `cargo update`.
- `libloading` is a direct dependency (the same version ort's
  `load-dynamic` uses) because **ort panics, not errors, when the library
  is missing**. `ml` therefore probes the library with `libloading` first
  (candidate list below) and only calls into `ort` when the probe passes —
  that probe is what makes "AI unavailable on this machine" a friendly
  status instead of a crash.

Library resolution (probed once, cached): the canonical soname first —
`libonnxruntime.so.1` (Linux), `libonnxruntime.dylib` (macOS),
`onnxruntime.dll` (Windows) — then, on Linux, a scan of standard library
directories (`/usr/lib/*-linux-gnu`, `/usr/lib64`, `/usr/lib`,
`/usr/local/lib`) for `libonnxruntime.so.1.<minor>`, highest minor version
first. Some installs ship only the versioned file with no soname symlink;
the scan covers exactly that (the dev box is one of them:
`libonnxruntime.so.1.23` only).

### Installing the runtime (user-facing, v0.1)

- **Linux:** any ONNX Runtime distribution with the shared library
  (e.g. the `libonnxruntime` package of the distro, or the OpenAI
  prebuilt archive's `.so` in a standard lib dir).
- **macOS:** e.g. `brew install onnxruntime`.
- **Windows:** ONNX Runtime MSVC prebuilds (DLL next to the app or on PATH).

When it is missing, the Settings card says face detection is off on this
machine and everything else works; the face pass refuses to start with the
same friendly note. Nothing in the app depends on it.

## Pipeline (what inference actually does)

All of this was validated against OpenCV's own `FaceDetectorYN` reference
implementation on the committed test portrait (score 0.93 vs its 0.95, box
center within ~1% of full resolution, size within ~8% — the resize engines
differ slightly; the face is large and the target is a count, so the delta
is immaterial).

1. **Input:** the photo is decoded (256 MB file guard, 250 MP pixel guard —
   see below), resized *distorted* (no aspect preservation, that's what the
   reference does) to the model's fixed **640×640** via linear
   interpolation (`image::FilterType::Triangle`), channels kept in **BGR**
   order (the OpenCV convention — the blob is NOT swapped to RGB) with the
   per-channel mean **(104, 177, 123)** subtracted, laid out CHW
   float32 `[1, 3, 640, 640]`. The model's input is strictly 640² — it
   rejects other sizes — so the whole pipeline is pinned to it.
2. **Outputs:** three detection scales (feature maps 80², 40², 20²;
   strides 8, 16, 32), each with objectness `obj`, face classification
   `cls`, box deltas `bbox (dx,dy,dw,dh)` and keypoints `kps` (decoded as
   part of the pipeline but not stored — v0.1 counts faces).
3. **Decode** (the official libfacedetection math, not a re-derivation):
   anchors sit at **cell corners** (`ax = c·stride`, `ay = r·stride`,
   offset 0); center = `delta·stride + anchor`; size = `exp(delta)·stride`;
   score = **`sqrt(cls·obj)`** — the sigmoids are *inside the ONNX graph*,
   so there is no post-processing sigmoid (verified: adding one collapses
   the score against the reference).
4. **Filter + merge:** `score ≥ 0.7` (the reference default), cross-scale
   NMS at IoU `0.3`, `top_k = 100` (plenty; the pass counts, never
   renders, boxes).
5. **Back-map:** blob-space boxes scale by `(W₀/640, H₀/640)` to original
   pixels. `face_count` = the number of surviving boxes (0 is a real
   result, stored as such).
6. **Face-appearance candidate hash:** each valid back-mapped face crop plus
   an 18% margin becomes a local grayscale dHash. Only that signed 64-bit
   integer and its photo id/index are persisted in `face_observations`; no
   face pixel or identity embedding is stored. The similarity pass compares
   these hashes only within the active project and presents matches for review
   as “matching face appearances,” never as a named person.

Constants live in `src-tauri/src/ml/mod.rs` (`INPUT_SIZE`, `MEAN_BGR`,
`FACE_SCORE_THRESHOLD`, `FACE_NMS_THRESHOLD`, `FACE_TOP_K`, `STRIDES`) and
are pinned by unit tests, including the decode formula itself.

## Incremental rule + guards

Mirrors the similarity pass, on `analysis.faces_at`:

- `faces_queue()` = decodable photos where `face_count IS NULL`, or
  `file_mtime` is newer than the `faces_at` stamp (captured-time order).
- A run re-detects only those; a re-run over an unchanged library is a
  no-op (integration-tested).
- **Guards** (v0.1 keeps them simple and documented):
  - file > 256 MB, or > 250 MP: stamped `face_count = 0` with a log line
    (the queue must not re-attempt it forever — the metadata pass's
    oversize precedent);
  - file missing: friendly per-file failure, **not** stamped (a re-scan may
    restore it);
  - undecodable / inference error: friendly per-file failure, retried on
    the next run; the first errors surface in the summary, the log has all.

## What is stored, and how it coexists with analysis

- `analysis.face_count` (the count) + `analysis.faces_at` (v10: the file
  mtime the count was computed from).
- `face_observations(photo_id, face_index, appearance_hash, source_mtime)`
  (v17): replaced transactionally with each face result so changed files have
  no stale face-appearance candidates.
- A photo that has faces but no measurements yet gets a **face-only
  analysis row**: the face pass must be able to store a count without
  fabricating any measurement. Its `sharpness`/… columns stay `NULL` until
  the analysis pass runs — `upsert_analysis` updates only analysis-owned
  columns and **never clobbers `face_count`/`faces_at`** (integration-
  tested), and the NULL-safe `source_mtime` comparison means a face-only
  row re-enters the analysis queue automatically.
- `DbStatus.faces_done` (count of rows with `face_count IS NOT NULL`) feeds
  Settings and the status bar.
- **Smile detection (v0.2):** `smile_count` stays `NULL` in v0.1 and
  statistics keep reporting "unavailable" — never 0, never "none".

## Commands, events, settings

- `ai_status` → `{ enabled, runtime_available, runtime_note, model,
  model_bytes, faces_done, photo_count }` (the Settings card).
- `set_ai_enabled(bool)` — persists the preference (`app_settings` key
  `ai_enabled`, **off by default**). Turning it on starts nothing by itself.
- `start_faces` / `stop_faces` — claim the **faces** job slot (separate
  from the similarity slot; the UI keeps them exclusive), spawn the
  sequential pass, stream `faces-progress` / `faces-complete`
  (`FaceSummary { processed, with_faces, failed, cancelled, elapsed_ms,
  errors }`). Cancellation is cooperative between files; already-stamped
  results are kept.
- **Auto-run:** when `ai_enabled` is on, the app starts the face pass
  automatically after a scan that indexed new photographs (right after the
  existing metadata auto-run; a no-op when nothing is queued). Manual runs
  are always available from Settings.
- The pass is sequential by design: one decode + one 640² inference per
  file through a single session — a few hundred photos per minute on CPU.
  The queue stays small because it is incremental; if a future library gets
  slow, workerization is the obvious next step (the per-file work is
  independent and `ort::Session` is `Send + Sync`).

## Frontend

- `AiStatus` / `FaceSummary` / `FaceCompletePayload` in `src/types/api.ts`;
  `aiStatus`/`setAiEnabled`/`startFaces`/`stopFaces` in `src/lib/ipc.ts`.
- `appStore`: `aiEnabled` (off by default), `aiStatus`, `detectingFaces`,
  `facesProgress`, `facesSummary`.
- Settings → **"Local intelligence"** card (`views/SettingsView.tsx`):
  on/off toggle (disabled + explained when the runtime is missing), runtime
  line, model provenance + embedded size, "N of M photographs checked for
  faces", run-now / stop buttons with live progress, last-pass summary, and
  one example error when files failed. Pure wording helpers live in
  `src/features/settings/ai.ts` (unit-tested in
  `src/tests/settingsAi.test.ts`); the language is factual
  ("42 of 1,000 photographs checked for faces"), never evaluative.
- `App.tsx` owns the two faces listeners + the scan auto-run (same place as
  the metadata auto-run).

## Tests

- **Unit (Rust, `ml::tests`):** blob layout (CHW/BGR/mean), the decode
  formula (offset-0 anchors, stride-scaled deltas, `exp` sizes, `sqrt`
  score), sub-threshold rejection, NMS suppression + determinism, IoU
  bounds, runtime-status consistency.
- **Integration (`tests/ml_integration.rs`, real model, real DB):**
  fixture portrait ≥ 1 face + two synthetic striped JPEGs = 0 faces;
  re-run is a no-op; newer `file_mtime` re-queues exactly the touched photo;
  a face-only row is queued for analysis and survives `upsert_analysis`
  (count + stamp preserved); pre-set cancel stops before any work; a
  missing file is a friendly failure that stamps nothing; pass outcome and
  `runtime_status()` agree (both paths) even when the runtime is absent.
- **Test fixture:** `src-tauri/tests/fixtures/face_portrait.jpg` — a
  White House / U.S. GPO official portrait (public domain), 560×700.
  SHA-256:
  `d304de10e992a084a63a320cd055c430ad1bf985dd268d5583995179089160f5`
- **Frontend:** store defaults + the `ai.ts` wording module (Vitest).

## Out of scope for v0.1 (deliberate)

- Smile detection (→ v0.2, see the scope table).
- Face boxes drawn in the viewer / per-face data — the product need in
  v0.1 is "which photographs contain faces" (filtering, statistics,
  culling context); boxes need a rendering design of their own.
- GPU execution providers (ROCm/CUDA/Metal): CPU-inference parity across
  three desktop OSes first; providers are an ort flag away later.
- Identity / clustering (→ v0.3).
