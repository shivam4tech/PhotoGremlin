#!/usr/bin/env python3
"""Download Open Images thumbnail images for the selected samples.

Reads samples/train.csv + samples/val.csv and the image metadata CSV
(train-images-boxable-with-rotation.csv / val-...) to resolve URLs.
Only images licensed "Attribution License" (CC-BY) or "Public Domain" are
downloaded; attribution is written to PROVENANCE.csv in the samples dir.

Resumable: existing files are skipped. Rerun to continue after a break.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import csv
import pathlib
import sys
import urllib.request

KEEP_LICENSES = {"Attribution License", "Public Domain"}

def load_sample_ids(csv_path: pathlib.Path) -> set[str]:
    with open(csv_path, newline="") as f:
        next(f)
        return {row[0] for row in csv.reader(f)}

def build_manifest(meta: pathlib.Path, samples: pathlib.Path, image_csv: str, ids: set[str]) -> list[tuple[str, str, str, str, str]]:
    """(image_id, url, author, license, title) for CC-BY images in ids."""
    out = []
    with open(meta / image_csv, newline="") as f:
        rows = csv.reader(f)
        header = next(rows)
        ix = {name: i for i, name in enumerate(header)}
        url_ix = ix["Thumbnail300KURL"] if "Thumbnail300KURL" in ix else ix["OriginalURL"]
        for r in rows:
            if r[ix["ImageID"]] not in ids:
                continue
            if r[ix["License"]] not in KEEP_LICENSES:
                continue
            out.append((r[ix["ImageID"]], r[url_ix], r[ix["Author"]], r[ix["License"]], r[ix["Title"]]))
    return out

def fetch_one(item: tuple[str, str, str, str, str], out_dir: pathlib.Path, provenance: pathlib.Path) -> tuple[str, str]:
    image_id, url, author, license_, title = item
    dest = out_dir / f"{image_id}.jpg"
    if dest.exists() and dest.stat().st_size > 1024:
        return image_id, "skip"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "photogremlin-dataset/0.1"})
        with urllib.request.urlopen(req, timeout=60) as r:
            data = r.read()
        if len(data) < 1024:
            return image_id, "empty"
        dest.write_bytes(data)
        with open(provenance, "a", newline="") as f:
            csv.writer(f).writerow([image_id, author, license_, title, url])
        return image_id, "ok"
    except Exception:
        return image_id, "err"

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--meta", default="ml-corpus/openimages/metadata")
    ap.add_argument("--samples", default="ml-corpus/openimages/samples")
    ap.add_argument("--out", default="ml-corpus/openimages/images/thumb")
    ap.add_argument("--split", default="train", choices=["train", "val"])
    ap.add_argument("--workers", type=int, default=10)
    args = ap.parse_args()
    meta, samples, out_dir = pathlib.Path(args.meta), pathlib.Path(args.samples), pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    provenance = samples / f"PROVENANCE_{args.split}.csv"
    if not provenance.exists():
        with open(provenance, "w", newline="") as f:
            csv.writer(f).writerow(["image_id", "author", "license", "title", "url"])

    sample_ids = load_sample_ids(samples / f"{args.split}.csv")
    image_csv = ("train-images-boxable-with-rotation.csv" if args.split == "train"
                 else "val-images-boxable-with-rotation.csv")
    manifest = build_manifest(meta, samples, image_csv, sample_ids)
    print(f"{args.split}: {len(sample_ids)} samples, {len(manifest)} CC-BY resolvable", flush=True)

    stats = {"ok": 0, "skip": 0, "err": 0, "empty": 0}
    done = 0
    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        for _, status in ex.map(lambda it: fetch_one(it, out_dir, provenance), manifest):
            stats[status] += 1
            done += 1
            if done % 5000 == 0:
                print(f"  {done}/{len(manifest)} ok={stats['ok']} skip={stats['skip']} err={stats['err']}", flush=True)
    print(f"{args.split} finished: {stats}", flush=True)

if __name__ == "__main__":
    main()