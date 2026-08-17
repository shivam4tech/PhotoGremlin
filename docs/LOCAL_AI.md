# Local AI

Philosophy first: **AI is the smallest possible layer of PhotoGremlin.**

- Optional. The entire core product — scan, thumbnails, EXIF, sharpness,
  brightness/contrast/saturation, clipping, monochrome, filtering,
  similarity, statistics, rename/move/copy/trash, collections, saved views —
  must work with AI completely disabled. That is a hard product requirement,
  not a preference.
- Local only. No OpenAI API. No RunInfra API. No cloud vision. No remote
  inference. No model download at runtime (models are bundled or explicitly
  user-installed files, with size and hash disclosed in Settings).
- Lightweight. No multi-gigabyte downloads. Small ONNX-class models (ONNX
  Runtime or equivalent) inside the Rust backend under an isolation boundary.

## Isolation boundary

`src-tauri/src/ml/` is the only module allowed to load models. It exposes
plain results to the analysis pipeline:

```
fn faces(photo_bytes) -> Option<FaceResult { count, smiles }>;
```

If the module is absent/disabled, `analysis.face_count` and `smile_count`
remain `NULL`, face/smile filters return "unavailable" (not 0, not "none"),
and everything else is unaffected. Tests for the core pipeline run with the
ML module compiled out of the path.

## Scope by release

| feature | release | status |
|---|---|---|
| face detection / face_count / faces_present | v0.1 (Sprint 9) | best-effort in the 10-sprint plan; ships only if stable |
| smile detection / smile_count | v0.1 → **v0.2 if not stable** | explicitly deferrable per plan |
| eyes-open detection | v0.3 | planned |
| face grouping (identity) | v0.3 | planned; local embedding model |
| semantic classification / local text search | v0.3 | planned |

If any model proves to destabilize the app, or the build/size cost is not
justified, it is deferred — never at the expense of core stability.

## Presenting results

Faces and smiles are **technical measurements** shown alongside sharpness
and clipping: "faces: 2", "smiling: 1". Filters: "contains faces",
"smiling photographs". No "happy photo", no "good portrait".
