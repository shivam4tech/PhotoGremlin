//! Local EXIF/metadata extraction (Sprint 5). Pure: file path in,
//! `ExifRecord` out. No Tauri, no database.
//!
//! Privacy contract (PRIVACY.md): GPS coordinates are NEVER stored — only a
//! presence bit (`gps_present`). A photograph's location stays in the file
//! and never enters the SQLite catalog.
//!
//! Date convention: EXIF datetimes carry no timezone. They are stored
//! verbatim as UTC RFC3339 (the recorded clock time), which keeps the
//! catalog lexicographically sortable (filters compare strings) without
//! silently assuming the user's timezone (DATABASE.md).

use std::io::BufReader;
use std::path::Path;

use exif::{Error as ExifError, In, Reader, Tag, Value};

use crate::error::{AppError, AppResult};

/// Everything the EXIF pass can contribute to a `photos` row. `None`
/// fields are left untouched by the upsert (COALESCE merge).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExifRecord {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Derived by the pass from the best-known (merged) dimensions.
    pub orientation: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    /// Lens manufacturer (often absent — the model string is the useful one).
    pub lens_make: Option<String>,
    /// Software that created/edited the file (e.g. "Adobe Lightroom").
    pub software: Option<String>,
    /// Millimetres.
    pub focal_length: Option<f64>,
    pub iso: Option<i64>,
    /// f-number, e.g. 2.8.
    pub aperture: Option<f64>,
    /// Exposure time in seconds, e.g. 1/250 = 0.004.
    pub shutter_speed: Option<f64>,
    /// UTC RFC3339 (EXIF clock time, no timezone by definition).
    pub capture_datetime: Option<String>,
    pub gps_present: bool,
}

impl ExifRecord {
    /// Whether this read contributed any camera/exposure/date value (the
    /// column-level truth behind `metadata_source` escalation in
    /// `upsert_exif`). Dimensions/orientation are technical geometry from
    /// the scanner and do not count.
    pub fn has_metadata(&self) -> bool {
        self.camera_make.is_some()
            || self.camera_model.is_some()
            || self.lens.is_some()
            || self.lens_make.is_some()
            || self.software.is_some()
            || self.focal_length.is_some()
            || self.iso.is_some()
            || self.aperture.is_some()
            || self.shutter_speed.is_some()
            || self.capture_datetime.is_some()
            || self.gps_present
    }
}

/// Read one file's EXIF. A file that is a readable image but carries no
/// EXIF segment yields an empty record (that is not an error). A file that
/// cannot be parsed at all is a friendly error.
pub fn extract_exif(path: &Path) -> AppResult<ExifRecord> {
    let file = std::fs::File::open(path)
        .map_err(|e| AppError::io(e, path.display().to_string()))?;
    match Reader::new().read_from_container(&mut BufReader::new(file)) {
        Ok(exif) => Ok(from_fields(&exif)),
        // No EXIF segment in an otherwise valid container: nothing to read.
        Err(ExifError::NotFound(_)) => Ok(ExifRecord::default()),
        Err(e) => Err(AppError::ImageRead {
            path: path.display().to_string(),
            reason: format!("could not read metadata: {e}"),
        }),
    }
}

pub(crate) fn from_fields(exif: &exif::Exif) -> ExifRecord {
    let mut rec = ExifRecord::default();
    let w = u32_field(exif, Tag::ImageWidth);
    let h = u32_field(exif, Tag::ImageLength);
    rec.width = w;
    rec.height = h;
    rec.camera_make = string_field(exif, Tag::Make);
    rec.camera_model = string_field(exif, Tag::Model);
    rec.lens = string_field(exif, Tag::LensModel);
    rec.lens_make = string_field(exif, Tag::LensMake);
    rec.software = string_field(exif, Tag::Software);
    // FocalLength is in 1/100 mm; FocalLengthIn35mmFilm is plain mm.
    rec.focal_length = rational_mm_100(exif, Tag::FocalLength)
        .map(|mm| mm)
        .or_else(|| u32_field(exif, Tag::FocalLengthIn35mmFilm).map(|mm| mm as f64));
    rec.iso = iso_field(exif);
    rec.aperture = rational_field(exif, Tag::FNumber);
    rec.shutter_speed = rational_field(exif, Tag::ExposureTime);
    rec.capture_datetime = string_field(exif, Tag::DateTimeOriginal)
        .or_else(|| string_field(exif, Tag::DateTime))
        .and_then(|s| parse_exif_datetime(&s));
    rec.gps_present = exif
        .get_field(Tag::GPSLatitude, In::PRIMARY)
        .is_some()
        || exif.get_field(Tag::GPSLatitudeRef, In::PRIMARY).is_some();
    rec
}

/// First number of a Short/Long/SShort/SLong value.
fn u32_field(exif: &exif::Exif, tag: Tag) -> Option<u32> {
    let v = exif.get_field(tag, In::PRIMARY)?;
    first_u32(&v.value)
}

fn first_u32(v: &Value) -> Option<u32> {
    match v {
        Value::Short(vec) => vec.first().copied().map(u32::from),
        Value::Long(vec) => vec.first().copied(),
        Value::SShort(vec) => vec.first().and_then(|x| u32::try_from(*x).ok()),
        Value::SLong(vec) => vec.first().and_then(|x| u32::try_from(*x).ok()),
        Value::Byte(vec) => vec.first().copied().map(u32::from),
        _ => None,
    }
}

fn rational_field(exif: &exif::Exif, tag: Tag) -> Option<f64> {
    let v = exif.get_field(tag, In::PRIMARY)?;
    if let Value::Rational(vec) = &v.value {
        vec.first().map(|r| r.to_f64())
    } else {
        None
    }
}

/// FocalLength tag is stored in 1/100 mm → convert to mm.
fn rational_mm_100(exif: &exif::Exif, tag: Tag) -> Option<f64> {
    rational_field(exif, tag).map(|v| v * 0.01)
}

fn iso_field(exif: &exif::Exif) -> Option<i64> {
    for tag in [Tag::ISOSpeed, Tag::RecommendedExposureIndex, Tag::PhotographicSensitivity] {
        if let Some(i) = u32_field(exif, tag) {
            if i > 0 {
                return Some(i as i64);
            }
        }
    }
    None
}

fn string_field(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let v = exif.get_field(tag, In::PRIMARY)?;
    let out = match &v.value {
        Value::Ascii(parts) => {
            let mut joined = Vec::new();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    joined.push(b' ');
                }
                joined.extend_from_slice(part);
            }
            joined
        }
        Value::Byte(bytes) => bytes.clone(),
        other => {
            // Numbers stored as strings are rare; fall back to display.
            let s = other.display_as(tag).to_string();
            if s.is_empty() {
                return None;
            }
            s.into_bytes()
        }
    };
    let s = String::from_utf8_lossy(&out).trim().to_string();
    // Blank values mean "unknown" per the EXIF spec.
    if s.is_empty() || s == "0000:00:00 00:00:00" {
        None
    } else {
        Some(s)
    }
}

/// EXIF stores "YYYY:MM:DD HH:MM:SS" with no zone. We store that clock time
/// verbatim as UTC RFC3339 (documented decision; keeps filters on plain
/// string comparison).
fn parse_exif_datetime(raw: &str) -> Option<String> {
    // EXIF allows optional fractional seconds: "YYYY:MM:DD HH:MM:SS[,f]".
    let raw = raw.trim();
    let (main, _frac) = raw.split_once(',').unwrap_or((raw, ""));
    let dt = chrono::NaiveDateTime::parse_from_str(main, "%Y:%m:%d %H:%M:%S").ok()?;
    Some(dt.and_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use exif::{experimental::Writer, Field, Rational, Value};

    /// Build EXIF bytes the way cameras carry them: "Exif\0\0" + TIFF.
    fn encode_exif_fields(fields: &[Field]) -> Vec<u8> {
        let mut writer = Writer::new();
        for f in fields {
            writer.push_field(f);
        }
        let mut tiff: Vec<u8> = Vec::new();
        writer
            .write(&mut std::io::Cursor::new(&mut tiff), true)
            .unwrap();
        let mut out = Vec::from("Exif\0\0".as_bytes());
        out.extend(tiff);
        out
    }

    fn f64r(num: u32, denom: u32) -> Value {
        Value::Rational(vec![Rational { num, denom }])
    }

    /// Encode a small solid-color JPEG (the pixel content is irrelevant to
    /// EXIF, but the container must be real so the reader accepts it).
    fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([120, 90, 60]));
        let mut jpeg: Vec<u8> = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        jpeg
    }

    /// JPEG (from the image crate) + an APP1 EXIF segment spliced after SOI.
    fn jpeg_with_exif(w: u32, h: u32, exif_bytes: &[u8]) -> Vec<u8> {
        let mut jpeg = plain_jpeg(w, h);
        assert!(jpeg.starts_with(&[0xFF, 0xD8]), "expected JPEG SOI");
        // APP1: FFE1, 16-bit length (includes the 2 length bytes), payload.
        let mut out = jpeg.split_off(2);
        let len = (exif_bytes.len() + 2) as u16;
        let mut head = vec![0xFFu8, 0xE1, (len >> 8) as u8, (len & 0xff) as u8];
        head.extend_from_slice(exif_bytes);
        let mut result = vec![0xFFu8, 0xD8u8];
        result.extend_from_slice(&head);
        result.append(&mut out);
        result
    }

    fn write_tmp(label: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("pg_exif_{label}_{}.jpg", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn file_without_exif_yields_empty_record() {
        let v = plain_jpeg(16, 16);
        let p = write_tmp("noexif", &v);
        let rec = extract_exif(&p).unwrap();
        assert_eq!(rec, ExifRecord::default());
        assert!(!rec.has_metadata());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn extracts_camera_exposure_and_datetime() {
        let fields = vec![
            Field {
                tag: Tag::Make,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"GremCam".to_vec()]),
            },
            Field {
                tag: Tag::Model,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"Gr-33".to_vec()]),
            },
            Field {
                tag: Tag::LensModel,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"Gremlin 50mm f/1.4".to_vec()]),
            },
            Field {
                tag: Tag::LensMake,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"GremOptics".to_vec()]),
            },
            Field {
                tag: Tag::Software,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"PhotoGremlin Lab 1.0".to_vec()]),
            },
            Field { tag: Tag::FNumber, ifd_num: In::PRIMARY, value: f64r(14, 5) },
            Field { tag: Tag::ExposureTime, ifd_num: In::PRIMARY, value: f64r(1, 250) },
            Field {
                tag: Tag::ISOSpeed,
                ifd_num: In::PRIMARY,
                value: Value::Short(vec![400]),
            },
            Field { tag: Tag::FocalLength, ifd_num: In::PRIMARY, value: f64r(5000, 1) },
            Field {
                tag: Tag::DateTimeOriginal,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"2026:08:15 14:30:22".to_vec()]),
            },
            Field {
                tag: Tag::ImageWidth,
                ifd_num: In::PRIMARY,
                value: Value::Short(vec![6400]),
            },
            Field {
                tag: Tag::ImageLength,
                ifd_num: In::PRIMARY,
                value: Value::Short(vec![4266]),
            },
            // GPS: present in the file; only the presence may reach the DB.
            Field { tag: Tag::GPSLatitudeRef, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"N".to_vec()]) },
            Field { tag: Tag::GPSLatitude, ifd_num: In::PRIMARY, value: f64r(52, 1) },
        ];
        let exif = encode_exif_fields(&fields);
        let p = write_tmp("full", &jpeg_with_exif(32, 32, &exif));
        let rec = extract_exif(&p).unwrap();
        assert_eq!(rec.camera_make.as_deref(), Some("GremCam"));
        assert_eq!(rec.camera_model.as_deref(), Some("Gr-33"));
        assert_eq!(rec.lens.as_deref(), Some("Gremlin 50mm f/1.4"));
        assert_eq!(rec.lens_make.as_deref(), Some("GremOptics"));
        assert_eq!(rec.software.as_deref(), Some("PhotoGremlin Lab 1.0"));
        assert!(rec.has_metadata());
        assert!((rec.aperture.unwrap() - 2.8).abs() < 1e-9);
        assert!((rec.shutter_speed.unwrap() - 1.0 / 250.0).abs() < 1e-9);
        assert_eq!(rec.iso, Some(400));
        assert!((rec.focal_length.unwrap() - 50.0).abs() < 1e-9);
        assert_eq!(
            rec.capture_datetime.as_deref(),
            Some("2026-08-15T14:30:22Z")
        );
        assert_eq!((rec.width, rec.height), (Some(6400), Some(4266)));
        assert!(rec.gps_present);
        // The record struct has no coordinate fields at all — presence only.
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn blank_values_are_treated_as_absent() {
        let fields = vec![
            Field {
                tag: Tag::Make,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![Vec::new()]),
            },
            Field {
                tag: Tag::DateTimeOriginal,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![b"0000:00:00 00:00:00".to_vec()]),
            },
        ];
        let exif = encode_exif_fields(&fields);
        let p = write_tmp("blank", &jpeg_with_exif(32, 32, &exif));
        let rec = extract_exif(&p).unwrap();
        assert_eq!(rec.camera_make, None);
        assert_eq!(rec.capture_datetime, None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn exif_datetime_parser_handles_formats() {
        assert_eq!(
            parse_exif_datetime("2026:08:15 14:30:22"),
            Some("2026-08-15T14:30:22Z".to_string())
        );
        assert_eq!(
            parse_exif_datetime("2026:08:15 14:30:22,12"),
            Some("2026-08-15T14:30:22Z".to_string())
        );
        assert_eq!(parse_exif_datetime("garbage"), None);
    }
}
