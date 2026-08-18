//! Thumbnail engine (Sprint 3).
//!
//! The grid NEVER sees full-resolution pixels: every tile is a small JPEG
//! generated locally and cached on disk. The UI receives base64 data URLs
//! (self-contained; ~20–50 KB per 256 px tile, bounded by the virtualized
//! grid — unmounted tiles free their memory).
//!
//! Cache key = FNV-1a of (path | size_bytes | mtime | max_width | version),
//! so a changed file or a changed encoder version automatically invalidates.
//! Deterministic across restarts (no random seed), which is what makes the
//! on-disk cache useful between runs.
//!
//! Concurrency: cache hits return immediately; generation is capped by a
//! semaphore (a few full-res decodes in memory at once) and deduplicated
//! in-flight, so a fast-scrolling grid cannot spawn a decode storm.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::{ImageReader, RgbImage};
use rusqlite::{params, OptionalExtension};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::scanner::{classify_extension, FileClass};

/// Bump when the encode/resize pipeline changes (invalidates the cache).
pub const THUMB_VERSION: u32 = 1;
/// Maximum concurrent full-image decodes/encodes.
pub const THUMB_GENERATE_CONCURRENCY: usize = 3;
/// Refuse to decode absurdly large images into memory (≈500 MP guard).
const MAX_PIXELS: u64 = 500_000_000;

const GRID_MAX_WIDTH: u32 = 256;
const VIEWER_MAX_WIDTH: u32 = 1600;
const SHEET_MAX_WIDTH: u32 = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbKind {
    Grid,
    Viewer,
    /// Contact-sheet tiles (~800 px — sharp enough for a 575 px print box,
    /// much cheaper than the full image).
    Sheet,
}

impl ThumbKind {
    pub fn max_width(self) -> u32 {
        match self {
            ThumbKind::Grid => GRID_MAX_WIDTH,
            ThumbKind::Viewer => VIEWER_MAX_WIDTH,
            ThumbKind::Sheet => SHEET_MAX_WIDTH,
        }
    }
}

/// Deterministic 64-bit hash — stable across runs (std's DefaultHasher is
/// randomly seeded and must NOT be used for cache keys).
pub fn fnv1a64(data: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

pub fn thumb_cache_name(photo_path: &str, size_bytes: i64, mtime: Option<&str>, max_width: u32) -> String {
    let input = format!(
        "{photo_path}|{size_bytes}|{}|{max_width}|{THUMB_VERSION}",
        mtime.unwrap_or("unknown")
    );
    format!("{:016x}.jpg", fnv1a64(input.as_bytes()))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ThumbData {
    /// `data:image/jpeg;base64,...` for the UI to use directly as an img src.
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub from_cache: bool,
}

#[derive(Debug)]
enum Outcome {
    Ready(Vec<u8>, u32, u32),
    Fail(AppError),
}

fn outcome_to_data(outcome: Outcome, from_cache: bool) -> AppResult<ThumbData> {
    match outcome {
        Outcome::Ready(bytes, w, h) => Ok(ThumbData {
            data_url: b64_data_url(&bytes),
            width: w,
            height: h,
            from_cache,
        }),
        Outcome::Fail(e) => Err(e),
    }
}

/// Long-lived thumbnail service: cache dir + generation throttle +
/// in-flight dedup (presence markers; waiters poll the cache file with a
/// bounded deadline). One instance per app, held in managed state.
pub struct ThumbService {
    cache_dir: PathBuf,
    sem: tokio::sync::Semaphore,
    in_flight: std::sync::Mutex<HashMap<String, ()>>,
}

impl ThumbService {
    pub fn new(cache_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);
        Self {
            cache_dir,
            sem: tokio::sync::Semaphore::new(THUMB_GENERATE_CONCURRENCY),
            in_flight: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Generate (or read from cache) a thumbnail for a photo row.
    pub async fn get(&self, db: &Db, photo_id: i64, kind: ThumbKind) -> AppResult<ThumbData> {
        // Photo row: path, extension, size, mtime.
        let (path, extension, size_bytes, mtime) = {
            let conn = db.lock()?;
            let row: Option<(String, String, Option<i64>, Option<String>)> = conn
                .query_row(
                    "SELECT path, extension, size_bytes, file_mtime
                     FROM photos WHERE id = ?1",
                    params![photo_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(|e| {
                    tracing::error!(error = %e, photo_id, "photo row lookup failed");
                    AppError::Database(e.to_string())
                })?;
            row.ok_or_else(|| AppError::operation("This photograph is no longer in the library"))?
        };

        // Formats without a local pixel provider in v0.1 (HEIC): the UI
        // shows a labelled placeholder tile — never a crash. RAW files get
        // the decode provider (Sprint 15): decodable files preview
        // normally, undecodable ones fall back to the same placeholder.
        if matches!(classify_extension(&extension), FileClass::Heic) {
            return Err(AppError::UnsupportedFormat { path });
        }

        let p = PathBuf::from(&path);
        if !p.exists() {
            return Err(AppError::FileMissing {
                target: path.clone(),
                reason: "the file is not present on disk".into(),
            });
        }

        let cache_file = self.cache_dir.join(thumb_cache_name(
            &path,
            size_bytes.unwrap_or(0),
            mtime.as_deref(),
            kind.max_width(),
        ));
        let key = cache_file
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        if cache_file.exists() {
            let bytes = std::fs::read(&cache_file)
                .map_err(|e| AppError::io(e, cache_file.display().to_string()))?;
            let (w, h) = image_dimensions_of(&bytes)
                .ok_or_else(|| AppError::operation("Cached thumbnail is corrupt"))?;
            tracing::debug!(photo_id, %path, "thumbnail cache hit");
            return Ok(ThumbData {
                data_url: b64_data_url(&bytes),
                width: w,
                height: h,
                from_cache: true,
            });
        }

        // In-flight dedup: another task may already be generating this key.
        // Waiting is an optimization, not correctness — with a bounded
        // deadline we fall back to generating our own if the generator is
        // slow or vanished.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            // Block scope: the std MutexGuard must not cross the await below
            // (futures must be Send for Tauri's command runtime).
            let generating = {
                let guard = self.in_flight.lock().expect("in-flight map poisoned");
                guard.contains_key(&key)
            };
            if !generating {
                break;
            }
            tracing::debug!(%key, "joining in-flight thumbnail generation");
            if std::time::Instant::now() >= deadline {
                break; // generator stalled: proceed with our own generation
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
        if cache_file.exists() {
            let bytes = std::fs::read(&cache_file)
                .map_err(|e| AppError::io(e, cache_file.display().to_string()))?;
            let (w, h) = image_dimensions_of(&bytes)
                .ok_or_else(|| AppError::operation("Cached thumbnail is corrupt"))?;
            return Ok(ThumbData {
                data_url: b64_data_url(&bytes),
                width: w,
                height: h,
                from_cache: true,
            });
        }

        // We are the generator for this key.
        self.in_flight
            .lock()
            .expect("in-flight map poisoned")
            .insert(key.clone(), ());
        let outcome = self
            .generate(&p, &cache_file, kind.max_width(), classify_extension(&extension))
            .await;
        self.in_flight
            .lock()
            .expect("in-flight map poisoned")
            .remove(&key);
        outcome_to_data(outcome, false)
    }

    /// Decode + downscale + encode + write cache. CPU work runs on a
    /// blocking thread under the generation semaphore.
    async fn generate(
        &self,
        path: &Path,
        cache_file: &Path,
        max_width: u32,
        class: FileClass,
    ) -> Outcome {
        let Ok(_permit) = self.sem.acquire().await else {
            return Outcome::Fail(AppError::operation("Thumbnail generator shut down"));
        };

        let path_owned = path.to_path_buf();
        let cache_owned = cache_file.to_path_buf();
        let result = tokio::task::spawn_blocking(move || {
            // RAW files go through the content-sniffing decode provider
            // (Sprint 15) — rawler discovers the format from the bytes, so
            // neither the header guard nor ImageReader applies.
            let (tw, th, rgb): (u32, u32, RgbImage) = if class == FileClass::Raw {
                match crate::decode::decode_to_preview(&path_owned, max_width) {
                    Ok(Some(img)) => (img.width(), img.height(), img),
                    Ok(None) => {
                        return Ok(Outcome::Fail(AppError::UnsupportedFormat {
                            path: path_owned.display().to_string(),
                        }))
                    }
                    Err(e) => return Ok(Outcome::Fail(e)),
                }
            } else {
                // Header-only check: reject absurdly huge images before
                // paying for a full decode.
                let (w, h) = image::image_dimensions(&path_owned).map_err(|e| {
                    AppError::ImageRead {
                        path: path_owned.display().to_string(),
                        reason: e.to_string(),
                    }
                })?;
                if (w as u64) * (h as u64) > MAX_PIXELS {
                    return Ok(Outcome::Fail(AppError::operation(
                        "Image is too large to preview safely",
                    )));
                }
                let scale = f64::from(max_width) / f64::from(w.max(1));
                let tw = if scale < 1.0 {
                    u32::max(1, (f64::from(w) * scale) as u32)
                } else {
                    w
                };
                let th = if scale < 1.0 {
                    u32::max(1, (f64::from(h) * scale) as u32)
                } else {
                    h
                };

                let img = ImageReader::open(&path_owned)
                    .map_err(|e| AppError::ImageRead {
                        path: path_owned.display().to_string(),
                        reason: e.to_string(),
                    })?
                    .with_guessed_format()
                    .map_err(|e| AppError::ImageRead {
                        path: path_owned.display().to_string(),
                        reason: e.to_string(),
                    })?
                    .decode()
                    .map_err(|e| AppError::ImageRead {
                        path: path_owned.display().to_string(),
                        reason: format!("Could not read image: {e}"),
                    })?;
                let resized = img.resize_exact(tw, th, image::imageops::FilterType::Triangle);
                (tw, th, resized.to_rgb8())
            };

            let mut bytes: Vec<u8> = Vec::new();
            {
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82);
                // `rgb` is an RgbImage by construction.
                encoder
                    .encode(
                        rgb.as_raw(),
                        rgb.width(),
                        rgb.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| AppError::operation(format!("Could not create thumbnail: {e}")))?;
            }

            // Atomic cache write: temp file + rename, never a half file.
            let tmp = cache_owned.with_extension("jpg.part");
            std::fs::write(&tmp, &bytes)
                .map_err(|e| AppError::io(e, cache_owned.display().to_string()))?;
            std::fs::rename(&tmp, &cache_owned)
                .map_err(|e| AppError::io(e, cache_owned.display().to_string()))?;

            Ok(Outcome::Ready(bytes, tw, th))
        })
        .await;

        match result {
            Ok(inner) => match inner {
                Ok(o) => o,
                Err(e) => Outcome::Fail(e),
            },
            Err(e) => Outcome::Fail(AppError::operation(format!("Thumbnail task failed: {e}"))),
        }
    }
}

fn image_dimensions_of(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Minimal base64 (standard alphabet, padding) — 20 lines instead of a
/// dependency. Round-trip tested below.
pub fn b64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn b64_data_url(jpeg: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", b64_encode(jpeg))
}

/// Base64 decode (used by tests to verify the encoder round-trip).
pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0;
    for b in s.bytes() {
        if b == b'=' {
            break;
        }
        let v = ALPHABET.iter().position(|&a| a == b)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pg_thumb_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn small_jpg(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let p = dir.join(name);
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(w, h, |x, y| Rgb([x as u8, y as u8, 128]));
        img.save(&p).unwrap();
        p
    }

    #[test]
    fn fnv_is_stable_and_distinct() {
        assert_eq!(fnv1a64(b"abc"), fnv1a64(b"abc"));
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
        assert_ne!(fnv1a64(b"a"), fnv1a64(b"b"));
    }

    #[test]
    fn base64_round_trips() {
        let cases: Vec<&[u8]> = vec![
            b"",
            b"a",
            b"ab",
            b"abc",
            b"hello world, gremlin",
            &[0, 1, 2, 250, 251, 255],
        ];
        for data in cases {
            let enc = b64_encode(data);
            assert_eq!(b64_decode(&enc).as_deref(), Some(data));
        }
    }

    #[test]
    fn cache_name_reacts_to_inputs() {
        let a = thumb_cache_name("/x/a.jpg", 100, Some("2026-01-01T00:00:00Z"), 256);
        let b = thumb_cache_name("/x/a.jpg", 101, Some("2026-01-01T00:00:00Z"), 256);
        let c = thumb_cache_name("/x/a.jpg", 100, Some("2026-01-02T00:00:00Z"), 256);
        let d = thumb_cache_name("/x/a.jpg", 100, Some("2026-01-01T00:00:00Z"), 1600);
        assert_eq!(a.len(), 20); // 16 hex + ".jpg"
        assert_ne!(a, b); // size changed
        assert_ne!(a, c); // mtime changed
        assert_ne!(a, d); // target size changed
        assert_eq!(
            a,
            thumb_cache_name("/x/a.jpg", 100, Some("2026-01-01T00:00:00Z"), 256)
        );
    }

    #[tokio::test]
    async fn generates_and_caches_a_thumbnail() {
        let dir = temp_dir("gen");
        let src = small_jpg(&dir, "big.jpg", 640, 480);
        let svc = ThumbService::new(dir.join("cache"));
        let meta = std::fs::metadata(&src).unwrap();
        let name = thumb_cache_name(src.to_str().unwrap(), meta.len() as i64, None, 256);
        let cache_file = svc.cache_dir().join(&name);

        let outcome = svc.generate(&src, &cache_file, GRID_MAX_WIDTH, crate::scanner::FileClass::Decodable).await;
        match outcome {
            Outcome::Ready(bytes, w, h) => {
                assert_eq!((w, h), (256, 192)); // 640x480 -> 256x192
                assert!(!bytes.is_empty());
                // Decodable as a valid JPEG.
                assert!(image_dimensions_of(&bytes) == Some((256, 192)));
            }
            Outcome::Fail(e) => panic!("unexpected failure: {e}"),
        }
        assert!(cache_file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn portrait_image_keeps_aspect() {
        // "max width" semantics: the width is capped, the aspect ratio is
        // preserved (grid CSS crops to the tile shape with object-fit).
        let dir = temp_dir("portrait");
        let src = small_jpg(&dir, "tall.jpg", 480, 640);
        let svc = ThumbService::new(dir.join("cache"));
        let outcome = svc
            .generate(&src, &dir.join("cache/tall.jpg"), GRID_MAX_WIDTH, crate::scanner::FileClass::Decodable)
            .await;
        match outcome {
            // 480 -> 256 (x0.5333), 640 -> 341.33 -> 341 (truncated)
            Outcome::Ready(_, w, h) => assert_eq!((w, h), (256, 341)),
            Outcome::Fail(e) => panic!("unexpected failure: {e}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_file_is_a_friendly_error() {
        let dir = temp_dir("missing");
        let svc = ThumbService::new(dir.join("cache"));
        let ghost = dir.join("ghost.jpg");
        let outcome = svc
            .generate(&ghost, &dir.join("cache/ghost.jpg"), GRID_MAX_WIDTH, crate::scanner::FileClass::Decodable)
            .await;
        match outcome {
            Outcome::Fail(e) => {
                let msg = e.to_string();
                assert!(msg.contains("ghost.jpg"), "msg: {msg}");
            }
            other => panic!("expected friendly error, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Full `get()` path with a real DB: miss -> generate -> hit.
    #[tokio::test]
    async fn get_hit_and_miss_lifecycle() {
        let dir = temp_dir("lifecycle");
        let src = small_jpg(&dir, "p.jpg", 300, 200);
        let dbp = dir.join("t.sqlite");
        let db = crate::database::Db::open(&dbp).unwrap();
        db.migrate().unwrap();
        let s = db.upsert_session("T", Some(dir.to_str().unwrap())).unwrap();
        let meta = std::fs::metadata(&src).unwrap();
        db.upsert_photo(&crate::database::PhotoUpsert {
            path: src.to_string_lossy().into_owned(),
            filename: "p.jpg".into(),
            extension: "jpg".into(),
            size_bytes: Some(meta.len() as i64),
            width: None,
            height: None,
            orientation: None,
            session_id: Some(s),
            file_mtime: None,
        })
        .unwrap();
        let photo_id: i64 = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM photos LIMIT 1", [], |r| r.get(0))
                .unwrap()
        };

        let svc = ThumbService::new(dir.join("cache"));
        let first = svc.get(&db, photo_id, ThumbKind::Grid).await.unwrap();
        assert!(!first.from_cache);
        assert!(first.data_url.starts_with("data:image/jpeg;base64,"));
        // 300x200 scaled to max-width 256: 200 * 256/300 = 170.67 -> 170 (truncated)
        assert_eq!((first.width, first.height), (256, 170));

        let second = svc.get(&db, photo_id, ThumbKind::Grid).await.unwrap();
        assert!(second.from_cache);
        assert_eq!(second.data_url, first.data_url); // same bytes from cache

        // Unknown photo id -> friendly error.
        let err = svc.get(&db, 999_999, ThumbKind::Grid).await.unwrap_err();
        assert!(err.to_string().contains("no longer in the library"));

        let _ = std::fs::remove_file(&dbp);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
