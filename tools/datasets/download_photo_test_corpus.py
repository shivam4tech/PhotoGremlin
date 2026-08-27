#!/usr/bin/env python3
"""Build a resumable, development-only photo corpus from Wikimedia Commons.

This script downloads original JPEG files into a sibling directory named
``photogremlin-test-corpus``.  It is intentionally not an application feature:
it makes network requests only when a developer runs it, and the resulting
photos and provenance files must never be committed.

The corpus is organised by camera/genre source and accepts a file only after
checking its downloaded JPEG EXIF for Make, Model, and DateTimeOriginal.

Usage:
    python3 tools/datasets/download_photo_test_corpus.py
    python3 tools/datasets/download_photo_test_corpus.py --output-root \
        /somewhere/photogremlin-test-corpus

Re-running is safe.  Completed files are recorded in manifest.jsonl, partial
downloads use .part files, and state.json stores each category's API cursor.
"""
from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import os
import pathlib
import re
import struct
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any


API = "https://commons.wikimedia.org/w/api.php"
USER_AGENT = (
    "PhotoGremlin-development-corpus/1.1 "
    "(https://github.com/shivam4tech/photogremlin; shivam4tech@gmail.com)"
)
DEFAULT_OUTPUT_ROOT = (
    pathlib.Path(__file__).resolve().parents[3] / "photogremlin-test-corpus"
)


@dataclass(frozen=True)
class Source:
    identifier: str
    folder: str
    category: str
    target: int
    purpose: str


# 1,000 files total.  The individual categories are deliberately large enough
# to tolerate files that fail the original-file EXIF validation below.
SOURCES = (
    Source(
        "nikon_d850_sport",
        "01-nikon-d850-sport",
        "Category:Taken with Nikon D850 for Sport",
        250,
        "fast action and natural near-duplicate sequences",
    ),
    Source(
        "nikon_d850_people",
        "02-nikon-d850-people-and-portrait",
        "Category:Taken with Nikon D850 for People and Portrait",
        100,
        "portraits and subject-sharpness cases",
    ),
    Source(
        "nikon_d850_landscape",
        "03-nikon-d850-landscape",
        "Category:Taken with Nikon D850 for Landscape",
        150,
        "large, detailed landscape and exposure cases",
    ),
    Source(
        "sony_a7iii",
        "04-sony-a7iii",
        "Category:Taken with Sony ILCE-7M3",
        200,
        "full-frame mirrorless camera metadata and mixed scenes",
    ),
    Source(
        "pixel_7_pro",
        "05-google-pixel-7-pro",
        "Category:Taken with Google Pixel 7 Pro",
        150,
        "phone-camera metadata and computational-photo edge cases",
    ),
    Source(
        "canon_5d_mark_iv",
        "06-canon-5d-mark-iv",
        "Category:Taken with Canon EOS 5D Mark IV",
        150,
        "DSLR metadata and mixed photo genres",
    ),
)

PERMANENT_STATUSES = {
    "downloaded",
    "skipped_existing",
    "rejected_exif",
    "rejected_mime",
    "rejected_size",
}
SUCCESS_STATUSES = {"downloaded", "skipped_existing"}


class RateLimited(RuntimeError):
    """A remote rate limit that must stop the current run immediately."""

    def __init__(self, wait_seconds: int) -> None:
        self.wait_seconds = wait_seconds
        super().__init__(f"Wikimedia requested a {wait_seconds}-second cooldown")


def retry_after_seconds(headers: Any) -> int:
    """Return a conservative cooldown from Retry-After, or two minutes."""
    raw_value = headers.get("Retry-After") if headers else None
    if raw_value:
        try:
            return max(5, int(float(raw_value)))
        except ValueError:
            # HTTP-date Retry-After is rare here, but accepting it costs little.
            from email.utils import parsedate_to_datetime

            try:
                target = parsedate_to_datetime(raw_value)
                return max(5, int((target - dt.datetime.now(dt.timezone.utc)).total_seconds()))
            except (TypeError, ValueError):
                pass
    return 120


def jsonl_records(path: pathlib.Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records: list[dict[str, Any]] = []
    with path.open(encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, start=1):
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                print(f"warning: ignoring malformed manifest line {line_no}", file=sys.stderr)
                continue
            if isinstance(record, dict):
                records.append(record)
    return records


def append_jsonl(path: pathlib.Path, record: dict[str, Any]) -> None:
    record = {"recorded_at": dt.datetime.now(dt.timezone.utc).isoformat(), **record}
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def load_state(path: pathlib.Path) -> dict[str, Any]:
    if not path.exists():
        return {"version": 1, "sources": {}}
    try:
        state = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        raise SystemExit(f"State file is invalid JSON: {path}")
    if not isinstance(state, dict) or not isinstance(state.get("sources"), dict):
        raise SystemExit(f"State file has an unexpected shape: {path}")
    return state


def save_state(path: pathlib.Path, state: dict[str, Any]) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def clean_text(value: str) -> str:
    return html.unescape(re.sub(r"<[^>]*>", "", value or "")).strip()


def metadata_value(metadata: dict[str, Any], key: str) -> str:
    value = metadata.get(key, {})
    if isinstance(value, dict):
        value = value.get("value", "")
    return clean_text(str(value))


def commons_request(params: dict[str, Any], delay: float) -> dict[str, Any]:
    query = {
        "action": "query",
        "format": "json",
        "formatversion": "2",
        "maxlag": "5",
        **params,
    }
    url = API + "?" + urllib.parse.urlencode(query)
    last_error: Exception | None = None
    for attempt in range(4):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(request, timeout=90) as response:
                payload = json.loads(response.read())
            if "error" in payload:
                if payload["error"].get("code") == "ratelimited":
                    raise RateLimited(120)
                raise RuntimeError(payload["error"].get("info", "Commons API error"))
            if delay:
                time.sleep(delay)
            return payload
        except urllib.error.HTTPError as error:
            if error.code in (429, 503):
                raise RateLimited(retry_after_seconds(error.headers)) from error
            last_error = error
            if attempt == 3:
                break
            wait = min(5 * (attempt + 1), 30)
            print(f"  API request failed ({error}); retrying in {wait}s", flush=True)
            time.sleep(wait)
        except RateLimited:
            raise
        except Exception as error:  # network failures are resumable
            last_error = error
            if attempt == 3:
                break
            wait = min(5 * (attempt + 1), 30)
            print(f"  API request failed ({error}); retrying in {wait}s", flush=True)
            time.sleep(wait)
    raise RuntimeError(f"Commons API request failed after retries: {last_error}")


def exif_ascii(tiff: bytes, byte_order: str, value_type: int, count: int,
               value_offset: int, entry: bytes) -> str | None:
    if value_type != 2 or count < 1:  # ASCII only
        return None
    if count <= 4:
        raw = entry[8:12][:count]
    elif value_offset + count <= len(tiff):
        raw = tiff[value_offset:value_offset + count]
    else:
        return None
    return raw.rstrip(b"\x00").decode("utf-8", errors="replace").strip() or None


def parse_jpeg_exif(path: pathlib.Path) -> dict[str, str]:
    """Read the three EXIF values required by this corpus without packages."""
    data = path.read_bytes()
    if not data.startswith(b"\xff\xd8"):
        return {}
    index = 2
    tiff = None
    while index + 4 <= len(data):
        if data[index] != 0xFF:
            break
        marker = data[index + 1]
        index += 2
        if marker in (0xD8, 0xD9) or 0xD0 <= marker <= 0xD7:
            continue
        if index + 2 > len(data):
            break
        segment_length = int.from_bytes(data[index:index + 2], "big")
        if segment_length < 2 or index + segment_length > len(data):
            break
        segment = data[index + 2:index + segment_length]
        if marker == 0xE1 and segment.startswith(b"Exif\x00\x00"):
            tiff = segment[6:]
            break
        index += segment_length
    if not tiff or len(tiff) < 8:
        return {}
    if tiff[:2] == b"II":
        byte_order, endian = "little", "<"
    elif tiff[:2] == b"MM":
        byte_order, endian = "big", ">"
    else:
        return {}
    if struct.unpack(endian + "H", tiff[2:4])[0] != 42:
        return {}

    def read_ifd(offset: int) -> list[tuple[int, int, int, int, bytes]]:
        if offset + 2 > len(tiff):
            return []
        count = struct.unpack(endian + "H", tiff[offset:offset + 2])[0]
        entries: list[tuple[int, int, int, int, bytes]] = []
        for entry_offset in range(offset + 2, offset + 2 + count * 12, 12):
            if entry_offset + 12 > len(tiff):
                break
            entry = tiff[entry_offset:entry_offset + 12]
            tag, value_type = struct.unpack(endian + "HH", entry[:4])
            item_count, value_offset = struct.unpack(endian + "II", entry[4:12])
            entries.append((tag, value_type, item_count, value_offset, entry))
        return entries

    first_ifd_offset = struct.unpack(endian + "I", tiff[4:8])[0]
    first_ifd = read_ifd(first_ifd_offset)
    result: dict[str, str] = {}
    exif_ifd_offset = 0
    for tag, value_type, count, value_offset, entry in first_ifd:
        if tag == 0x010F:
            value = exif_ascii(tiff, byte_order, value_type, count, value_offset, entry)
            if value:
                result["make"] = value
        elif tag == 0x0110:
            value = exif_ascii(tiff, byte_order, value_type, count, value_offset, entry)
            if value:
                result["model"] = value
        elif tag == 0x8769:
            exif_ifd_offset = value_offset
    for tag, value_type, count, value_offset, entry in read_ifd(exif_ifd_offset):
        if tag == 0x9003:
            value = exif_ascii(tiff, byte_order, value_type, count, value_offset, entry)
            if value:
                result["datetime_original"] = value
    return result


def extension_for(url: str) -> str:
    suffix = pathlib.PurePosixPath(urllib.parse.urlparse(url).path).suffix.lower()
    return suffix if suffix in {".jpg", ".jpeg"} else ".jpg"


def download_original(url: str, destination: pathlib.Path, max_bytes: int) -> None:
    partial = destination.with_name(destination.name + ".part")
    prior_bytes = partial.stat().st_size if partial.exists() else 0
    headers = {"User-Agent": USER_AGENT}
    if prior_bytes:
        headers["Range"] = f"bytes={prior_bytes}-"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            status = getattr(response, "status", response.getcode())
            if prior_bytes and status != 206:
                prior_bytes = 0
                partial.unlink(missing_ok=True)
            content_length = response.headers.get("Content-Length")
            if content_length and prior_bytes + int(content_length) > max_bytes:
                raise ValueError("file exceeds configured size limit")
            mode = "ab" if prior_bytes else "wb"
            written = prior_bytes
            with partial.open(mode) as handle:
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    written += len(chunk)
                    if written > max_bytes:
                        raise ValueError("file exceeds configured size limit")
                    handle.write(chunk)
                handle.flush()
                os.fsync(handle.fileno())
    except urllib.error.HTTPError as error:
        if error.code in (429, 503):
            raise RateLimited(retry_after_seconds(error.headers)) from error
        raise
    if partial.stat().st_size < 1024:
        raise ValueError("downloaded file is unexpectedly small")
    partial.replace(destination)


def write_readme(root: pathlib.Path) -> None:
    readme = root / "README.md"
    if readme.exists():
        return
    source_lines = "\n".join(
        f"- `{source.folder}/`: {source.target} originals from {source.category} "
        f"({source.purpose})."
        for source in SOURCES
    )
    readme.write_text(
        "# PhotoGremlin development photo corpus\n\n"
        "Generated by `photogremlin/tools/datasets/download_photo_test_corpus.py`. "
        "This directory is development data, not application data. Never commit or "
        "redistribute it. Each accepted file has embedded Make, Model, and "
        "DateTimeOriginal EXIF; `manifest.jsonl` records its Commons source page and "
        "license information.\n\n"
        "## Sources\n\n" + source_lines + "\n",
        encoding="utf-8",
    )


def source_progress(records: list[dict[str, Any]], source: Source) -> tuple[set[str], int]:
    processed: set[str] = set()
    completed: set[str] = set()
    for record in records:
        if record.get("source") != source.identifier:
            continue
        page_id = str(record.get("page_id", ""))
        if not page_id:
            continue
        status = record.get("status")
        if status in PERMANENT_STATUSES:
            processed.add(page_id)
        if status in SUCCESS_STATUSES:
            completed.add(page_id)
    return processed, len(completed)


def legacy_rate_limit_not_before(records: list[dict[str, Any]], source: Source) -> float:
    """Turn 429s recorded by v1.0 into a one-time conservative cooldown."""
    latest = 0.0
    for record in records:
        if record.get("source") != source.identifier or record.get("status") != "failed":
            continue
        if "429" not in str(record.get("error", "")):
            continue
        try:
            recorded_at = dt.datetime.fromisoformat(str(record["recorded_at"]))
            latest = max(latest, recorded_at.timestamp())
        except (KeyError, TypeError, ValueError):
            continue
    # v1.0 did not preserve Retry-After. Five minutes prevents an immediate
    # repeat while still allowing the new script to resume automatically.
    return latest + 300 if latest else 0.0


def record_from_page(source: Source, page: dict[str, Any], info: dict[str, Any]) -> dict[str, Any]:
    extmetadata = info.get("extmetadata") or {}
    return {
        "source": source.identifier,
        "source_category": source.category,
        "page_id": str(page.get("pageid", "")),
        "title": page.get("title", ""),
        "file_page_url": info.get("descriptionurl", ""),
        "original_url": info.get("url", ""),
        "license": metadata_value(extmetadata, "LicenseShortName"),
        "artist": metadata_value(extmetadata, "Artist"),
        "width": info.get("width"),
        "height": info.get("height"),
        "bytes": info.get("size"),
    }


def process_source(args: argparse.Namespace, source: Source, root: pathlib.Path,
                   state: dict[str, Any], manifest: pathlib.Path) -> None:
    records = jsonl_records(manifest)
    processed, completed = source_progress(records, source)
    if completed >= source.target:
        print(f"{source.identifier}: complete ({completed}/{source.target})")
        return

    folder = root / source.folder
    folder.mkdir(parents=True, exist_ok=True)
    source_state = state["sources"].setdefault(source.identifier, {})
    source_state["not_before_unix"] = max(
        float(source_state.get("not_before_unix") or 0),
        legacy_rate_limit_not_before(records, source),
    )
    not_before = float(source_state.get("not_before_unix") or 0)
    remaining = int(not_before - time.time())
    if remaining > 0:
        resume_at = dt.datetime.fromtimestamp(not_before).astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
        print(f"{source.identifier}: rate-limit cooldown active; retry after {resume_at} ({remaining}s)")
        return
    source_state.pop("not_before_unix", None)
    continuation = source_state.get("continuation") or {}
    if source_state.get("exhausted"):
        print(f"{source.identifier}: category exhausted ({completed}/{source.target})")
        return

    print(f"{source.identifier}: {completed}/{source.target}", flush=True)
    while completed < source.target:
        try:
            payload = commons_request({
                "generator": "categorymembers",
                "gcmtitle": source.category,
                "gcmnamespace": "6",
                "gcmtype": "file",
                "gcmlimit": "50",
                "prop": "imageinfo",
                "iiprop": "url|size|mime|extmetadata",
                **continuation,
            }, args.delay)
        except RateLimited as limit:
            source_state["not_before_unix"] = time.time() + limit.wait_seconds
            save_state(root / "state.json", state)
            print(f"  Wikimedia rate-limited the metadata request. Stopping; retry after {limit.wait_seconds}s.")
            return
        pages = payload.get("query", {}).get("pages", [])
        if not pages:
            source_state["exhausted"] = True
            save_state(root / "state.json", state)
            print(f"  category exhausted at {completed}/{source.target}")
            return

        page_has_transient_failure = False
        for page in pages:
            page_id = str(page.get("pageid", ""))
            if not page_id or page_id in processed:
                continue
            infos = page.get("imageinfo") or []
            info = infos[0] if infos else {}
            base = record_from_page(source, page, info)
            if info.get("mime") != "image/jpeg":
                append_jsonl(manifest, {**base, "status": "rejected_mime"})
                processed.add(page_id)
                continue
            if not info.get("url"):
                append_jsonl(manifest, {**base, "status": "failed", "error": "missing original URL"})
                continue
            if int(info.get("size") or 0) > args.max_file_mib * 1024 * 1024:
                append_jsonl(manifest, {**base, "status": "rejected_size"})
                processed.add(page_id)
                continue

            destination = folder / f"{page_id}{extension_for(info['url'])}"
            try:
                if destination.exists() and destination.stat().st_size >= 1024:
                    exif = parse_jpeg_exif(destination)
                    status = "skipped_existing" if {
                        "make", "model", "datetime_original"
                    }.issubset(exif) else "rejected_exif"
                    if status == "rejected_exif":
                        destination.unlink(missing_ok=True)
                else:
                    download_original(info["url"], destination, args.max_file_mib * 1024 * 1024)
                    exif = parse_jpeg_exif(destination)
                    status = "downloaded"
                    if not {"make", "model", "datetime_original"}.issubset(exif):
                        status = "rejected_exif"
                        destination.unlink(missing_ok=True)
                append_jsonl(manifest, {**base, "status": status, "exif": exif})
                processed.add(page_id)
                if status in SUCCESS_STATUSES:
                    completed += 1
                    print(f"  accepted {completed}/{source.target}: {page_id}", flush=True)
                    if completed >= source.target:
                        break
            except RateLimited as limit:
                append_jsonl(manifest, {
                    **base,
                    "status": "rate_limited",
                    "retry_after_seconds": limit.wait_seconds,
                })
                source_state["not_before_unix"] = time.time() + limit.wait_seconds
                save_state(root / "state.json", state)
                print(f"  Wikimedia rate-limited downloads. Stopping; retry after {limit.wait_seconds}s.")
                return
            except Exception as error:
                append_jsonl(manifest, {**base, "status": "failed", "error": str(error)})
                print(f"  failed {page_id}: {error}", file=sys.stderr, flush=True)
                page_has_transient_failure = True

            if args.delay:
                time.sleep(args.delay)

        if page_has_transient_failure:
            # Keep the old cursor: completed and permanently rejected files
            # will be skipped next time, while just the failed originals retry.
            save_state(root / "state.json", state)
            print("  transient failures recorded; re-run to retry this page")
            return
        continuation = payload.get("continue") or {}
        source_state["continuation"] = continuation
        source_state["completed"] = completed
        if not continuation:
            source_state["exhausted"] = True
        save_state(root / "state.json", state)
        if source_state.get("exhausted"):
            print(f"  category exhausted at {completed}/{source.target}")
            return


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-root", type=pathlib.Path, default=DEFAULT_OUTPUT_ROOT,
        help=f"Corpus directory (default: {DEFAULT_OUTPUT_ROOT})",
    )
    parser.add_argument(
        "--max-file-mib", type=int, default=30,
        help="Skip originals larger than this limit (default: 30)",
    )
    parser.add_argument(
        "--delay", type=float, default=5.0,
        help="Minimum seconds to wait after every serial API/download request (default: 5)",
    )
    parser.add_argument(
        "--list-sources", action="store_true",
        help="Print the fixed source plan and exit without creating files",
    )
    args = parser.parse_args()
    if args.max_file_mib < 1:
        parser.error("--max-file-mib must be at least 1")
    if args.delay < 2:
        parser.error("--delay must be at least 2 seconds to respect Wikimedia rate limits")
    return args


def main() -> None:
    args = parse_args()
    if args.list_sources:
        for source in SOURCES:
            print(f"{source.target:>3}  {source.folder:<40}  {source.category}")
        print(f"total: {sum(source.target for source in SOURCES)}")
        return

    root = args.output_root.expanduser().resolve()
    root.mkdir(parents=True, exist_ok=True)
    write_readme(root)
    manifest = root / "manifest.jsonl"
    state_path = root / "state.json"
    state = load_state(state_path)
    print(f"PhotoGremlin development corpus: {root}")
    for source in SOURCES:
        process_source(args, source, root, state, manifest)
    print("Finished. Re-run the same command to continue any incomplete category.")


if __name__ == "__main__":
    main()
