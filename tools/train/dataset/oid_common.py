#!/usr/bin/env python3
"""Shared Open Images download helpers for the dataset tools (stdlib only).

Resumability contract: a file on disk with size > MIN_BYTES is 'done';
downloaders skip it. Interrupting any script and re-running the same
command continues where it stopped.
"""
from __future__ import annotations

import csv
import pathlib
import time
import urllib.request

MIN_BYTES = 1024
UA = {"User-Agent": "photogremlin-dataset/0.2"}

def fetch(url: str, dest: pathlib.Path, timeout: int = 120) -> None:
    tmp = dest.with_suffix(dest.suffix + ".part")
    headers = dict(UA)
    mode = "wb"
    if tmp.exists() and tmp.stat().st_size > 0:
        headers["Range"] = f"bytes={tmp.stat().st_size}-"
        mode = "ab"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=timeout) as r, open(tmp, mode) as f:
        while chunk := r.read(1 << 20):
            f.write(chunk)
    tmp.replace(dest)

def license_ok(license_url: str) -> bool:
    l = license_url.lower()
    return "creativecommons.org/licenses/by/2.0" in l or "publicdomain" in l

def image_meta_index(meta_csv: pathlib.Path) -> tuple[dict[str, int], csv.reader]:
    f = open(meta_csv, newline="")
    rows = csv.reader(f)
    header = next(rows)
    return {name: i for i, name in enumerate(header)}, rows

def resolve_manifest(sample_ids: set[str], meta_csv: pathlib.Path,
                     require_ccby: bool = True) -> list[tuple[str, str, str, str, str]]:
    """(image_id, thumb_url, author, license, title) for sample_ids."""
    ix, rows = image_meta_index(meta_csv)
    url_ix = ix["Thumbnail300KURL"] if "Thumbnail300KURL" in ix else ix["OriginalURL"]
    out = []
    for r in rows:
        if r[ix["ImageID"]] not in sample_ids:
            continue
        if require_ccby and not license_ok(r[ix["License"]]):
            continue
        out.append((r[ix["ImageID"]], r[url_ix], r[ix["Author"]],
                    r[ix["License"]], r[ix["Title"]]))
    return out

def fetch_image(item: tuple[str, str, str, str, str], out_dir: pathlib.Path,
                provenance: pathlib.Path) -> tuple[str, str]:
    image_id, url, author, license_, title = item
    dest = out_dir / f"{image_id}.jpg"
    if dest.exists() and dest.stat().st_size > MIN_BYTES:
        return image_id, "skip"
    try:
        req = urllib.request.Request(url, headers=UA)
        with urllib.request.urlopen(req, timeout=60) as r:
            data = r.read()
        if len(data) < MIN_BYTES:
            return image_id, "empty"
        dest.write_bytes(data)
        new = not provenance.exists()
        with open(provenance, "a", newline="") as f:
            w = csv.writer(f)
            if new:
                w.writerow(["image_id", "author", "license", "title", "url"])
            w.writerow([image_id, author, license_, title, url])
        return image_id, "ok"
    except Exception:
        return image_id, "err"

def wait_retry(seconds: float) -> None:
    time.sleep(seconds)
