#!/usr/bin/env python3
"""Select per-class training/validation samples from Open Images v7
human-verified image-level labels (Confidence >= 0.7, train cap per class).

Single-label: an image labeled with several mapped classes goes to the class
with the highest confidence. Writes:
  ml-corpus/openimages/samples/train.csv (ImageID,mid,fine_label,coarse,conf)
  ml-corpus/openimages/samples/val.csv   (same)
  ml-corpus/openimages/samples/stats.txt
"""
from __future__ import annotations

import argparse
import csv
import json
import pathlib
from collections import defaultdict

def select(labels_csv: pathlib.Path, mapping: dict, cap: int) -> tuple[dict[str, list[tuple[str, float]]], dict[str, int]]:
    mids = {v["mid"] for v in mapping.values()}
    by_mid: dict[str, list[tuple[str, float]]] = defaultdict(list)
    with open(labels_csv, newline="") as f:
        for image_id, _source, label_name, confidence in csv.reader(f):
            if label_name not in mids:
                continue
            conf = float(confidence)
            if conf < 0.7:
                continue
            by_mid[label_name].append((image_id, conf))
    counts: dict[str, int] = {}
    chosen: dict[str, list[tuple[str, float]]] = {}
    for mid, rows in by_mid.items():
        rows.sort(key=lambda r: -r[1])
        rows = rows[:cap]
        counts[mid] = len(rows)
        chosen[mid] = rows
    return chosen, counts

def write(out: pathlib.Path, chosen: dict, mapping: dict, keep_all: bool = False) -> None:
    # Single-label mode (default): ImageID -> (mid, conf), highest confidence
    # wins. keep_all mode: one row PER verified label — used for multi-label
    # training where an image legitimately carries several of our classes.
    label_of = {v["mid"]: (label, v["coarse"]) for label, v in mapping.items()}
    with open(out, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["image_id", "mid", "fine", "coarse", "confidence"])
        if keep_all:
            rows = []
            for mid, lst in chosen.items():
                fine, coarse = label_of[mid]
                for image_id, conf in lst:
                    rows.append((image_id, mid, fine, coarse, f"{conf:.3f}"))
            for row in sorted(rows):
                w.writerow(row)
            return
        best: dict[str, tuple[str, float]] = {}
        for mid, lst in chosen.items():
            for image_id, conf in lst:
                cur = best.get(image_id)
                if cur is None or conf > cur[1]:
                    best[image_id] = (mid, conf)
        for image_id, (mid, conf) in sorted(best.items()):
            fine, coarse = label_of[mid]
            w.writerow([image_id, mid, fine, coarse, f"{conf:.3f}"])

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--meta", default="ml-corpus/openimages/metadata")
    ap.add_argument("--samples", default="ml-corpus/openimages/samples")
    ap.add_argument("--map", default="tools/train/class-map.json")
    ap.add_argument("--train-cap", type=int, default=8000)
    ap.add_argument("--val-cap", type=int, default=300)
    ap.add_argument("--keep-all-labels", action="store_true",
                    help="write one row per verified label (multi-label training)")
    args = ap.parse_args()
    meta, samples = pathlib.Path(args.meta), pathlib.Path(args.samples)
    samples.mkdir(parents=True, exist_ok=True)
    mapping = json.loads(pathlib.Path(args.map).read_text())

    suffix = "_multi" if args.keep_all_labels else ""
    train, tc = select(meta / "oidv7-train-annotations-human-imagelabels.csv", mapping, args.train_cap)
    val, vc = select(meta / "oidv7-val-annotations-human-imagelabels.csv", mapping, args.val_cap)
    write(samples / f"train{suffix}.csv", train, mapping, args.keep_all_labels)
    write(samples / f"val{suffix}.csv", val, mapping, args.keep_all_labels)

    with open(samples / "stats.txt", "w") as f:
        f.write(f"mapped fine labels: {len(mapping)}  distinct mids: {len(set(v['mid'] for v in mapping.values()))}\n")
        for name, chosen, counts in (("train", train, tc), ("val", val, vc)):
            n = sum(len(r) for r in chosen.values())
            below = {m: c for m, c in counts.items() if c < 100}
            f.write(f"{name}: {n} images across {len(chosen)} mids; classes <100 imgs: {len(below)} {below}\n")
            sparse = sorted(counts.items(), key=lambda kv: kv[1])[:8]
            f.write(f"  smallest: {sparse}\n")

    print("done — see", samples / "stats.txt")

if __name__ == "__main__":
    main()