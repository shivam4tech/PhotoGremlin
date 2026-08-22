#!/usr/bin/env python3
"""Merge all data tiers into the v2 training corpus.

Sources (all resumable upstream):
  oi_human    samples/train_multi.csv   (clean, multi-label, tier D)
  oi_machine  samples/samples_machine.csv (tier A, conf>=0.9)
  openverse   openverse/targets.csv     (tier B, query-tagged)

Output under ml-corpus/corpus_v2/:
  train.csv  path,fine,coarse,confidence,source   (multi-label rows allowed)
  val.csv    clean OI-human val ONLY (unchanged vs earlier rounds -> metrics stay comparable)
  audit.csv  10% of openverse rows held out for region-bias evaluation

Cross-source dedup: sha1 of file bytes; collisions keep the higher-priority
copy (oi_human > oi_machine > openverse). Hashes cached for resume.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import pathlib
from collections import defaultdict

def sha1(path: pathlib.Path, cache: dict, cache_path: pathlib.Path) -> str:
    key = f"{path}|{path.stat().st_size}|{int(path.stat().st_mtime)}"
    if key in cache:
        return cache[key]
    h = hashlib.sha1()
    with open(path, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    digest = h.hexdigest()
    cache[key] = digest
    if len(cache) % 20000 == 0:
        cache_path.write_text(json.dumps(cache))
    return digest

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()
    base = repo / "ml-corpus"
    out = base / "corpus_v2"
    out.mkdir(parents=True, exist_ok=True)

    mapping = json.loads((repo / "tools/train/class-map.json").read_text())
    coarse_of = {k: v["coarse"] for k, v in mapping.items()}

    cache_path = out / ".hashcache.json"
    cache = json.loads(cache_path.read_text()) if cache_path.exists() else {}

    rows_by_source: dict[str, list[tuple]] = defaultdict(list)
    audit_rows: list[tuple] = []
    # ---- oi_human (multi-label rows already grouped per image_id) ----------
    with open(base / "openimages/samples/train_multi.csv", newline="") as f:
        next(f)
        for iid, _mid, fine, _coarse, conf in csv.reader(f):
            p = base / "openimages/images/thumb" / f"{iid}.jpg"
            if p.exists():
                rows_by_source["oi_human"].append(
                    (p.relative_to(base).as_posix(), fine, float(conf or 1.0)))
    # ---- oi_machine --------------------------------------------------------
    mp = base / "openimages/samples/samples_machine.csv"
    if mp.exists():
        with open(mp, newline="") as f:
            next(f)
            for iid, _mid, fine, conf in csv.reader(f):
                p = base / "openimages/images/machine" / f"{iid}.jpg"
                if p.exists():
                    rows_by_source["oi_machine"].append(
                        (p.relative_to(base).as_posix(), fine, float(conf)))
    # ---- openverse ---------------------------------------------------------
    op = base / "openverse/targets.csv"
    ov_dir = base / "openverse/images/openverse"
    if op.exists():
        with open(op, newline="") as f:
            for rec in csv.DictReader(f):
                p = ov_dir / f"{rec['id']}.jpg"
                if not p.exists():
                    continue
                fine = rec["fine"]
                if fine not in coarse_of:
                    continue
                is_audit = int(hashlib.sha1(rec["id"].encode()).hexdigest(), 16) % 10 == 0
                row = (p.relative_to(base).as_posix(), fine, 0.8,
                       rec.get("region_query", ""), is_audit)
                (audit_rows if is_audit else rows_by_source["openverse"]).append(row)

    # ---- commons (tier B+, deep Wikimedia crawl) ---------------------------
    cp = base / "commons/targets.csv"
    cm_dir = base / "commons/images"
    if cp.exists():
        with open(cp, newline="") as f:
            for rec in csv.DictReader(f):
                p = cm_dir / f"{rec['pageid']}.jpg"
                if not p.exists():
                    continue
                fine = rec["fine"]
                if fine not in coarse_of:
                    continue
                is_audit = int(hashlib.sha1(rec["pageid"].encode()).hexdigest(), 16) % 10 == 0
                row = (p.relative_to(base).as_posix(), fine, 0.75,
                       rec.get("region_query", ""), is_audit)
                if is_audit:
                    audit_rows.append(row)
                else:
                    rows_by_source["commons"].append(
                        (row[0], row[1], row[2]))

    # ---- cross-source dedup -------------------------------------------------
    PRIORITY = {"oi_human": 0, "oi_machine": 1, "openverse": 2, "commons": 3}
    by_hash: dict[str, tuple[int, str]] = {}
    kept_rows = []
    dropped = 0
    for source in ("oi_human", "oi_machine", "openverse"):
        for row in rows_by_source[source]:
            p = base / row[0]
            digest = sha1(p, cache, cache_path)
            prev = by_hash.get(digest)
            if prev is None:
                by_hash[digest] = (PRIORITY[source], row[0])
                kept_rows.append((row[0], row[1], row[2], source))
            elif PRIORITY[source] < prev[0]:
                dropped += 1  # a better copy supersedes; drop this row anyway
            else:
                dropped += 1
    cache_path.write_text(json.dumps(cache))
    print(f"kept {len(kept_rows)} unique images ({dropped} cross-source dupes dropped)")

    with open(out / "train.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["path", "fine", "confidence", "source"])
        for r in sorted(kept_rows):
            w.writerow([r[0], r[1], f"{r[2]:.3f}", r[3]])

    # clean subset for stage-2 fine-tuning (human labels only)
    with open(out / "train_clean.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["path", "fine", "confidence", "source"])
        n_clean = 0
        for r in sorted(kept_rows):
            if r[3] == "oi_human":
                w.writerow([r[0], r[1], f"{r[2]:.3f}", r[3]])
                n_clean += 1
    print(f"train.csv={len(kept_rows)} rows; train_clean.csv={n_clean} rows (oi_human)")

    # val: unchanged clean human-only multi-label val (comparable metrics)
    import shutil
    shutil.copyfile(base / "openimages/samples/val_multi.csv", out / "val.csv")

    with open(out / "audit.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["path", "fine", "region_query"])
        for r in sorted(audit_rows):
            w.writerow([r[0], r[1], r[3]])
    print(f"audit rows: {len(audit_rows)}")
    print(f"corpus_v2 written: {out/'train.csv'}, val.csv, audit.csv")

if __name__ == "__main__":
    main()