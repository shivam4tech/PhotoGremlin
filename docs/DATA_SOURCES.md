# Data sources for scene classification (Sprint 17-v2)

Every source below is usable in a proprietary commercial product. Provenance
(author, license, source URL) is recorded per image at download time and kept
in the local corpus (never pushed); this page documents the aggregate.

| Tier | Source | License filter | Labels | Volume target | Role |
|---|---|---|---|---|---|
| D | Open Images v7 human-verified | CC-BY 2.0 / PD URLs only | multi-label, conf >= 0.7 | ~62k | clean fine-tune + val |
| A | Open Images v7 machine-generated (`oidv7-train-annotations-machine-imagelabels.csv`, 7.3 GB) | same image pool | machine, conf >= 0.9 | ~25-35k (only ~16% of that pool is CC-BY/PD — measured) | noisy pre-train supplement |
| B | Openverse API (Flickr/Wikimedia/etc.) | `license=by,cc0,pdm` + `license_type=commercial` | query-tagged (class x region matrix) | ~13-15k measured (thin index, 240/query cap) | demographic balance |
| B+ | Wikimedia Commons direct API (deep pagination, 500/page) | extmetadata allowlist: CC0/PD/CC-BY only | query-tagged (same class x region matrix) | 30-80k per ~75min budget | bulk + demographic depth |
| C | Optional manual adds: Unsplash Lite Dataset, Nappy.co | their free licenses | keywords/manual | optional | people-class balance |

## Region matrix (Tier B)

`openverse_crawl.py` queries every class name plus `{region} {class}` for a
rotating sample of 8 of these 30 regions per class: indian, nigerian,
ethiopian, kenyan, egyptian, moroccan, chinese, japanese, korean, vietnamese,
thai, indonesian, filipino, turkish, iranian, arab, israeli, russian, polish,
greek, spanish, portuguese, italian, mexican, brazilian, peruvian, colombian,
argentinian, cuban, nepalese.

Purpose: counter the Western skew of Flickr-sourced corpora — weddings,
churches, markets and streets exist everywhere and the model must see that.
10% of Tier B rows are held out (`corpus_v2/audit.csv`) as a region-bias
audit set; `eval_ckpt.py --gates` fails the run if region accuracy deltas
are too large.

## Rules

- No share-alike licenses (CC-BY-SA excluded everywhere).
- No research-only datasets (Places365/CEW/RT-BENE/KonIQ etc. stay out).
- Corpora/checkpoints/provenance never enter git; scripts and class-map do.
- Attribution ships as docs notes per bundled model artifact.

## Developer-only photo application regression corpus

`tools/datasets/download_photo_test_corpus.py` builds a separate, 1,000-image
photo-application test corpus from Wikimedia Commons originals. It is for
manual scanning, filtering, metadata and culling evaluation; it is not an ML
training source and does not change the application's offline runtime policy.

The script creates `../photogremlin-test-corpus/` by default, beside this
repository. It separates camera/genre sources into folders, records original
URLs, file pages and file-specific license details in `manifest.jsonl`, and
accepts only JPEGs whose downloaded EXIF contains Make, Model and
DateTimeOriginal. It is resumable through that manifest, `.part` download
files and `state.json` category cursors.

```
python3 tools/datasets/download_photo_test_corpus.py --list-sources
python3 tools/datasets/download_photo_test_corpus.py
```

The resulting photos, manifests and provenance remain local development data:
they must never enter this repository or any release bundle.

## Commands

```
bash tools/train/dataset/collect_v2.sh          # tiers A+B+merge, resumable
# stage 1: noisy pre-train
tools/train/.venv/bin/python tools/train/train.py --corpus ml-corpus/corpus_v2/train.csv --epochs 5 --mixup 0.2
# stage 2: clean fine-tune
tools/train/.venv/bin/python tools/train/train.py --corpus ml-corpus/corpus_v2/train_clean.csv --init-from tools/train/runs/<ts>/last.pt --epochs 10
# calibrate + evaluate gates
tools/train/.venv/bin/python tools/train/calibrate.py --checkpoint tools/train/runs/<ts2>/best.pt
tools/train/.venv/bin/python tools/train/eval_ckpt.py --checkpoint tools/train/runs/<ts2>/best.pt --gates
```
