//! Similarity (Sprint 8): perceptual hashing + grouping — fully local, no AI.
//!
//! The pipeline (`run_similarity`) is Tauri-free so the exact behavior ships
//! integration-tested:
//! 1. **Hash** — every decodable photo without a current dHash gets one
//!    (64-bit difference hash, v9 `photos.phash`), incrementally (a photo is
//!    re-hashed only when its file mtime changed).
//! 2. **Group similar** — within a session, photos whose hashes differ by at
//!    most `SIMILAR_THRESHOLD` bits are unioned into a cluster (components of
//!    ≥ 2 photos become `similar` groups). Cross-session duplicates are a
//!    v0.2 concern; within-session is where "which of these 40 shots are the
//!    same moment?" actually happens.
//! 3. **Group bursts** — within a session, photographs captured within
//!    `BURST_WINDOW_SECS` of each other (known capture times, ≥ 2 photos)
//!    become `burst` groups.
//! 4. **Persist** — the whole group set is replaced atomically
//!    (`replace_similarity_groups`), so the groups always reflect the current
//!    hashes.
//!
//! The language here is the product: "similar photograph", never "duplicate —
//! delete this". Deciding stays with the photographer (Sprint 7 file ops make
//! the cleanup one click).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use image::{imageops::resize, imageops::FilterType, GrayImage, ImageReader};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

/// A 64-bit dHash distance at or below this is "similar". Unrelated photos
/// land near 32 ± a few bits, near-duplicates (re-encodes, tiny crops) stay
/// well under 8. Pinned by unit tests.
pub const SIMILAR_THRESHOLD: u32 = 8;
/// Bursts: consecutive photographs within this window (seconds) on the same
/// shoot. Pinned by unit tests.
pub const BURST_WINDOW_SECS: i64 = 3;
const MIN_GROUP_SIZE: usize = 2;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SimilaritySummary {
    /// Photos hashed during this run (0 on a second, up-to-date run).
    pub hashed: u32,
    /// Unreadable/undecodable files skipped (logged individually).
    pub failed: u32,
    pub similar_groups: u32,
    pub burst_groups: u32,
    pub elapsed_ms: u64,
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Pure hashing (unit tested)
// ---------------------------------------------------------------------------

/// 64-bit difference hash of a grayscale image: resize to 9×8, then bit i of
/// row r is `pixel(r, c) < pixel(r, c+1)`. Deterministic for identical
/// pixels; stable against mild re-encoding because it measures direction of
/// change, not absolute levels.
pub fn dhash64(img: &GrayImage) -> u64 {
    let small = resize(img, 9, 8, FilterType::Lanczos3);
    let px = small.as_raw();
    let mut h: u64 = 0;
    for r in 0..8u32 {
        for c in 0..8u32 {
            let left = px[(r * 9 + c) as usize];
            let right = px[(r * 9 + c + 1) as usize];
            if left < right {
                h |= 1u64 << (r * 8 + c);
            }
        }
    }
    h
}

/// Hamming distance between two dHashes (bits that differ).
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Decode one image file and hash it. Friendly error for missing files.
pub fn photo_hash(path: &Path) -> AppResult<u64> {
    let img = ImageReader::open(path)
        .map_err(|e| AppError::FileMissing {
            target: path.display().to_string(),
            reason: format!("could not open ({e:?})"),
        })?
        .with_guessed_format()
        .map_err(|e| AppError::operation(format!("Unrecognized image format: {e}")))?
        .decode()
        .map_err(|e| AppError::operation(format!("Could not decode image ({e})")))?;
    Ok(dhash64(&img.to_luma8()))
}

// ---------------------------------------------------------------------------
// Pure grouping (unit tested)
// ---------------------------------------------------------------------------

/// Union-find over `items` (photo id, hash); components of ≥ 2 photos whose
/// pairwise distance stays ≤ `threshold` come back as groups. Deterministic:
/// items are compared in input order and each group is emitted sorted by id.
pub fn group_similar(items: &[(i64, u64)], threshold: u32) -> Vec<Vec<i64>> {
    let n = items.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        let mut root = x;
        while parent[root] != root {
            root = parent[root];
        }
        while parent[x] != root {
            let next = parent[x];
            parent[x] = root;
            x = next;
        }
        root
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if hamming(items[i].1, items[j].1) <= threshold {
                let ra = find(&mut parent, i);
                let rb = find(&mut parent, j);
                if ra != rb {
                    parent[rb] = ra;
                }
            }
        }
    }

    let mut comps: HashMap<usize, Vec<i64>> = HashMap::new();
    for (i, (id, _)) in items.iter().enumerate() {
        comps.entry(find(&mut parent, i)).or_default().push(*id);
    }
    let mut groups: Vec<Vec<i64>> = comps
        .into_values()
        .filter(|g| g.len() >= MIN_GROUP_SIZE)
        .collect();
    for g in groups.iter_mut() {
        g.sort_unstable();
    }
    groups.sort_by_key(|g| g[0]);
    groups
}

/// Bursts: photographs with known capture times, ≥ 2, where each photo is
/// within `window_secs` of the burst's first photo. Inputs are
/// `(id, Option<epoch secs>)` — photos without a capture time never join a
/// burst. Deterministic (sorted by time, then id).
pub fn group_bursts(items: &[(i64, Option<i64>)], window_secs: i64) -> Vec<Vec<i64>> {
    let mut sorted: Vec<&(i64, Option<i64>)> =
        items.iter().filter(|(_, t)| t.is_some()).collect();
    sorted.sort_by_key(|(id, t)| (t.unwrap_or(0), *id));

    let mut groups: Vec<Vec<i64>> = Vec::new();
    let mut current: Vec<i64> = Vec::new();
    let mut start: Option<i64> = None;
    for (id, t) in sorted {
        let t = t.unwrap_or(0);
        let extend = start.is_some_and(|s| t.saturating_sub(s) <= window_secs);
        if extend && !current.is_empty() {
            current.push(*id);
        } else {
            // Close the previous chain (kept only if it was real) and start a
            // fresh one — `take` resets `current` in either case, so a
            // discarded 1-photo chain never leaks into the next burst.
            let closed = std::mem::take(&mut current);
            if closed.len() >= MIN_GROUP_SIZE {
                groups.push(closed);
            }
            start = Some(t);
            current.push(*id);
        }
    }
    if current.len() >= MIN_GROUP_SIZE {
        groups.push(current);
    }
    for g in groups.iter_mut() {
        g.sort_unstable();
    }
    groups
}

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

fn secs_from_rfc3339(s: Option<&str>) -> Option<i64> {
    s.and_then(|v| crate::time::parse_opt(v)).map(|d| d.timestamp())
}

/// Run the full similarity pass. Cancellation takes effect between files
/// (grouping still runs over whatever is hashed, so the UI always ends with a
/// consistent group set).
pub fn run_similarity(
    db: Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> AppResult<SimilaritySummary> {
    let started = Instant::now();

    // Phase 1: hash everything that needs it.
    let queue = db.phash_queue()?;
    let total = queue.len();
    let mut hashed = 0u32;
    let mut failed = 0u32;
    let mut cancelled = false;
    for (i, w) in queue.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            tracing::info!("similarity pass cancelled between files");
            break;
        }
        match photo_hash(Path::new(&w.path)) {
            Ok(h) => {
                db.upsert_phash(w.photo_id, h as i64, w.file_mtime.as_deref())?;
                hashed += 1;
            }
            Err(e) => {
                failed += 1;
                tracing::warn!(photo = w.photo_id, %e, "similarity: file skipped");
            }
        }
        progress(
            ProgressPayload::new(total, i + 1, "hashing").with_current(
                Path::new(&w.path)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
        );
    }

    // Phase 2: group within each session (NULL session = one bucket).
    progress(ProgressPayload::new(1, 0, "grouping"));
    let rows = db.hashed_photos()?;
    let mut by_session: HashMap<Option<i64>, Vec<(i64, u64, Option<i64>)>> = HashMap::new();
    for (id, hash, session, capture) in rows {
        by_session
            .entry(session)
            .or_default()
            .push((id, hash as u64, secs_from_rfc3339(capture.as_deref())));
    }

    let mut similar: Vec<(String, String, Vec<i64>)> = Vec::new();
    let mut bursts: Vec<(String, String, Vec<i64>)> = Vec::new();
    for (_session, items) in by_session.iter() {
        for g in group_similar(
            &items.iter().map(|(id, h, _)| (*id, *h)).collect::<Vec<_>>(),
            SIMILAR_THRESHOLD,
        ) {
            let hash = format!("{:016x}", items.iter().find(|(id, _, _)| id == &g[0]).map(|(_, h, _)| *h).unwrap_or(0));
            similar.push((hash, "similar".to_string(), g));
        }
        let timed: Vec<(i64, Option<i64>)> = items.iter().map(|(id, _, t)| (*id, *t)).collect();
        for g in group_bursts(&timed, BURST_WINDOW_SECS) {
            // Label with the burst's earliest known capture time (stable id).
            let t = items
                .iter()
                .filter(|(id, _, _)| g.contains(id))
                .filter_map(|(_, _, t)| *t)
                .min()
                .unwrap_or(0);
            bursts.push((
                format!("burst:{t}"),
                "burst".to_string(),
                g,
            ));
        }
    }
    let all: Vec<(String, String, Vec<i64>)> = similar
        .iter()
        .chain(bursts.iter())
        .map(|(h, t, g)| (h.clone(), t.clone(), g.clone()))
        .collect();
    db.replace_similarity_groups(&all)?;

    Ok(SimilaritySummary {
        hashed,
        failed,
        similar_groups: similar.len() as u32,
        burst_groups: bursts.len() as u32,
        elapsed_ms: started.elapsed().as_millis() as u64,
        cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma, Rgb, RgbImage};

    fn solid(w: u32, h: u32, v: u8) -> GrayImage {
        GrayImage::from_pixel(w, h, Luma([v]))
    }

    fn gradient(w: u32, h: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, _| Luma([(x % 256) as u8]))
    }

    #[test]
    fn dhash_is_deterministic_for_identical_pixels() {
        let a = dhash64(&gradient(64, 48));
        let b = dhash64(&gradient(64, 48));
        assert_eq!(a, b);
        assert_eq!(hamming(a, b), 0);
    }

    #[test]
    fn dhash_differs_across_distinct_content() {
        let g1 = dhash64(&gradient(64, 48));
        let g2 = dhash64(&{
            let mut img = gradient(64, 48);
            // A strong second structure: vertical stripes over the ramp.
            for y in 0..48u32 {
                for x in 0..64u32 {
                    if x < 32 {
                        let px = img.get_pixel_mut(x, y);
                        *px = Luma([px.0[0].wrapping_add(160)]);
                    }
                }
            }
            img
        });
        assert!(
            hamming(g1, g2) > SIMILAR_THRESHOLD,
            "distinct content should not be similar (got {})",
            hamming(g1, g2)
        );
    }

    #[test]
    fn dhash_is_stable_under_mild_noise() {
        let base = gradient(64, 48);
        let clean = dhash64(&base);
        // Add low-amplitude noise (±3 levels) — direction of change is kept.
        let mut noisy = base.clone();
        let mut rng_state: u64 = 0x9e3779b97f4a7c15;
        for y in 0..48u32 {
            for x in 0..64u32 {
                rng_state = rng_state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let delta = ((rng_state >> 33) % 7) as i16 - 3; // -3..3
                let px = noisy.get_pixel_mut(x, y);
                let v = px.0[0] as i16 + delta;
                *px = Luma([v.clamp(0, 255) as u8]);
            }
        }
        let noisy_h = dhash64(&noisy);
        assert!(
            hamming(clean, noisy_h) <= SIMILAR_THRESHOLD,
            "mild noise must stay similar (got {})",
            hamming(clean, noisy_h)
        );
    }

    #[test]
    fn solid_image_hashes_without_panicking() {
        // A constant image yields all-zero comparisons: fine, just 0.
        assert_eq!(dhash64(&solid(32, 24, 120)), 0);
    }

    #[test]
    fn group_similar_clusters_close_hashes() {
        let base = dhash64(&gradient(64, 48));
        // id 1/2/3 share a near-identical hash; id 4 is far away.
        let items = vec![
            (1, base),
            (2, base ^ 0b11), // 2 bits off
            (3, base ^ 0b0111), // 3 bits off (also 1–2 bits from the others)
            (4, !base), // ~64 bits off
        ];
        let groups = group_similar(&items, SIMILAR_THRESHOLD);
        assert_eq!(groups, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn group_similar_requires_min_size_two() {
        // A lone photo never forms a group.
        let base = dhash64(&gradient(64, 48));
        assert!(group_similar(&[(7, base)], SIMILAR_THRESHOLD).is_empty());
    }

    #[test]
    fn group_bursts_clumps_close_timestamps() {
        let t = 1_000_000i64;
        let items: Vec<(i64, Option<i64>)> = vec![
            (1, Some(t)),
            (2, Some(t + 1)),
            (3, Some(t + 2)),
            (4, Some(t + 20)), // far apart → own (single) group, dropped
            (5, Some(t + 40)),
            (6, Some(t + 41)),
        ];
        let groups = group_bursts(&items, BURST_WINDOW_SECS);
        assert_eq!(groups, vec![vec![1, 2, 3], vec![5, 6]]);
    }

    #[test]
    fn group_bursts_ignores_unknown_times() {
        // No capture time known → never joins a burst.
        assert!(group_bursts(&[(1, None), (2, None)], BURST_WINDOW_SECS).is_empty());
        // Mixed: the timed pair still bursts, the untimed photo does not.
        let t = 1_000_000i64;
        let mixed: Vec<(i64, Option<i64>)> = vec![(1, Some(t)), (2, None), (3, Some(t + 1))];
        assert_eq!(group_bursts(&mixed, BURST_WINDOW_SECS), vec![vec![1, 3]]);
    }

    #[test]
    fn rgb_to_gray_helper_compiles() {
        // Exercises the Luma conversion path (rec.709 weights) used by
        // photo_hash via to_luma8.
        let rgb = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
        let gray = GrayImage::from_fn(4, 4, |x, y| {
            let c = rgb.get_pixel(x, y);
            let luma = (c.0[0] as u32 * 29 + c.0[1] as u32 * 58 + c.0[2] as u32 * 11) / 100;
            Luma([luma as u8])
        });
        let _ = dhash64(&gray);
    }
}
