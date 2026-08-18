# RAW previews (Sprint 15 — done 2026-08-18)

Camera raw previews for the tile grid, the viewer, and contact sheets:
RAW files now decode to real previews locally, wind up in the thumbnail
cache like any other format, and fall back to the existing placeholder tile
when a file can't be decoded.

## Decode provider

- `src-tauri/src/decode.rs` wraps `rawler` (v0.7, the maintained successor
  of `rawloader`): content-sniffed (the format comes from the bytes, not the
  extension), pure Rust, no native libs.
- Formats covered — every extension in `scanner::RAW_EXT` (CR2, CR3, NEF,
  ARW, RAF, DNG, ORF, RW2) plus PEF, 3FR, IIQ, ERF, SRW and others rawler
  knows. CR3, X-Trans RAF and JXL-compressed DNGs decode via rawler's
  jxl-oxide support.
- Develop pipeline: rawler's `RawDevelop::default()` (rescale, demosaic
  [PPG/Malvar], crop active area, white balance, camera→sRGB calibration),
  then EXIF orientation is applied by hand (`image` 0.25 dropped
  `DynamicImage::transpose`, so orientations 5/7 use a manual pixel remap).
- Output is downscaled to the requested `max_width` and flows through the
  normal thumbnail path (JPEG encode, atomic cache write, in-flight dedup).

## Caps & failures

- `RAW_MAX_PIXELS = 30_000_000`: the develop buffer is transient
  full-sensor u16 (≈6 B/px — 30 MP peaks ~180 MB). Larger sensors get a
  friendly "too large to preview" message instead of a swap storm. (The
  JPEG path keeps its own 500 MP guard.)
- Undecodable raws (corrupt, exotic, unsupported camera) return the same
  `UnsupportedFormat` error the UI renders as a placeholder tile — never a
  crash, and never a corrupted cache entry.
- HEIC remains placeholder-only (no preview provider yet).

## Tests

- Unit (`src-tauri/src/decode.rs`): EXIF orientation corner mapping,
  garbage-raw graceful fallback, a **synthetic 16×16 RGGB DNG written by
  hand** decodes to real pixels (no real-file fixtures — AGENTS.md rule 16;
  the writer is `decode::synthetic_dng_bytes`, reused by integration).
- Integration (`tests/raw_preview_integration.rs`): real DB row + real
  file through `ThumbService::get` — DNG produces a cached preview, garbage
  CR2 keeps the placeholder contract and leaves the cache clean.

## Licensing note

`rawler` is **LGPL-2.1**; the app itself is MIT. Local, personal use is
unrestricted, but distributing the PhotoGremlin binary means complying with
the LGPL's static-linking obligations (provide relinkable object files or
the equivalent). If that ever becomes a constraint, the provider interface
(`decode_to_preview`) is the single swap point: rawloader (MIT, no CR3 /
no X-Trans) fits the same signature.