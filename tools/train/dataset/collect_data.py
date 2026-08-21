#!/usr/bin/env python3
"""One-command dataset collection for Sprint 17 scene classification.

Runs every stage in order; each stage is resumable (existing outputs are
skipped), so this script can be re-run after any interruption:

  1. metadata CSVs (Open Images v7 + Places365 names)
  2. class mapping (frozen at tools/train/class-map.json if present)
  3. sample selection (single-label, conf >= 0.7, caps)
  4. train thumbnail download (~150k CC-BY images, resumable)
  5. val thumbnail download
  6. dedup + ImageFolder split under ml-corpus/dataset/
  7. summary

Usage:  python3 tools/train/dataset/collect_data.py [--workers 10]
"""
from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent

STEPS = [
    ("metadata", [HERE / "download_metadata.py"]),
    ("class-map", [HERE / "map_classes.py"]),
    ("select", [HERE / "select_samples.py"]),
    ("train-thumbs", [HERE / "download_images.py", "--split", "train"]),
    ("val-thumbs", [HERE / "download_images.py", "--split", "val"]),
    ("build-split", [HERE / "build_split.py"]),
]

def run(name: str, cmd: list) -> None:
    print(f"\n=== [{name}] {' '.join(str(c) for c in cmd)}", flush=True)
    t0 = time.time()
    r = subprocess.run([sys.executable, *cmd], cwd=REPO)
    dt = time.time() - t0
    if r.returncode != 0:
        sys.exit(f"stage '{name}' failed after {dt:.0f}s (exit {r.returncode})")
    print(f"=== [{name}] done in {dt:.0f}s", flush=True)

def summary() -> None:
    samples = REPO / "ml-corpus/openimages/samples"
    thumbs = REPO / "ml-corpus/openimages/images/thumb"
    n_train = sum(1 for _ in open(samples / "train.csv")) - 1
    n_val = sum(1 for _ in open(samples / "val.csv")) - 1
    have = len(list(thumbs.glob("*.jpg"))) if thumbs.exists() else 0
    size = sum(f.stat().st_size for f in thumbs.glob("*.jpg")) / 1e9 if thumbs.exists() else 0
    print(f"\n==== SUMMARY ====")
    print(f"train samples: {n_train}   val samples: {n_val}")
    print(f"thumbnails on disk: {have} ({size:.1f} GB)")
    ds = REPO / "ml-corpus/dataset"
    if (ds / "train").exists():
        n_tr = sum(1 for _ in (ds / "train").rglob("*.jpg"))
        n_va = sum(1 for _ in (ds / "val").rglob("*.jpg"))
        classes = len({p.parent.name for p in (ds / "train").rglob("*.jpg")})
        print(f"ImageFolder: train={n_tr} val={n_va} fine-classes-with-files={classes}")
    print("next: bash tools/train/setup_env.sh && .venv/bin/python tools/train/train.py")

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=10)
    ap.add_argument("--only", nargs="*", choices=[s for s, _ in STEPS])
    args = ap.parse_args()
    for name, cmd in STEPS:
        if args.only and name not in args.only:
            continue
        run(name, cmd + (["--workers", str(args.workers)] if name.endswith("thumbs") else []))
    summary()

if __name__ == "__main__":
    main()