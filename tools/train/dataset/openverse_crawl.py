#!/usr/bin/env python3
"""Tier B: demographically-targeted crawl via the Openverse API.

Builds a class x region query matrix over our 130 canonical classes:
  queries(class) = [class name] + [f"{region} {class name}"] for a rotating
  sample of ~8 regions per class — so coverage of "weddings/churches/
  markets everywhere" is spread across all classes, not just a few.
Filters: license=by,cc0,pdm AND license_type=commercial (attribution-only
or public domain — safe for a proprietary product).

Two resumable phases:
  1. metadata crawl -> ml-corpus/openverse/cache/<query-hash>.jsonl
     (one cache file per query; re-running skips cached queries)
  2. download      -> ml-corpus/openverse/images/openverse/<id>.jpg
     (skips files already on disk)

Set OPENVERSE_TOKEN for registered rate limits (strongly recommended);
without it the script runs anonymous and polite (slow).

Usage:  python3 tools/train/dataset/openverse_crawl.py [--per-class 600]
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import csv
import hashlib
import json
import os
import pathlib
import sys
import time
import urllib.parse
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

API = "https://api.openverse.org/v1/images/"
AUTH_TOKEN_URL = "https://api.openverse.org/v1/auth_tokens/token/"
UA = "photogremlin-dataset/0.2"

class TokenManager:
    """Openverse OAuth2 client-credentials flow.

    Access tokens expire (~12h); this fetches one lazily and re-fetches on
    demand, caching to disk so re-runs reuse a still-valid token.
    Falls back to a static OPENVERSE_TOKEN if credentials are not provided.
    """

    def __init__(self, client_id: str | None, client_secret: str | None,
                 static_token: str | None, cache_path: pathlib.Path):
        self.client_id = client_id
        self.client_secret = client_secret
        self.static = static_token
        self.cache_path = cache_path
        self._access: str | None = None

    def bearer(self) -> str | None:
        if self.static:
            return self.static
        if self._access:
            return self._access
        cache = {}
        if self.cache_path.exists():
            try:
                cache = json.loads(self.cache_path.read_text())
                if cache.get("expires_at", 0) - 600 > time.time():
                    self._access = cache["access_token"]
                    return self._access
            except Exception:
                pass
        if not (self.client_id and self.client_secret):
            return None
        data = urllib.parse.urlencode({
            "client_id": self.client_id, "client_secret": self.client_secret,
            "grant_type": "client_credentials",
        }).encode()
        req = urllib.request.Request(AUTH_TOKEN_URL, data=data, headers={"User-Agent": UA})
        payload = None
        for t_attempt in range(3):
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    payload = json.loads(r.read())
                break
            except Exception as e:
                if t_attempt == 2:
                    raise RuntimeError(f"could not fetch Openverse token: {e}") from e
                print(f"    [auth] token fetch failed ({e}); retrying", flush=True)
                time.sleep(5 * (t_attempt + 1))
        self._access = payload["access_token"]
        expires_in = int(payload.get("expires_in", 43200))
        self.cache_path.write_text(json.dumps({
            "access_token": self._access,
            "expires_at": time.time() + expires_in,
        }))
        return self._access

    def invalidate(self) -> None:
        self._access = None
        if self.cache_path.exists():
            self.cache_path.unlink()

def api_get(url: str, auth: TokenManager | None, timeout: int = 60) -> dict:
    """GET with full retry coverage: bearer()/token-endpoint hiccups, 401/403
    (invalidate + refetch token), and server errors (backoff)."""
    last_err = None
    for attempt in range(6):
        try:
            headers = {"User-Agent": UA}
            if auth is not None:
                bearer = auth.bearer()   # network call — kept inside the try
                if bearer:
                    headers["Authorization"] = f"Bearer {bearer}"
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            last_err = e
            if e.code in (401, 403) and auth is not None:
                print("    [auth] 401/403 — dropping cached token, will refetch", flush=True)
                auth.invalidate()
                time.sleep(5)
                continue
            if e.code in (429, 500, 502, 503, 504):
                wait = min(60 * (attempt + 1), 300)
                print(f"    {e.code}; backing off {wait}s", flush=True)
                time.sleep(wait)
            else:
                raise
        except Exception as e:  # transient network or token-endpoint error
            last_err = e
            print(f"    transient error ({last_err}); retrying", flush=True)
            time.sleep(min(5 * (attempt + 1), 60))
    raise RuntimeError(f"openverse query failed after retries: {last_err}")
REGIONS = [
    "indian", "nigerian", "ethiopian", "kenyan", "egyptian", "moroccan",
    "chinese", "japanese", "korean", "vietnamese", "thai", "indonesian",
    "filipino", "turkish", "iranian", "arab", "israeli", "russian", "polish",
    "greek", "spanish", "portuguese", "italian", "mexican", "brazilian",
    "peruvian", "colombian", "argentinian", "cuban", "nepalese",
]
REGIONS_PER_CLASS = 8

def queries_for(classes: list[str]) -> list[tuple[str, str]]:
    """(fine_class, query) pairs covering every class + regional variants."""
    out = []
    for idx, cls in enumerate(sorted(classes)):
        base = cls.replace(" ", " ")
        out.append((cls, base))
        step = max(1, len(REGIONS) // REGIONS_PER_CLASS)
        picked = [REGIONS[(idx * 7 + k * step) % len(REGIONS)] for k in range(REGIONS_PER_CLASS)]
        for region in picked:
            out.append((cls, f"{region} {base}"))
    return out

def crawl_metadata(args, classes: list[str], auth) -> pathlib.Path:
    cache_dir = args.repo / "ml-corpus/openverse/cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    targets_path = args.repo / "ml-corpus/openverse/targets.csv"
    done_marker = cache_dir / "_crawl_done.json"
    done = {}
    if done_marker.exists():
        done = json.loads(done_marker.read_text())

    pairs = queries_for(classes)
    total = len(pairs)
    seen_ids: set[str] = set()
    if targets_path.exists():
        with open(targets_path, newline="") as f:
            next(f)
            seen_ids = {row[0] for row in csv.reader(f)}

    rows_new = []
    for qi, (fine, query) in enumerate(pairs):
        qhash = hashlib.sha1(query.encode()).hexdigest()[:16]
        cache_file = cache_dir / f"{qhash}.jsonl"
        if qhash in done and cache_file.exists():
            continue
        n_saved = 0
        page = 1
        quota = args.per_query
        with open(cache_file, "a") as cf_:
            while quota > 0:
                params = urllib.parse.urlencode({
                    "q": query, "license": "by,cc0,pdm",
                    "license_type": "commercial", "page_size": 50, "page": page,
                })
                try:
                    data = api_get(f"{API}?{params}", auth)
                except RuntimeError as e:
                    print(f"    giving up on '{query}' page {page}: {e}", flush=True)
                    break
                results = data.get("results", [])
                if not results:
                    break
                for r in results:
                    if quota <= 0:
                        break
                    oid = r.get("id")
                    if not oid or oid in seen_ids:
                        continue
                    rec = {
                        "id": oid, "fine": fine, "query": query,
                        "url": r.get("url"), "thumbnail": r.get("thumbnail"),
                        "width": r.get("width"), "height": r.get("height"),
                        "creator": r.get("creator"), "license": (
                            f"CC {r.get('license', '').upper()} "
                            f"{r.get('license_version', '')}").strip(),
                        "source": r.get("source"),
                        "landing": r.get("foreign_landing_url"),
                        "title": r.get("title"),
                    }
                    if not (rec["thumbnail"] or rec["url"]):
                        continue
                    cf_.write(json.dumps(rec) + "\n")
                    seen_ids.add(oid)
                    rows_new.append(rec)
                    quota -= 1
                    n_saved += 1
                page += 1
                time.sleep(args.delay)
        done[qhash] = {"query": query, "results": n_saved}
        done_marker.write_text(json.dumps(done))
        print(f"  [{qi+1}/{total}] '{query}': {n_saved} candidates", flush=True)

    new_flag = not targets_path.exists()
    with open(targets_path, "a", newline="") as f:
        w = csv.writer(f)
        if new_flag:
            w.writerow(["id", "fine", "region_query", "image_url", "thumb_url",
                        "author", "license", "source", "landing", "title"])
        for r in rows_new:
            w.writerow([r["id"], r["fine"], r["query"], r["url"], r["thumbnail"],
                        r["creator"], r["license"], r["source"], r["landing"],
                        r["title"]])
    return targets_path

def download_targets(args, targets_path: pathlib.Path) -> None:
    out_dir = args.repo / "ml-corpus/openverse/images/openverse"
    out_dir.mkdir(parents=True, exist_ok=True)
    provenance = args.repo / "ml-corpus/openverse/samples" / "PROVENANCE_openverse.csv"

    import urllib.request  # local alias to keep top imports tidy
    stats = {"ok": 0, "skip": 0, "err": 0}
    with open(targets_path, newline="") as f:
        reader = csv.DictReader(f)
        items = list(reader)
    print(f"downloading {len(items)} openverse targets...", flush=True)

    def fetch_one(rec) -> str:
        dest = out_dir / f"{rec['id']}.jpg"
        if dest.exists() and dest.stat().st_size > 1024:
            return "skip"
        url = rec["thumb_url"] or rec["image_url"]
        if not url:
            return "err"
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "photogremlin-dataset/0.2"})
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
            if len(data) < 1024:
                return "empty"
            dest.write_bytes(data)
            new = not provenance.exists()
            import csv as _csv
            with open(provenance, "a", newline="") as pf:
                w = _csv.writer(pf)
                if new:
                    w.writerow(["id", "fine", "author", "license", "source", "landing"])
                w.writerow([rec["id"], rec["fine"], rec["author"], rec["license"],
                            rec["source"], rec["landing"]])
            return "ok"
        except Exception:
            return "err"

    with cf.ThreadPoolExecutor(max_workers=args.workers) as ex:
        for i, status in enumerate(ex.map(fetch_one, items)):
            stats[status] = stats.get(status, 0) + 1
            if (i + 1) % 5000 == 0:
                print(f"  {i+1}/{len(items)} {stats}", flush=True)
    print(f"tier B download finished: {stats}", flush=True)

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--per-class", type=int, default=1500,
                    help="candidate budget per fine class (all its queries combined)")
    ap.add_argument("--workers", type=int, default=6)
    ap.add_argument("--delay", type=float, default=1.2, help="seconds between API pages")
    ap.add_argument("--download-only", action="store_true")
    ap.add_argument("--metadata-only", action="store_true")
    args = ap.parse_args()
    args.repo = pathlib.Path(args.repo).resolve()

    mapping = json.loads((args.repo / "tools/train/class-map.json").read_text())
    classes = sorted(mapping)
    # per-query budget keeps total near --per-class (1 base + 8 region queries)
    args.per_query = max(20, args.per_class // 9)

    auth = TokenManager(
        os.environ.get("OPENVERSE_CLIENT_ID"),
        os.environ.get("OPENVERSE_CLIENT_SECRET"),
        os.environ.get("OPENVERSE_TOKEN"),   # legacy static token still honored
        args.repo / "ml-corpus/openverse/.token.json",
    )
    if not (auth.client_id and auth.client_secret) and not auth.static:
        print("WARNING: OPENVERSE_CLIENT_ID/SECRET not set — running ANONYMOUS "
              "(heavily rate-limited; register at api.openverse.org).", flush=True)
    else:
        print(f"auth: {'static token' if auth.static else 'client credentials'}", flush=True)

    if not args.download_only:
        targets = crawl_metadata(args, classes, auth)
        print(f"targets: {targets}")
    if not args.metadata_only:
        download_targets(args, args.repo / "ml-corpus/openverse/targets.csv")

if __name__ == "__main__":
    main()