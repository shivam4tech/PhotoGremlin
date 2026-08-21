#!/usr/bin/env python3
"""Download Open Images v7 metadata files into the corpus (gitignored).

Pure stdlib: urllib + csv. Prints a summary of what was fetched.
Full paths are relative to the corpus root (default: ml-corpus/).
"""
from __future__ import annotations

import argparse
import pathlib
import urllib.request

BASE = "https://storage.googleapis.com/openimages"

FILES = {
    "class-descriptions.csv": f"{BASE}/2017_11/class-descriptions.csv",
    "classes-trainable.txt": f"{BASE}/2017_11/classes-trainable.txt",
    "oidv7-train-annotations-human-imagelabels.csv": f"{BASE}/v7/oidv7-train-annotations-human-imagelabels.csv",
    "oidv7-val-annotations-human-imagelabels.csv": f"{BASE}/v7/oidv7-val-annotations-human-imagelabels.csv",
    # Image CSVs live in the v6-era bucket (still current for v7; the /v7/
    # path 403s).
    "train-images-boxable-with-rotation.csv": f"{BASE}/2018_04/train/train-images-boxable-with-rotation.csv",
    "val-images-boxable-with-rotation.csv": f"{BASE}/2018_04/validation/validation-images-with-rotation.csv",
    # Places365 class *names* only (the taxonomy words; images come from
    # Open Images, CC-BY. Names are not copyrightable.)
    "categories_places365.txt": (
        "https://raw.githubusercontent.com/csailvision/places365/master/categories_places365.txt"
    ),
}

def fetch(url: str, dest: pathlib.Path) -> None:
    tmp = dest.with_suffix(dest.suffix + ".part")
    resume_from = tmp.stat().st_size if tmp.exists() else 0
    headers = {"User-Agent": "photogremlin-dataset/0.1"}
    mode = "ab" if resume_from else "wb"
    if resume_from:
        headers["Range"] = f"bytes={resume_from}-"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=300) as r, open(tmp, mode) as f:
        while chunk := r.read(1 << 20):
            f.write(chunk)
    tmp.replace(dest)
    print(f"  ok {dest.name} ({dest.stat().st_size / 1e6:.1f} MB)", flush=True)

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="ml-corpus/openimages/metadata")
    ap.add_argument("--only", nargs="*", help="download only these filenames")
    args = ap.parse_args()
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for name in sorted(FILES):
        if args.only and name not in args.only:
            continue
        dest = out / name
        if dest.exists() and dest.stat().st_size > 1024:
            print(f"  skip {name} (already present)")
            continue
        print(f"  fetch {name}")
        fetch(FILES[name], dest)

if __name__ == "__main__":
    main()