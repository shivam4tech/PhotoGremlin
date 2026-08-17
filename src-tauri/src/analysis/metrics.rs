//! Deterministic image measurements (Sprint 4). Pure functions of the pixel
//! data — no I/O, no Tauri — so every metric is unit-testable against
//! synthetic images (see tests in `mod.rs` of the parent module).
//!
//! All continuous scores are normalized 0–100 (see docs/IMAGE_ANALYSIS.md).
//! These are technical estimates, never quality verdicts.

use image::RgbImage;

/// Monochrome gate: mean per-pixel saturation below this counts as
/// "likely monochrome" (≈ 15/255). Conservative by design — foggy color
/// scenes can dip below this, and that is an acceptable false positive
/// for a *flag* that the photographer can override with filters.
pub const MONO_SAT_MAX: f64 = 0.06;
/// Monochrome gate: every channel-pair mean difference below this
/// (0–255 gray levels) confirms the image is achromatic.
pub const MONO_CHANNEL_SIM: f64 = 8.0;
/// Pixels with luma >= this count as highlight clipping.
pub const CLIP_HIGH_LUMA: u32 = 250;
/// Pixels with luma <= this count as shadow clipping.
pub const CLIP_LOW_LUMA: u32 = 5;
/// Brightness categories (0–100): <15 very dark, <35 dark, 35–65 normal,
/// >65 bright, >85 very bright. Flags use the 35/65 boundaries.
pub const DARK_BELOW: f64 = 35.0;
pub const BRIGHT_ABOVE: f64 = 65.0;
/// Contrast: sigma saturates near this (visually maxed for photos),
/// percentile spread saturates near this gray range.
pub const CONTRAST_SIGMA_SATURATION: f64 = 70.0;
pub const CONTRAST_SPREAD_SATURATION: f64 = 180.0;
/// Sharpness sigmoid calibration (v0.1): typical sharp photo material at a
/// ~2 MP working resolution has log10(laplacian variance) around 3.5–4.5,
/// visibly soft material below ~2.5. μ=3.5, k=0.6 maps that spread onto
/// 0–100 with headroom at both ends. Recalibrate when the working
/// resolution or filter changes and bump ANALYSIS_ALGORITHM_VERSION.
pub const SHARP_SIGMOID_MU: f64 = 3.5;
pub const SHARP_SIGMOID_K: f64 = 0.6;

/// One photo's technical measurements. Stored 1:1 in the `analysis` table.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Metrics {
    /// 0–100, variance of the Laplacian (edge energy), sigmoid-scaled.
    pub sharpness: f64,
    /// 0–100, mean Rec.709 luma rescaled.
    pub brightness: f64,
    /// 0–100, blend of luma sigma and p95−p5 spread.
    pub contrast: f64,
    /// 0–100, mean per-pixel saturation × 255, clamped.
    pub saturation: f64,
    /// 0–100, % of pixels with luma >= 250.
    pub highlight_clipping: f64,
    /// 0–100, % of pixels with luma <= 5.
    pub shadow_clipping: f64,
    pub is_monochrome: bool,
    pub is_dark: bool,
    pub is_bright: bool,
}

/// Rec. 709 luma, rounded to the nearest gray level.
pub fn luma(r: u32, g: u32, b: u32) -> u32 {
    let v = 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b);
    v.round() as u32
}

/// Sigmoid-scaled sharpness from a Laplacian variance (see module docs).
pub fn sharpness_score(laplacian_variance: f64) -> f64 {
    let v = laplacian_variance.max(1e-9);
    let x = (v.log10() - SHARP_SIGMOID_MU) / SHARP_SIGMOID_K;
    100.0 / (1.0 + (-x).exp())
}

/// 0-centered percentile from a 256-bin luma histogram (0–255 scale).
fn histogram_percentile(hist: &[u64; 256], p: f64) -> f64 {
    let n = hist.iter().sum::<u64>();
    if n == 0 {
        return 0.0;
    }
    let target = (p / 100.0) * n as f64;
    let mut seen = 0u64;
    for (value, count) in hist.iter().enumerate() {
        seen += count;
        if seen as f64 >= target {
            return value as f64;
        }
    }
    255.0
}

/// Compute every Sprint-4 metric from a single RGB image.
/// Deterministic: same pixels → same metrics, on any platform.
pub fn measure(rgb: &RgbImage) -> Metrics {
    let n = u64::from(rgb.width()) * u64::from(rgb.height());
    let mut hist = [0u64; 256];
    let mut sum_r = 0.0f64;
    let mut sum_g = 0.0f64;
    let mut sum_b = 0.0f64;
    let mut sum_sat = 0.0f64;

    for px in rgb.pixels() {
        let r = u32::from(px[0]);
        let g = u32::from(px[1]);
        let b = u32::from(px[2]);
        let l = luma(r, g, b).min(255) as usize;
        hist[l] += 1;
        sum_r += r as f64;
        sum_g += g as f64;
        sum_b += b as f64;
        let mx = r.max(g).max(b);
        let mn = r.min(g).min(b);
        if mx > 0 {
            sum_sat += (mx - mn) as f64 / f64::from(mx);
        }
    }

    // Luma moments + percentiles from the histogram (one pass, no sort).
    let n64 = n as f64;
    let mut sum_luma = 0.0f64;
    let mut sum_luma_sq = 0.0f64;
    for (value, count) in hist.iter().enumerate() {
        let x = value as f64;
        let c = *count as f64;
        sum_luma += x * c;
        sum_luma_sq += x * x * c;
    }
    let mean_luma = sum_luma / n64;
    let variance = (sum_luma_sq / n64 - mean_luma * mean_luma).max(0.0);
    let sigma = variance.sqrt();

    let p5 = histogram_percentile(&hist, 5.0);
    let p95 = histogram_percentile(&hist, 95.0);

    let high = hist[CLIP_HIGH_LUMA as usize..].iter().sum::<u64>();
    let low = hist[..=CLIP_LOW_LUMA as usize].iter().sum::<u64>();

    let brightness = 100.0 * mean_luma / 255.0;
    let contrast = 0.6 * 100.0 * (sigma / CONTRAST_SIGMA_SATURATION).min(1.0)
        + 0.4 * 100.0 * ((p95 - p5) / CONTRAST_SPREAD_SATURATION).min(1.0);
    let saturation = (255.0 * sum_sat / n64).min(100.0);

    let mean_r = sum_r / n64;
    let mean_g = sum_g / n64;
    let mean_b = sum_b / n64;
    let mean_sat = sum_sat / n64;
    let channel_similar = (mean_r - mean_g).abs() < MONO_CHANNEL_SIM
        && (mean_g - mean_b).abs() < MONO_CHANNEL_SIM
        && (mean_r - mean_b).abs() < MONO_CHANNEL_SIM;
    let is_monochrome = mean_sat < MONO_SAT_MAX && channel_similar;

    Metrics {
        sharpness: sharpness_from_luma(rgb),
        brightness,
        contrast,
        saturation,
        highlight_clipping: 100.0 * high as f64 / n64,
        shadow_clipping: 100.0 * low as f64 / n64,
        is_monochrome,
        is_dark: brightness < DARK_BELOW,
        is_bright: brightness > BRIGHT_ABOVE,
    }
}

/// Variance of the 4-neighbor Laplacian over the image interior.
/// Edge energy proxy: flat material ≈ 0, busy/sharp material is high.
fn sharpness_from_luma(rgb: &RgbImage) -> f64 {
    let w = rgb.width() as isize;
    let h = rgb.height() as isize;
    if w < 3 || h < 3 {
        return 0.0;
    }
    let lx = |px: &image::Rgb<u8>| luma(px[0] as u32, px[1] as u32, px[2] as u32) as f64;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut count = 0u64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = lx(rgb.get_pixel(x as u32, y as u32));
            let v = lx(rgb.get_pixel(x as u32, (y - 1) as u32))
                + lx(rgb.get_pixel(x as u32, (y + 1) as u32))
                + lx(rgb.get_pixel((x - 1) as u32, y as u32))
                + lx(rgb.get_pixel((x + 1) as u32, y as u32))
                - 4.0 * c;
            sum += v;
            sum_sq += v * v;
            count += 1;
        }
    }
    let cn = count as f64;
    let mean = sum / cn;
    let var = (sum_sq / cn - mean * mean).max(0.0);
    sharpness_score(var)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageBuffer;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbImage {
        ImageBuffer::from_pixel(w, h, image::Rgb(rgb))
    }

    fn from_fn(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> RgbImage {
        ImageBuffer::from_fn(w, h, |x, y| image::Rgb(f(x, y)))
    }

    #[test]
    fn luma_matches_rec709() {
        assert_eq!(luma(255, 0, 0), 54); // 0.2126*255
        assert_eq!(luma(0, 255, 0), 182); // 0.7152*255
        assert_eq!(luma(0, 0, 255), 18); // 0.0722*255
        assert_eq!(luma(255, 255, 255), 255);
        assert_eq!(luma(0, 0, 0), 0);
    }

    #[test]
    fn brightness_of_solid_grays() {
        let mid = measure(&solid(64, 64, [128, 128, 128]));
        assert!((mid.brightness - 50.2).abs() < 0.5, "got {}", mid.brightness);
        let dark = measure(&solid(64, 64, [10, 10, 10]));
        assert!((dark.brightness - 3.9).abs() < 0.5);
        let bright = measure(&solid(64, 64, [245, 245, 245]));
        assert!((bright.brightness - 96.1).abs() < 0.5);
    }

    #[test]
    fn dark_and_bright_flags() {
        assert!(measure(&solid(32, 32, [10, 10, 10])).is_dark);
        assert!(!measure(&solid(32, 32, [10, 10, 10])).is_bright);
        assert!(measure(&solid(32, 32, [245, 245, 245])).is_bright);
        assert!(!measure(&solid(32, 32, [245, 245, 245])).is_dark);
        let normal = measure(&solid(32, 32, [128, 128, 128]));
        assert!(!normal.is_dark && !normal.is_bright);
    }

    #[test]
    fn saturation_extremes() {
        // Fully saturated magenta: per-pixel sat = 1 → 100 after clamp.
        let magenta = measure(&solid(16, 16, [255, 0, 255]));
        assert_eq!(magenta.saturation, 100.0);
        assert!(!magenta.is_monochrome);
        // Achromatic gray: sat 0.
        let gray = measure(&solid(16, 16, [128, 128, 128]));
        assert_eq!(gray.saturation, 0.0);
        assert!(gray.is_monochrome);
    }

    #[test]
    fn monochrome_gating() {
        // Near-gray with a tiny cast: sat ≈ 0.023 < 0.06 AND channel means
        // differ by < 8 → flagged monochrome (conservative by design).
        let near_gray = measure(&solid(16, 16, [128, 131, 126]));
        assert!(near_gray.is_monochrome);
        // A color image fails even at moderate saturation.
        let color = measure(&solid(16, 16, [200, 80, 40]));
        assert!(!color.is_monochrome);
        // Passes the sat gate (11/200 = 0.055 < 0.06) but fails the
        // channel-similarity gate (R-B = 11 >= 8) → not flagged. This is
        // the faded-color path the sat gate alone would miss.
        let faded = measure(&solid(16, 16, [200, 196, 189]));
        assert!(!faded.is_monochrome);
    }

    #[test]
    fn clipping_percentages() {
        // Half pure white (luma 255 ≥ 250), half mid-gray (100).
        let img16 = from_fn(64, 64, |x, _| {
            if x < 32 {
                [255, 255, 255]
            } else {
                [100, 100, 100]
            }
        });
        let m = measure(&img16);
        assert!((m.highlight_clipping - 50.0).abs() < 1.0, "got {}", m.highlight_clipping);
        assert_eq!(m.shadow_clipping, 0.0);

        // Half pure black (luma 0 ≤ 5), half mid-gray.
        let img2 = from_fn(64, 64, |x, _| {
            if x < 32 {
                [0, 0, 0]
            } else {
                [128, 128, 128]
            }
        });
        let m2 = measure(&img2);
        assert!((m2.shadow_clipping - 50.0).abs() < 1.0, "got {}", m2.shadow_clipping);
        assert_eq!(m2.highlight_clipping, 0.0);

        // Nothing clipped in the middle.
        assert_eq!(measure(&solid(16, 16, [128, 128, 128])).highlight_clipping, 0.0);
        assert_eq!(measure(&solid(16, 16, [128, 128, 128])).shadow_clipping, 0.0);
    }

    #[test]
    fn sharpness_orders_sharp_above_smooth() {
        // 8×8 checkerboard: strong edge energy.
        let sharp = from_fn(256, 256, |x, y| {
            if (x / 8 + y / 8) % 2 == 0 {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            }
        });
        // Linear ramp: the Laplacian of a linear function is ~0.
        let smooth = from_fn(256, 256, |x, _| [(x % 256) as u8, (x % 256) as u8, (x % 256) as u8]);
        let ms = measure(&sharp);
        let mu = measure(&smooth);
        assert!(
            ms.sharpness > mu.sharpness + 15.0,
            "sharp {} should beat smooth {}",
            ms.sharpness,
            mu.sharpness
        );
        assert!((0.0..=100.0).contains(&ms.sharpness));
        assert!((0.0..=100.0).contains(&mu.sharpness));
    }

    #[test]
    fn contrast_orders_spread_above_flat() {
        let high_contrast = from_fn(64, 64, |x, _| {
            if x % 2 == 0 {
                [250, 250, 250]
            } else {
                [5, 5, 5]
            }
        });
        let low_contrast = from_fn(64, 64, |x, _| {
            let v = (120 + (x % 4) * 4) as u8; // 120..131 band
            [v, v, v]
        });
        let hi = measure(&high_contrast);
        let lo = measure(&low_contrast);
        assert!(hi.contrast > lo.contrast + 10.0, "hi {} vs lo {}", hi.contrast, lo.contrast);
        assert!((0.0..=100.0).contains(&hi.contrast));
        assert!((0.0..=100.0).contains(&lo.contrast));
    }

    #[test]
    fn sigmoid_scale_sane() {
        // Flat field: near-zero laplacian variance → ~0.
        assert!(sharpness_score(1e-6) < 5.0);
        // Typical sharp material variance → high score.
        assert!(sharpness_score(10f64.powf(4.6)) > 80.0);
        // Monotonic.
        assert!(sharpness_score(100.0) < sharpness_score(10_000.0));
    }

    #[test]
    fn degenerate_sizes_do_not_panic() {
        let m1 = measure(&solid(1, 1, [200, 100, 50]));
        assert!((0.0..=100.0).contains(&m1.sharpness)); // <3px → 0
        assert_eq!(m1.sharpness, 0.0);
        let m2 = measure(&solid(2, 2, [10, 200, 10]));
        assert_eq!(m2.sharpness, 0.0);
        assert!(m2.brightness > 0.0);
    }
}
