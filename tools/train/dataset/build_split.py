#!/usr/bin/env python3
"""Build the torchvision ImageFolder layout from downloaded thumbs.

Content-hash dedup per class (identical bytes can't repeat within a class);
the train/val splits come from samples/{train,val}.csv (OI's own split, so
no leakage). Files are hardlinked into
  ml-corpus/dataset/{train,val}/<coarse>/<fine>/<ImageID>.jpg
Single-label assignment was already done by select_samples.py; an image can
only appear in one class file, so no cross-class dedup is needed.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import pathlib

def sha1(path: pathlib.Path) -> str:
    h = hashlib.sha1()
    with open(path, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()

def build(samples_csv: pathlib.Path, thumbs: pathlib.Path, split_dir: pathlib.Path) -> None:
    seen: dict[str, str] = {}
    ok = skipped = missing = 0
    with open(samples_csv, newline="") as f:
        next(f)
        for image_id, _mid, fine, coarse, _conf in csv.reader(f):
            src = thumbs / f"{image_id}.jpg"
            if not src.exists():
                missing += 1
                continue
            digest = sha1(src)
            if digest in seen:
                skipped += 1
                continue
            seen[digest] = image_id
            dst_dir = split_dir / coarse.replace(" ", "_") / fine.replace(" ", "_")
            dst_dir.mkdir(parents=True, exist_ok=True)
            dst = dst_dir / f"{image_id}.jpg"
            if not dst.exists():
                dst.hardlink_to(src)  # same filesystem -> no 70GB copy
            ok += 1
    print(f"{split_dir}: ok={ok} deduped={skipped} missing={missing}")

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--samples", default="ml-corpus/openimages/samples")
    ap.add_argument("--thumbs", default="ml-corpus/openimages/images/thumb")
    ap.add_argument("--out", default="ml-corpus/dataset")
    args = ap.parse_args()
    samples, thumbs, out = pathlib.Path(args.samples), pathlib.Path(args.thumbs), pathlib.Path(args.out)
    for split in ("train", "val"):
        build(samples / f"{split}.csv", thumbs, out / split)

if __name__ == "__main__":
    main()