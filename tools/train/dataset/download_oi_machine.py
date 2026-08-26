#!/usr/bin/env python3
"""Tier A: bulk pre-training data from Open Images v7 MACHINE labels.

Selects images whose machine-generated label for one of our 130 classes has
Confidence >= --conf (default 0.9), caps per class, resolves CC-BY/PD
licenses and downloads 300K thumbnails. Fully resumable: the label CSV
downloads with Range-resume; selection caches to samples_machine.csv;
image download skips files already on disk.

Usage:  python3 tools/train/dataset/download_oi_machine.py [--cap 3000]
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import csv
import json
import pathlib
import sys
from collections import defaultdict

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from oid_common import fetch, fetch_image, resolve_manifest  # noqa: E402

MACHINE_CSV_URL = "https://storage.googleapis.com/openimages/v7/oidv7-train-annotations-machine-imagelabels.csv"

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--cap", type=int, default=3000, help="max machine-labeled images per class")
    ap.add_argument("--conf", type=float, default=0.9)
    ap.add_argument("--workers", type=int, default=10)
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    meta = repo / "ml-corpus/openimages/metadata"
    out_dir = repo / "ml-corpus/openimages/images/machine"
    out_dir.mkdir(parents=True, exist_ok=True)
    provenance = meta.parent / "samples" / "PROVENANCE_machine.csv"
    machine_csv = meta / "oidv7-train-annotations-machine-imagelabels.csv"

    mapping = json.loads((repo / "tools/train/class-map.json").read_text())
    mids = {v["mid"] for v in mapping.values()}
    mid_label = {v["mid"]: k for k, v in mapping.items()}

    # 1. label CSV (7.3GB, Range-resume)
    if not machine_csv.exists() or machine_csv.stat().st_size < 1024:
        print(f"downloading machine labels ({MACHINE_CSV_URL.rsplit('/', 1)[-1]})...", flush=True)
        fetch(MACHINE_CSV_URL, machine_csv, timeout=300)
    print(f"machine labels on disk: {machine_csv.stat().st_size/1e9:.2f} GB", flush=True)

    # 2. select per class (skip image_ids we already downloaded as thumbs)
    thumb_dir = repo / "ml-corpus/openimages/images/thumb"
    have = {p.stem for p in thumb_dir.glob("*.jpg")} if thumb_dir.exists() else set()
    sel_path = repo / "ml-corpus/openimages/samples/samples_machine.csv"
    if sel_path.exists():
        print(f"selection cached: {sel_path}")
    else:
        per_class: dict[str, list[tuple[str, float]]] = defaultdict(list)
        tmp = sel_path.with_suffix(".csv.part")
        with open(machine_csv, newline="") as f:
            for image_id, _source, label_name, confidence in csv.reader(f):
                if label_name not in mids:
                    continue
                conf = float(confidence)
                if conf < args.conf or image_id in have:
                    continue
                per_class[label_name].append((image_id, conf))
        chosen: dict[str, tuple[str, float]] = {}   # image -> best (mid, conf), single primary
        for mid, lst in sorted(per_class.items()):
            lst.sort(key=lambda r: -r[1])
            for image_id, conf in lst[: args.cap]:
                cur = chosen.get(image_id)
                if cur is None or conf > cur[1]:
                    chosen[image_id] = (mid, conf)
        with open(tmp, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["image_id", "mid", "fine", "confidence"])
            for image_id, (mid, conf) in sorted(chosen.items()):
                w.writerow([image_id, mid, mid_label[mid], f"{conf:.3f}"])
        tmp.replace(sel_path)
        print(f"selected {len(chosen)} machine-labeled images", flush=True)

    # 3. resolve licenses + download
    with open(sel_path, newline="") as f:
        next(f)
        ids = {row[0] for row in csv.reader(f)}
    boxable = meta / "train-images-boxable-with-rotation.csv"
    manifest = resolve_manifest(ids, boxable, require_ccby=True)
    print(f"{len(ids)} selected, {len(manifest)} CC-BY resolvable", flush=True)

    stats = {"ok": 0, "skip": 0, "err": 0, "empty": 0}
    done = 0
    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        for _, status in ex.map(lambda it: fetch_image(it, out_dir, provenance), manifest):
            stats[status] += 1
            done += 1
            if done % 10000 == 0:
                print(f"  {done}/{len(manifest)} {stats}", flush=True)
    print(f"tier A finished: {stats}", flush=True)

if __name__ == "__main__":
    main()