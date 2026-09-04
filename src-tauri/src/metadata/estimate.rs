//! Capture-date estimation (Sprint 12). Pure: filename + the file's stored
//! mtime in, `DateEstimate` out. No filesystem, no Tauri, no database.
//!
//! Why: EXIF `DateTimeOriginal` is missing on a large share of real
//! libraries (screenshots, web downloads, re-exports). The capture date is
//! *derivable* — camera apps stamp the timestamp into the filename, and the
//! file's modification time is the last resort. ISO/shutter/aperture are
//! physical settings with no recoverable trace in pixels or names — they are
//! never estimated (honesty rule, see DATABASE.md).
//!
//! Conventions:
//! - Same date convention as EXIF: the wall-clock time is stored verbatim as
//!   UTC RFC3339 (lexicographically sortable, no timezone assumption).
//! - The estimate is always labelled: `DateEstimate.source` is `"filename"`
//!   or `"mtime"`, and the catalog records it per photo
//!   (`photos.capture_datetime_source`), so the UI can say "date estimated
//!   from file time" instead of pretending precision it does not have.
//! - Dominance: filename patterns beat mtime (a camera-app filename is a
//!   capture-time record; mtime only reflects copy/export moments).
//!   Best-match order below.

/// A derived capture date and its provenance label.
#[derive(Debug, Clone, PartialEq)]
pub struct DateEstimate {
    /// UTC RFC3339 (verbatim wall clock, seconds precision).
    pub datetime: String,
    /// `"filename"` (camera-app naming) or `"mtime"` (file modification).
    pub source: &'static str,
}

/// Estimate a photo's capture datetime from its file name, falling back to
/// the stored modification time. Returns `None` only when even the mtime is
/// unknown (a file the scanner never wrote `file_mtime` for).
pub fn estimate_datetime(filename: &str, file_mtime: Option<&str>) -> Option<DateEstimate> {
    if let Some(dt) = estimate_from_filename(filename) {
        return Some(DateEstimate {
            datetime: dt,
            source: "filename",
        });
    }
    file_mtime.map(|t| DateEstimate {
        datetime: t.to_string(),
        source: "mtime",
    })
}

/// Filename patterns cameras and apps use; first match wins (order matters).
fn estimate_from_filename(filename: &str) -> Option<String> {
    let stem = filename.trim().rsplit_once('.')?.0; // drop the extension
    let upper = stem.to_ascii_uppercase();

    // Camera-roll family: IMG_/VID_/PXL_ + YYYYMMDD_HHMMSS (Android/iPhone
    // roll, Google Pixel). Also MVIMG_ (Pixel motion photo).
    for prefix in ["IMG", "VID", "PXL", "MVIMG"] {
        if let Some(rest) = upper.strip_prefix(&format!("{prefix}_")) {
            if let Some(dt) = parse_compact_datetime(rest) {
                return Some(dt);
            }
        }
    }

    // Screenshots: Windows "Screenshot_2025-01-09-143022…",
    // macOS "Screenshot 2025-01-09 at 14.30.22…", and the compact forms.
    if upper.starts_with("SCREENSHOT") {
        if let Some(dt) = parse_loose_datetime(&upper) {
            return Some(dt);
        }
    }

    // Generic: date+time digits in common separators, anywhere in the name
    // (20250101_143022, 2025-01-09 14.30.22, IMG20250109143022…).
    parse_loose_datetime(&upper)
}

/// "YYYYMMDD_HHMMSS" (also with `-`, `:`, `.` or nothing between the parts).
fn parse_compact_datetime(s: &str) -> Option<String> {
    let digits: Vec<u8> = s.chars().filter(|c| c.is_ascii_digit()).map(|c| c as u8 - b'0').collect();
    if digits.len() < 14 {
        return None;
    }
    let date = decode_ymd(&digits[..8])?;
    let time = chrono::NaiveTime::from_hms_opt(
        u32::from(digits[8]) * 10 + u32::from(digits[9]),
        u32::from(digits[10]) * 10 + u32::from(digits[11]),
        u32::from(digits[12]) * 10 + u32::from(digits[13]),
    )?;
    combine(date, time)
}

fn is_digit2(b: &[u8]) -> bool {
    b.len() == 2 && b.iter().all(|c| c.is_ascii_digit())
}

fn is_sep(b: u8) -> bool {
    b == b'.' || b == b'-' || b == b'_' || b == b':' || b == b' '
}

/// Find a time (HHMMSS run, or H.M.S / H-M-S / H:M:S) inside `bytes[start..]`
/// within `window` chars. Returns `None` when the window holds no time.
fn time_in_window(bytes: &[u8], start: usize, window: usize) -> Option<chrono::NaiveTime> {
    let end = (start + window).min(bytes.len());
    let mut j = start;
    while j + 6 <= end {
        // 6-digit run.
        if bytes[j..j + 6].iter().all(|b| b.is_ascii_digit()) {
            return chrono::NaiveTime::from_hms_opt(
                u32::from(bytes[j] - b'0') * 10 + u32::from(bytes[j + 1] - b'0'),
                u32::from(bytes[j + 2] - b'0') * 10 + u32::from(bytes[j + 3] - b'0'),
                u32::from(bytes[j + 4] - b'0') * 10 + u32::from(bytes[j + 5] - b'0'),
            );
        }
        // H.M.S / H-M-S / H:M:S triple.
        let mut t = 1usize;
        while j + t < end && bytes[j + t].is_ascii_digit() {
            t += 1;
        }
        if t >= 1
            && j + t < end
            && is_sep(bytes[j + t])
            && j + t + 3 <= end
            && is_digit2(&bytes[j + t + 1..j + t + 3])
        {
            let sep = bytes[j + t];
            let mut k = j + t + 3;
            while k < end && bytes[k].is_ascii_digit() {
                k += 1;
            }
            if k < end && bytes[k] == sep && k + 2 <= end && is_digit2(&bytes[k + 1..k + 3]) {
                let v: Vec<u8> = bytes[j..k + 3]
                    .iter()
                    .copied()
                    .filter(|b| b.is_ascii_digit())
                    .collect();
                if v.len() == 6 {
                    return chrono::NaiveTime::from_hms_opt(
                        u32::from(v[0] - b'0') * 10 + u32::from(v[1] - b'0'),
                        u32::from(v[2] - b'0') * 10 + u32::from(v[3] - b'0'),
                        u32::from(v[4] - b'0') * 10 + u32::from(v[5] - b'0'),
                    );
                }
            }
        }
        j += 1;
    }
    None
}

/// Loose search for a date anywhere in an uppercased stem: a compact 8-digit
/// run `YYYYMMDD` or a dashed `YYYY-sep-MM-sep-DD`, each followed by an
/// optional time inside a short window (`HHMMSS`, `HH.MM.SS`, …). A date
/// with no time anywhere in the window yields day-precision midnight, but
/// only when the window holds no digits at all (a truncated
/// `YYYYMMDDHHMM` name is ambiguous and must not be pinned to midnight).
fn parse_loose_datetime(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    while i + 8 <= n {
        // Compact 8-digit run.
        if bytes[i..i + 8].iter().all(|b| b.is_ascii_digit()) {
            let ymd = decode_ymd(&bytes[i..i + 8].iter().map(|b| b - b'0').collect::<Vec<_>>())?;
            if let Some(t) = time_in_window(bytes, i + 8, 16) {
                return combine(ymd, t);
            }
            if !bytes[i + 8..(i + 24).min(n)].iter().any(|b| b.is_ascii_digit()) {
                return combine(ymd, chrono::NaiveTime::from_hms_opt(0, 0, 0)?);
            }
            i += 1;
        } else if bytes[i..i + 4].iter().all(|b| b.is_ascii_digit())
            && i + 10 <= n
            && is_sep(bytes[i + 4])
            && is_digit2(&bytes[i + 5..i + 7])
            && is_sep(bytes[i + 7])
            && is_digit2(&bytes[i + 8..i + 10])
            && (i + 10 == n || !bytes[i + 10].is_ascii_digit())
        {
            // Dashed date: YYYY-sep-MM-sep-DD (allow a wide window for
            // "at 14.30.22" forms).
            let ymd = decode_ymd(&[
                bytes[i] - b'0',
                bytes[i + 1] - b'0',
                bytes[i + 2] - b'0',
                bytes[i + 3] - b'0',
                bytes[i + 5] - b'0',
                bytes[i + 6] - b'0',
                bytes[i + 8] - b'0',
                bytes[i + 9] - b'0',
            ])?;
            if let Some(t) = time_in_window(bytes, i + 10, 24) {
                return combine(ymd, t);
            }
            return combine(ymd, chrono::NaiveTime::from_hms_opt(0, 0, 0)?);
        } else {
            i += 1;
        }
    }
    None
}

fn decode_ymd(d: &[u8]) -> Option<chrono::NaiveDate> {
    if d.len() != 8 {
        return None;
    }
    let y = d[0] as i32 * 1000 + d[1] as i32 * 100 + d[2] as i32 * 10 + d[3] as i32;
    let m = d[4] as u32 * 10 + d[5] as u32;
    let day = d[6] as u32 * 10 + d[7] as u32;
    chrono::NaiveDate::from_ymd_opt(y, m, day)
}

fn combine(date: chrono::NaiveDate, time: chrono::NaiveTime) -> Option<String> {
    Some(date.and_time(time).and_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_roll_patterns() {
        // Android/iPhone camera roll.
        assert_eq!(
            estimate_from_filename("IMG_20250101_143022.jpg").as_deref(),
            Some("2025-01-01T14:30:22Z")
        );
        // Pixel motion photo.
        assert_eq!(
            estimate_from_filename("MVIMG_20240315_091011.jpg").as_deref(),
            Some("2024-03-15T09:10:11Z")
        );
        // Video still.
        assert_eq!(
            estimate_from_filename("VID_20220605_123000.mp4.jpg").as_deref(),
            Some("2022-06-05T12:30:00Z")
        );
        // Lowercase file name — stems are normalized before matching.
        assert_eq!(
            estimate_from_filename("img_20210704_080001.JPG").as_deref(),
            Some("2021-07-04T08:00:01Z")
        );
        // SCREENSHOT_ + a naked date: not the camera-roll compact form, but
        // the loose scan still recovers the day (midnight precision).
        assert_eq!(
            estimate_from_filename("SCREENSHOT_IMG_20250101.jpg").as_deref(),
            Some("2025-01-01T00:00:00Z")
        );
    }

    #[test]
    fn screenshot_patterns() {
        // Windows: Screenshot_2025-01-09-143022_WhatsApp.png
        assert_eq!(
            estimate_from_filename("Screenshot_2025-01-09-143022_WhatsApp.png").as_deref(),
            Some("2025-01-09T14:30:22Z")
        );
        // macOS: Screenshot 2025-01-09 at 14.30.22.png
        assert_eq!(
            estimate_from_filename("Screenshot 2025-01-09 at 14.30.22.png").as_deref(),
            Some("2025-01-09T14:30:22Z")
        );
    }

    #[test]
    fn generic_date_time_forms() {
        assert_eq!(
            estimate_from_filename("20250101_143022.png").as_deref(),
            Some("2025-01-01T14:30:22Z")
        );
        assert_eq!(
            estimate_from_filename("holiday_20220605_123000.jpg").as_deref(),
            Some("2022-06-05T12:30:00Z")
        );
        assert_eq!(
            estimate_from_filename("IAN1208_20250214_091122.jpg").as_deref(),
            Some("2025-02-14T09:11:22Z")
        );
        // Underscore-less run.
        assert_eq!(
            estimate_from_filename("IMG20250109143022.jpg").as_deref(),
            Some("2025-01-09T14:30:22Z")
        );
        // Day-only names get midnight.
        assert_eq!(
            estimate_from_filename("20250101.jpg").as_deref(),
            Some("2025-01-01T00:00:00Z")
        );
    }

    #[test]
    fn invalid_dates_rejected() {
        assert_eq!(estimate_from_filename("IMG_20251301_143022.jpg"), None);
        assert_eq!(estimate_from_filename("IMG_20250199_143022.jpg"), None);
        assert_eq!(estimate_from_filename("IMG_20250101_259999.jpg"), None);
        assert_eq!(estimate_from_filename("IMG_2025010114302.jpg"), None); // 13 digits
        // A phone number is not a date.
        assert_eq!(estimate_from_filename("call_9876543210.jpg"), None);
    }

    #[test]
    fn unparseable_names_fall_back_to_mtime() {
        // Nikon D-series names carry no timestamp.
        assert_eq!(estimate_from_filename("DSC_1234.NEF"), None);
        assert_eq!(estimate_from_filename("_DSC5678.jpg"), None);
        // Canon "R0000123.JPG".
        assert_eq!(estimate_from_filename("R0000123.JPG"), None);
        // Unsplash random IDs.
        assert_eq!(estimate_from_filename("U6KmF4RpgiU.jpg"), None);

        let est = estimate_datetime("U6KmF4RpgiU.jpg", Some("2026-08-17T12:00:00Z"));
        assert_eq!(est.as_ref().map(|e| e.source), Some("mtime"));
        assert_eq!(est.as_ref().map(|e| e.datetime.as_str()), Some("2026-08-17T12:00:00Z"));

        // Even mtime unknown → nothing (and nothing fake is invented).
        assert_eq!(estimate_datetime("U6KmF4RpgiU.jpg", None), None);
    }
}