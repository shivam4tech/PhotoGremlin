# Scene classification (Sprint 17 plan — training; Sprint 18 — app integration)

Status: **planned 2026-08-18, approved, not yet executed.** This page is the
frozen plan so nothing discussed gets lost. Sprint 17 produces the dataset
pipeline + the trained model artifact; Sprint 18 wires it into the app.

## Goal

A local, on-device **scene/subject classifier** for PhotoGremlin's v0.3
"subject classification / semantic local search", built by **training
MobileNetV3-Large** on a large dataset, exported as a small ONNX artifact.
Same shape as the YuNet face detector: optional local intelligence that the
core works perfectly without (hard rule 5).

## Why MobileNetV3-Large + a real training run

- MobileNetV3-Large is the best accuracy/size point in the MobileNet family
  (~5.4M params; int8 ONNX ≈ 5.6 MB, similar precedent to YuNet's 232 KB but
  for a 250–330-way classifier).
- It is trained (fine-tuned) from ImageNet-pretrained torchvision weights on
  our scene corpus — transfer learning, not from scratch.
- Two-tier design, one backbone, two heads:
  - **Coarse head** (30–60 broad groups) → filter chips, target **≥ 95% top-1**.
  - **Fine head** (~250–330 scenes) → scene tag (top-1/top-3 in UI).

## Dataset decision (important — licensing)

**Places365 is research/non-commercial only and is NOT used as the image
source** — training a commercial product on it is a license breach, and
discovery (due diligence, litigation) is realistic. Instead:

- **Taxonomy:** the Places365 365 class *names* (words are not copyrightable).
- **Images:** **Open Images v7** (Google) — annotations **CC BY 4.0**, images
  **CC BY 2.0** (attribution-only, no share-alike, no non-commercial clause)
  → $0 and commercial-safe. Verified: 9M images, 20,638 image-level classes,
  5,000 trainable; Google recommends the **human-verified** labels for
  training (they "practically eliminate false positives").

### Scale (locked)

- ~1.1M train images (cap ~3.5k/class, ~250–330 mapped classes),
  256px thumbnails → **~70GB** total on disk (292GB free).
- Val: Open Images' own val split only (~50/class, no leakage).
- Download: label CSVs (~3GB) + resumable concurrent thumbnail download
  (overnight, 6–20h).

### Pipeline (Sprint 17a, `tools/train/dataset/`)

1. Fetch metadata: `class-descriptions.csv`, `classes-trainable.txt`,
   human-verified image-level labels (train+val), image metadata CSV
   (URL/license/author).
2. Map 365 names → OI MIDs (fuzzy + manual review); commit `class-map.json`
   (fine → coarse group → MID). Expect ~10–20% of niche classes dropped.
3. Select: human-verified only, confidence ≥ 0.7, train cap ~3.5k/class.
4. Download CC-BY rows only; keep **PROVENANCE.csv** (image, author, URL,
   license) in the corpus for attribution.
5. Content-hash dedup; multi-label images assigned to their best class
   (single-label training).
6. Emit ImageFolder layout under `ml-corpus/` (gitignored).

## Training (Sprint 17b, `tools/train/`)

- venv on **python3.11** (torch wheels; system python3 is 3.14 — no wheels).
- `train.py`: MobileNetV3-Large, ImageNet init, two heads (coarse + fine),
  joint loss; bf16 mixed precision; batch 128 @ 224px on the RTX 5060
  (8GB VRAM) → ~1–1.5 h/epoch; 8 epochs; cosine LR; per-epoch val eval.
- Runs under `tools/train/runs/` (gitignored); best checkpoint → export.

## Accuracy expectations (grounded, honest)

Published Places365-standard results (same task family):

| Model | top-1 | top-5 |
|---|---|---|
| Best published (ViT-H/MAE 448) | 60.7% | — |
| ResNet-152 | 54.7% | 85.0% |
| MobileNetV3 family | 53–56% | ~85–90% |

**95% top-1 on ~300 fine classes is not achievable (world SOTA ≈ 60%).** The
95%+ target applies to the **coarse tier only**. Fine tier: top-1 ~55–60%,
top-5 ~85–90%; the UI uses top-3 tags + confidence gating so it never
overclaims. Ground-truth label precision (OI human-verified + our mapping)
≈ 93–97%.

## Export (Sprint 17c)

- ONNX export → int8 static quantization (val calibration) → verify top-1
  drop < 2% → commit artifact to `src-tauri/models/scene_mobilenetv3_large.onnx`.
- CC-BY corpus attribution note in docs (ships with the app, like YuNet's).

## App integration (Sprint 18 — mirror of the YuNet pattern)

- `ml/scene.rs`: 224×224 preprocess → ONNX → coarse tag + fine tag/confidences.
- `analysis` columns: `scene_coarse`, `scene_fine`, `scene_conf`,
  `scene_mtime` (incremental queue like `face_count`); background command
  with progress events; model absent → columns NULL, app fully functional.
- New "scene" filter area (coarse in-list); scene tag in metadata panel +
  tile badge; semantic search later.
- Docs: LOCAL_AI.md, FILTER_ENGINE.md, DATABASE.md, ROADMAP.

## Git hygiene (rule 16)

Corpus, checkpoints, PROVENANCE.csv, runs → never pushed (`.gitignore`
already covers `ml-corpus/`, `tools/train/data/`, `tools/train/runs/`).
Committed: scripts, `class-map.json`, this plan, the final ONNX artifact.

## Timeline & cost

~2–3 days wall-clock (downloads + training run overnight), **$0**, commercial
-safe via CC-BY corpus + documented attribution.
