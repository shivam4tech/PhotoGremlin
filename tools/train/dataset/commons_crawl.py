#!/usr/bin/env python3
"""Tier B+: Wikimedia Commons deep crawl.

Commons serves far deeper result sets than the Openverse index and has
genuinely global coverage. Same class x region matrix as openverse_crawl;
license filter is strict (CC0 / Public domain / CC BY only; SA/NC/ND
rejected). Fully resumable: per-query JSONL caches, skip-existing
downloads, incremental provenance. --minutes bounds total wall-clock and
the script exits cleanly when it elapses (re-run to continue).

Usage:  python3 tools/train/dataset/commons_crawl.py --minutes 75
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import csv
import hashlib
import html
import itertools
import json
import pathlib
import re
import sys
import time
import urllib.parse
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from openverse_crawl import queries_for  # noqa: E402  same class x region matrix

API = "https://commons.wikimedia.org/w/api.php"
UA = "photogremlin-dataset/0.2 (contact: shivam4tech@gmail.com)"

BAD_LICENSE = ("by-sa", "by-nc", "by-nd", "nc-sa", "sharealike", "non-free")
GOOD_LICENSE = {"cc by", "cc by 1.0", "cc by 2.0", "cc by 2.5",
                "cc by 3.0", "cc by 4.0", "cc0", "cc0 1.0", "cc-zero"}

def license_ok(short: str) -> bool:
    n = html.unescape(short or "").strip().lower()
    if not n:
        return False
    if any(b in n for b in BAD_LICENSE):
        return False
    if n in GOOD_LICENSE:
        return True
    if "public domain" in n or n.startswith(("pd-", "pd ", "no restrictions")):
        return True
    return False

def commons_get(base_params: dict, deadline: float | None,
                delay: float) -> dict | None:
    """One API request with polite retries; None on budget stop / give-up."""
    params = {"action": "query", "format": "json", "formatversion": "2",
              "maxlag": 5, **base_params}
    for attempt in range(4):
        if deadline and time.time() > deadline:
            return None
        url = API + "?" + urllib.parse.urlencode(params)
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                data = json.loads(r.read())
            if "error" in data:  # maxlag or other soft errors
                wait = 10 * (attempt + 1)
                print(f"    api error '{data['error'].get('code', '')}'; "
                      f"waiting {wait}s", flush=True)
                time.sleep(wait)
                continue
            time.sleep(delay)
            return data
        except Exception as e:
            wait = min(10 * (attempt + 1), 60)
            print(f"    request failed ({e}); retrying in {wait}s", flush=True)
            time.sleep(wait)
    return None

def strip_html(s: str) -> str:
    return html.unescape(re.sub(r"<[^>]+>", "", s or "")).strip()

def crawl_metadata(args, classes: list[str], deadline: float) -> pathlib.Path:
    cache_dir = args.repo / "ml-corpus/commons/cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    targets_path = args.repo / "ml-corpus/commons/targets.csv"
    done_path = cache_dir / "_done.json"
    done = json.loads(done_path.read_text()) if done_path.exists() else {}

    seen: set[str] = set()
    if targets_path.exists():
        with open(targets_path, newline="") as f:
            for rec in csv.DictReader(f):
                seen.add(rec["pageid"])

    pairs = queries_for(classes)
    class_counts: dict[str, int] = {c: 0 for c in classes}
    added_total = 0

    for qi, (fine, query) in enumerate(pairs):
      try:
        qhash = hashlib.sha1(query.encode()).hexdigest()[:16]
        if done.get(qhash):
            continue
        if deadline is not None and time.time() > deadline:
            print("budget reached — metadata phase stops here (resumable)",
                  flush=True)
            break
        rows = []
        cont: dict = {}
        while True:
            params = {"generator": "search",
                      "gsrsearch": f"{query} filetype:bitmap",
                      "gsrnamespace": 6, "gsrlimit": args.page_limit,
                      "prop": "imageinfo",
                      "iiprop": "url|extmetadata|mime|size",
                      "iiurlwidth": args.thumb_width, **cont}
            data = commons_get(params, deadline, args.delay)
            if not data:
                break
            pages = data.get("query", {}).get("pages", [])
            for p in pages:
                pageid = str(p.get("pageid"))
                if pageid in seen:
                    continue
                infos = p.get("imageinfo") or []
                info = infos[0] if infos else {}
                em = info.get("extmetadata", {}) or {}
                lic_raw = (em.get("LicenseShortName", {}) or {}).get("value", "")
                w, h = info.get("width") or 0, info.get("height") or 0
                thumb = info.get("thumburl")
                if info.get("mime") != "image/jpeg" or w < args.min_px \
                        or h < args.min_px or not thumb:
                    continue
                if not license_ok(lic_raw):
                    continue
                seen.add(pageid)
                rows.append({
                    "pageid": pageid, "fine": fine, "region_query": query,
                    "title": p.get("title", ""), "license": lic_raw.strip(),
                    "thumb_url": thumb,
                    "page_url": info.get("descriptionurl", ""),
                    "artist": strip_html((em.get("Artist", {}) or {}).get("value", "")),
                })
                class_counts[fine] += 1
            cont = data.get("continue") or {}
            if not cont or class_counts[fine] >= args.per_class:
                break
            if deadline is not None and time.time() > deadline:
                break
        # crash-safe: persist this query's rows + done marker immediately
        new_flag = not targets_path.exists()
        with open(targets_path, "a", newline="") as tf:
            w = csv.writer(tf)
            if new_flag:
                w.writerow(["pageid", "fine", "region_query", "title",
                            "license", "thumb_url", "page_url", "artist"])
            for r in rows:
                w.writerow([r["pageid"], r["fine"], r["region_query"],
                            r["title"], r["license"], r["thumb_url"],
                            r["page_url"], r["artist"]])
        (cache_dir / f"{qhash}.jsonl").write_text(
            "".join(json.dumps(r) + "\n" for r in rows))
        done[qhash] = len(rows)
        done_path.write_text(json.dumps(done))
        added_total += len(rows)
        print(f"  [{qi+1}/{len(pairs)}] '{query}': {len(rows)} pass "
              f"(class {fine}: {class_counts[fine]}/{args.per_class})",
              flush=True)
      except Exception as e:
        # isolate: log, mark not-done, move to the next query
        print(f"  [{qi+1}/{len(pairs)}] '{query}' FAILED ({e}); continuing",
              flush=True)
        time.sleep(10)
    failed_queries = len(done) and sum(1 for qh, v in list(done.items()))
    print(f"metadata phase complete: {added_total} new candidates "
          f"({len(done)}/{len(pairs)} queries processed)", flush=True)
    return targets_path

def download(args, targets_path: pathlib.Path, deadline: float) -> None:
    out_dir = args.repo / "ml-corpus/commons/images"
    out_dir.mkdir(parents=True, exist_ok=True)
    prov = args.repo / "ml-corpus/commons/samples/PROVENANCE_commons.csv"
    prov.parent.mkdir(parents=True, exist_ok=True)

    with open(targets_path, newline="") as f:
        items = list(csv.DictReader(f))
    stats: dict[str, int] = {}
    stopped_on_budget = False
    print(f"downloading {len(items)} commons targets...", flush=True)

    err_samples: dict[str, int] = {}

    pace = {"delay": max(args.delay, 0.4)}

    def fetch_one(rec) -> str:
        if deadline is not None and time.time() > deadline:
            return "budget"
        dest = out_dir / f"{rec['pageid']}.jpg"
        if dest.exists() and dest.stat().st_size > 1024:
            return "skip"
        url = rec.get("thumb_url") or ""
        if not url:
            return "err"
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
            time.sleep(pace["delay"])
            if len(data) < 1024:
                return "empty"
            dest.write_bytes(data)
            new = not prov.exists()
            with open(prov, "a", newline="") as pf:
                w = csv.writer(pf)
                if new:
                    w.writerow(["pageid", "fine", "title", "license",
                                "artist", "page_url"])
                w.writerow([rec["pageid"], rec["fine"], rec["title"],
                            rec["license"], rec["artist"], rec["page_url"]])
            return "ok"
        except urllib.error.HTTPError as e:
            if e.code == 429:
                retry_after = float(e.headers.get("Retry-After", 30))
                print(f"    429 rate-limited; sleeping {retry_after:.0f}s",
                      flush=True)
                time.sleep(retry_after)
                pace["delay"] = min(pace["delay"] * 2, 8.0)
                err_samples["HTTP 429 throttled"] = \
                    err_samples.get("HTTP 429 throttled", 0) + 1
                return "err"
            key = f"HTTP {e.code}"
            err_samples[key] = err_samples.get(key, 0) + 1
            time.sleep(pace["delay"])
            return "err"
        except Exception as e:
            key = f"{type(e).__name__}: {str(e)[:80]}"
            err_samples[key] = err_samples.get(key, 0) + 1
            return "err"

    pending = iter(items)
    consecutive_errs = 0
    last_err_key = None
    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        while True:
            if deadline is not None and time.time() > deadline:
                stopped_on_budget = True
                break
            chunk = list(itertools.islice(pending, args.workers * 8))
            if not chunk:
                break
            for status in ex.map(fetch_one, chunk):
                stats[status] = stats.get(status, 0) + 1
                if status == "err" and err_samples:
                    top = max(err_samples.items(), key=lambda kv: kv[1])[0]
                    if top == last_err_key:
                        consecutive_errs += 1
                    else:
                        last_err_key, consecutive_errs = top, 1
                    if consecutive_errs >= 25:
                        print(f"    aborting: {consecutive_errs} consecutive "
                              f"failures ({top}) — fix and re-run to resume",
                              flush=True)
                        stopped_on_budget = True   # clean exit, resumable
                        break
                else:
                    consecutive_errs = 0
            if stopped_on_budget:
                break
    label = "stopped on budget" if stopped_on_budget else "finished"
    print(f"commons download {label}: {stats}", flush=True)
    for msg, cnt in sorted(err_samples.items(), key=lambda kv: -kv[1])[:5]:
        print(f"  err[{cnt}]: {msg}", flush=True)
    if stopped_on_budget:
        print("(re-run the same command to continue)", flush=True)

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--minutes", type=float, default=None,
                    help="optional wall-clock budget; OMIT to run until every "
                         "query is crawled and every target downloaded")
    ap.add_argument("--per-class", type=int, default=1200)
    ap.add_argument("--page-limit", type=int, default=200, choices=range(10, 501))
    ap.add_argument("--min-px", type=int, default=300)
    ap.add_argument("--thumb-width", type=int, default=640)
    ap.add_argument("--workers", type=int, default=2)
    ap.add_argument("--delay", type=float, default=0.7,
                    help="seconds between API requests (politeness)")
    ap.add_argument("--download-only", action="store_true")
    ap.add_argument("--metadata-only", action="store_true")
    args = ap.parse_args()
    args.repo = pathlib.Path(args.repo).resolve()

    mapping = json.loads((args.repo / "tools/train/class-map.json").read_text())
    classes = sorted(mapping)
    deadline = time.time() + args.minutes * 60 if args.minutes is not None else None
    targets_path = args.repo / "ml-corpus/commons/targets.csv"
    mode = f"{args.minutes:.0f} min budget" if args.minutes else "RUN TO COMPLETION"
    print(f"commons crawl mode: {mode}", flush=True)

    if args.download_only:
        download(args, targets_path, deadline)
    elif args.metadata_only:
        crawl_metadata(args, classes, deadline)
    else:
        # Bank first: secure whatever earlier runs already targeted before
        # spending budget discovering more (a tight budget must never leave
        # known-good candidates unfetched).
        download(args, targets_path, deadline)
        if deadline is not None and time.time() < deadline:
            crawl_metadata(args, classes, deadline)
            download(args, targets_path, deadline)   # fetch step-2 additions

if __name__ == "__main__":
    main()