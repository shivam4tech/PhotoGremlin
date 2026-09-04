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
- The preview ladder is deliberately cheapest-first: a same-stem JPEG beside
  the RAW, then the RAW's embedded preview, then its embedded thumbnail, and
  only then a bounded full develop. A camera's `IMG_0042.CR3` plus
  `IMG_0042.JPG` therefore never allocates a sensor-sized RGB buffer merely
  to paint a grid tile.
- The full-develop fallback uses rawler's `RawDevelop::default()` (rescale,
  demosaic [PPG/Malvar], crop active area, white balance, camera→sRGB
  calibration), then applies EXIF orientation and immediately downsizes to
  the requested width.
- Every successful route flows through the normal thumbnail path (JPEG
  encode, atomic cache write, in-flight dedup). The React UI sends RAW files
  to this Rust provider; it only suppresses HEIC/HEIF, for which no local
  decoder exists yet.

## Caps & failures

- `RAW_MAX_PIXELS = 24_000_000`: dimensions are checked from raw metadata
  **before** asking rawler to produce full developed pixels. Larger sources
  must use a paired/embedded preview and otherwise get a friendly
  unsupported-preview message. This ordering is the memory-safety boundary:
  the rejected sensor buffer is never allocated. (The ordinary encoded-image
  path keeps its separate 500 MP header guard.)
- Undecodable raws (corrupt, exotic, unsupported camera) return the same
  `UnsupportedFormat` error the UI renders as a placeholder tile — never a
  crash, and never a corrupted cache entry.
- HEIC remains placeholder-only (no preview provider yet).

## Tests

- Unit (`src-tauri/src/decode.rs`): EXIF orientation corner mapping,
  garbage-raw graceful fallback, case-insensitive paired-JPEG preference, and
  a **synthetic 16×16 RGGB DNG written by hand** decodes to real pixels (no
  real-file fixtures — AGENTS.md rule 16; the writer is
  `decode::synthetic_dng_bytes`, reused by integration).
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
