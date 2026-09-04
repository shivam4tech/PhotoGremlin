//! EXIF distribution binning (Sprint 6). Pure functions with pinned,
//! documented boundary rules (STATISTICS.md). Bins are half-open
//! `[lo, hi)` except the last of a family, which is closed; values below a
//! family's first lower bound clip into the first bin (sensors report ISO
//! 50; apertures open past f/1.4), so every value lands in exactly one bin.

/// ISO bins, low → high. `iso_bin(400)` = "400–800" (400 is the upper bound
/// of the first bin, lower bound of the second).
pub const ISO_BINS: &[&str] = &["100–400", "400–800", "800–1600", "1600–3200", "3200+"];

pub fn iso_bin(iso: i64) -> &'static str {
    match iso {
        i if i < 400 => ISO_BINS[0],
        i if i < 800 => ISO_BINS[1],
        i if i < 1600 => ISO_BINS[2],
        i if i < 3200 => ISO_BINS[3],
        _ => ISO_BINS[4],
    }
}

/// Aperture bins (f-numbers), wide → narrow.
pub const APERTURE_BINS: &[&str] = &["f/1.4–2.0", "f/2.0–2.8", "f/2.8–4.0", "f/4–8", "f/8+"];

pub fn aperture_bin(f: f64) -> &'static str {
    match f {
        v if v < 2.0 => APERTURE_BINS[0],
        v if v < 2.8 => APERTURE_BINS[1],
        v if v < 4.0 => APERTURE_BINS[2],
        v if v < 8.0 => APERTURE_BINS[3],
        _ => APERTURE_BINS[4],
    }
}

/// Focal length buckets: assign to the nearest of the fixed set. 60 mm →
/// "50 mm" (10 away vs 25 from 85); 72 mm → "85 mm". A value exactly half-way
/// (rare in reality) rounds to the higher bucket.
pub const FOCAL_BUCKETS_MM: &[u32] = &[24, 35, 50, 85, 135];
pub const FOCAL_LABELS: &[&str] = &["24 mm", "35 mm", "50 mm", "85 mm", "135 mm"];

pub fn focal_bin(mm: f64) -> &'static str {
    let best = FOCAL_BUCKETS_MM
        .iter()
        .enumerate()
        .map(|(i, &b)| (i, (mm - f64::from(b)).abs()))
        .min_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0).reverse())
        })
        .map(|(i, _)| i)
        .unwrap_or(FOCAL_LABELS.len() - 1);
    FOCAL_LABELS[best]
}

/// Shutter bins, slow → fast, in seconds. The labels follow STATISTICS.md's
/// specification verbatim; the final labeled bin "1/8000+" is the overflow
/// bucket (everything faster than 1/4000 s).
pub const SHUTTER_BINS: &[&str] = &[
    "1s+",
    "1/2–1",
    "1/30–1/2",
    "1/125–1/30",
    "1/1000–1/125",
    "1/4000–1/1000",
    "1/8000+",
];

pub fn shutter_bin(seconds: f64) -> &'static str {
    match seconds {
        s if s >= 1.0 => SHUTTER_BINS[0],
        s if s >= 0.5 => SHUTTER_BINS[1],
        s if s >= 1.0 / 30.0 => SHUTTER_BINS[2],
        s if s >= 1.0 / 125.0 => SHUTTER_BINS[3],
        s if s >= 1.0 / 1000.0 => SHUTTER_BINS[4],
        s if s >= 1.0 / 4000.0 => SHUTTER_BINS[5],
        _ => SHUTTER_BINS[6],
    }
}

/// One histogram cell: a fixed bin label + its count.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BinCount {
    pub label: &'static str,
    pub count: u32,
}

/// Count per bin, in the fixed label order (zero bins included, so the UI
/// renders a stable shape).
pub fn bin_counts<T>(
    values: &[Option<T>],
    bin_of: fn(T) -> &'static str,
    labels: &[&'static str],
) -> Vec<BinCount>
where
    T: Copy,
{
    let mut counts = vec![0u32; labels.len()];
    for v in values {
        if let Some(v) = v {
            let l = bin_of(*v);
            if let Some(i) = labels.iter().position(|x| *x == l) {
                counts[i] += 1;
            }
        }
    }
    labels
        .iter()
        .zip(counts)
        .map(|(label, count)| BinCount { label, count })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_boundaries() {
        assert_eq!(iso_bin(50), "100–400"); // below-range clips into first
        assert_eq!(iso_bin(100), "100–400");
        assert_eq!(iso_bin(399), "100–400");
        assert_eq!(iso_bin(400), "400–800"); // documented edge case
        assert_eq!(iso_bin(799), "400–800");
        assert_eq!(iso_bin(1600), "1600–3200");
        assert_eq!(iso_bin(3200), "3200+");
        assert_eq!(iso_bin(12800), "3200+");
    }

    #[test]
    fn aperture_boundaries() {
        assert_eq!(aperture_bin(1.2), "f/1.4–2.0");
        assert_eq!(aperture_bin(1.4), "f/1.4–2.0");
        assert_eq!(aperture_bin(2.0), "f/2.0–2.8");
        assert_eq!(aperture_bin(2.8), "f/2.8–4.0");
        assert_eq!(aperture_bin(4.0), "f/4–8");
        assert_eq!(aperture_bin(8.0), "f/8+");
        assert_eq!(aperture_bin(16.0), "f/8+");
    }

    #[test]
    fn focal_nearest_bucket() {
        assert_eq!(focal_bin(20.0), "24 mm");
        assert_eq!(focal_bin(30.0), "35 mm"); // 30: 6 from 24, 5 from 35
        assert_eq!(focal_bin(50.0), "50 mm");
        assert_eq!(focal_bin(60.0), "50 mm");
        assert_eq!(focal_bin(72.0), "85 mm");
        assert_eq!(focal_bin(100.0), "85 mm");
        assert_eq!(focal_bin(160.0), "135 mm");
    }

    #[test]
    fn shutter_boundaries() {
        assert_eq!(shutter_bin(2.0), "1s+");
        assert_eq!(shutter_bin(1.0), "1s+");
        assert_eq!(shutter_bin(0.5), "1/2–1");
        assert_eq!(shutter_bin(0.25), "1/30–1/2");
        assert_eq!(shutter_bin(1.0 / 30.0), "1/30–1/2");
        assert_eq!(shutter_bin(1.0 / 125.0), "1/125–1/30");
        assert_eq!(shutter_bin(1.0 / 1000.0), "1/1000–1/125");
        assert_eq!(shutter_bin(1.0 / 4000.0), "1/4000–1/1000");
        assert_eq!(shutter_bin(1.0 / 8000.0), "1/8000+");
        assert_eq!(shutter_bin(1e-6), "1/8000+");
    }

    #[test]
    fn bin_counts_keeps_zero_bins_and_order() {
        let counts = bin_counts(&[Some(100i64), None, Some(1600)], iso_bin, ISO_BINS);
        assert_eq!(counts.len(), 5);
        let sums: u32 = counts.iter().map(|b| b.count).sum();
        assert_eq!(sums, 2);
        assert_eq!(counts[0].count, 1);
        assert_eq!(counts[3].count, 1);
        assert_eq!(counts[1], BinCount { label: "400–800", count: 0 });
    }
}
