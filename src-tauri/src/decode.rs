//! RAW decode provider (Sprint 15): previews for camera raw files via
//! `rawler` (pure Rust, content-sniffed — the format is discovered from the
//! bytes, not the extension).
//!
//! Contract: `decode_to_preview` returns `Ok(Some(rgb))` when the file
//! decoded, `Ok(None)` when rawler reports it as undecodable (corrupt,
//! exotic, or an unsupported camera) — the thumbnailer then falls back to
//! the existing placeholder tile, never a crash — and `Err` only for
//! situations we must not hide (e.g. refusing a gigantic sensor).
//!
//! Deps note: `rawler` is LGPL-2.1; see docs/RAW_PREVIEWS.md for the
//! compliance note.

use std::path::Path;

use image::{imageops::FilterType, DynamicImage, RgbImage};
use rawler::{decode_file, decoders::Orientation, imgop::develop::RawDevelop};

use crate::error::{AppError, AppResult};

/// One raw decode at a time is dialed to full-sensor resolution; transient
/// u16 RGB ≈ 6 bytes/pixel, so 30 MP peaks around 180 MB before the preview
/// downscale. Refuse anything above that with a friendly message instead of
/// a swap storm (the JPEG path keeps its own 500 MP guard).
const RAW_MAX_PIXELS: u64 = 30_000_000;

/// Decode `path` to an RGB preview at most `max_width` px wide.
pub fn decode_to_preview(path: &Path, max_width: u32) -> AppResult<Option<RgbImage>> {
    let raw = match decode_file(path) {
        Ok(raw) => raw,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "raw decode rejected the file");
            return Ok(None);
        }
    };

    if (raw.width as u64) * (raw.height as u64) > RAW_MAX_PIXELS {
        return Err(AppError::operation(
            "This raw file is too large to preview safely on this computer.",
        ));
    }

    let dynimg = RawDevelop::default()
        .develop_intermediate(&raw)
        .map_err(|e| AppError::operation(format!("Could not develop raw image: {e}")))
        .and_then(|inter| {
            inter
                .to_dynamic_image()
                .ok_or_else(|| AppError::operation("Raw develop produced no image data."))
        })?;

    let oriented = apply_exif_orientation(dynimg, raw.orientation);

    let (w, h) = (oriented.width(), oriented.height());
    let scale = f64::from(max_width) / f64::from(w.max(1));
    let (tw, th) = if scale < 1.0 {
        (
            u32::max(1, (f64::from(w) * scale) as u32),
            u32::max(1, (f64::from(h) * scale) as u32),
        )
    } else {
        (w, h)
    };

    Ok(Some(
        oriented
            .resize_exact(tw, th, FilterType::Triangle)
            .to_rgb8(),
    ))
}

/// EXIF orientation → actual pixels. The develop pipeline crops and
/// color-manages but never rotates, so the thumbnailer's JPEG path would
/// show portrait raws sideways without this.
fn apply_exif_orientation(img: DynamicImage, o: Orientation) -> DynamicImage {
    match o {
        // 1 (normal) and files that didn't record one: use as-is.
        Orientation::Normal | Orientation::Unknown => img,
        // 2: mirrored over the vertical axis.
        Orientation::HorizontalFlip => img.fliph(),
        // 3: rotated 180°.
        Orientation::Rotate180 => img.rotate180(),
        // 4: mirrored over the horizontal axis.
        Orientation::VerticalFlip => img.flipv(),
        // 5: mirrored over the top-left → bottom-right diagonal
        // (scanner output; image 0.25 dropped `transpose`, so remap by hand).
        Orientation::Transpose => transpose_to_rgb(img),
        // 6: rotated 90° clockwise.
        Orientation::Rotate90 => img.rotate90(),
        // 7: mirrored over the top-right → bottom-left diagonal:
        // transpose, then rotate 180°.
        Orientation::Transverse => transpose_to_rgb(img).rotate180(),
        // 8: rotated 270° clockwise.
        Orientation::Rotate270 => img.rotate270(),
    }
}

/// Mirror over the top-left → bottom-right diagonal (anti-diagonal flip).
fn transpose_to_rgb(img: DynamicImage) -> DynamicImage {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let t = RgbImage::from_fn(h, w, |x, y| *rgb.get_pixel(y, x));
    DynamicImage::ImageRgb8(t)
}

#[cfg(test)]
mod tests {
    use super::*;


    /// Marker-pixel ASCII art: `a` at top-left, `b` top-right, `c` at
    /// bottom-left, `d` bottom-right. Rotations/flips must move them to the
    /// obvious corners.
    fn marker_image(w: u32, h: u32) -> DynamicImage {
        let mut img = DynamicImage::new_luma8(w, h).to_rgb8();
        img.put_pixel(0, 0, image::Rgb([1, 0, 0]));
        img.put_pixel(w - 1, 0, image::Rgb([0, 1, 0]));
        img.put_pixel(0, h - 1, image::Rgb([0, 0, 1]));
        img.put_pixel(w - 1, h - 1, image::Rgb([2, 2, 2]));
        DynamicImage::ImageRgb8(img)
    }

    fn corner(img: &RgbImage, x: u32, y: u32) -> [u8; 3] {
        img.get_pixel(x, y).0
    }

    #[test]
    fn orientation_mapping_moves_corners_correctly() {
        // Non-square on purpose: a 90° rotation must swap width/height.
        let base = marker_image(4, 3);

        let normal = apply_exif_orientation(base.clone(), Orientation::Normal);
        assert_eq!(corner(&normal.to_rgb8(), 0, 0), [1, 0, 0]);

        let rot90 = apply_exif_orientation(base.clone(), Orientation::Rotate90);
        assert_eq!(rot90.width(), 3);
        assert_eq!(rot90.height(), 4);
        assert_eq!(corner(&rot90.to_rgb8(), 3 - 1, 0), [1, 0, 0]); // TL → TR

        let rot270 = apply_exif_orientation(base.clone(), Orientation::Rotate270);
        assert_eq!(corner(&rot270.to_rgb8(), 0, 4 - 1), [1, 0, 0]); // TL → BL

        let rot180 = apply_exif_orientation(base.clone(), Orientation::Rotate180);
        assert_eq!(corner(&rot180.to_rgb8(), 3, 2), [1, 0, 0]); // TL → BR

        let flip_h = apply_exif_orientation(base.clone(), Orientation::HorizontalFlip);
        assert_eq!(corner(&flip_h.to_rgb8(), 3, 0), [1, 0, 0]); // TL → TR

        let flip_v = apply_exif_orientation(base, Orientation::VerticalFlip);
        assert_eq!(corner(&flip_v.to_rgb8(), 0, 2), [1, 0, 0]); // TL → BL
    }

    #[test]
    fn garbage_raw_file_falls_back_gracefully() {
        let dir = std::env::temp_dir().join(format!("pg_rawfall_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("IMG_0001.CR2");
        std::fs::write(&path, b"this is definitely not a camera raw file").unwrap();
        assert!(matches!(decode_to_preview(&path, 256).unwrap(), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Minimal *valid* uncompressed 16-bit DNG (4×4 RGGB) assembled by hand —
    /// no real-file fixtures in the repo (AGENTS.md rule 16). Proves the
    /// provider produces real pixels, not just silence.
    #[test]
    fn synthetic_dng_decodes_to_real_pixels() {
        let dir = std::env::temp_dir().join(format!("pg_dng_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("IMG_0001.DNG");
        std::fs::write(&path, synthetic_dng_bytes()).unwrap();

        let img = decode_to_preview(&path, 256)
            .unwrap()
            .expect("synthetic DNG must decode");
        assert_eq!((img.width(), img.height()), (16, 16));
        let any_nonblack = img
            .as_raw()
            .chunks_exact(3)
            .any(|px| px != [0, 0, 0]);
        assert!(any_nonblack, "develop must produce non-black pixels");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_dng_shares_writer_with_integration_tests() {
        let bytes = synthetic_dng_bytes();
        assert!(bytes.windows(4).any(|w| w == b"II\x2a\x00"));
    }
}

/// Minimal *valid* uncompressed 16-bit DNG (4×4→16×16 RGGB) assembled by
/// hand — no real-file fixtures in the repo (AGENTS.md rule 16). Public so
/// library integration tests can reuse the exact same bytes via
/// `photogremlin_lib::decode::synthetic_dng_bytes`.
#[doc(hidden)]
pub fn synthetic_dng_bytes() -> Vec<u8> {
        use std::io::Write;

      type Entry = (u16, u16, u32, Vec<u8>); // tag, type, count, payload
      let mut entries: Vec<Entry> = Vec::new();

      let push = |entries: &mut Vec<Entry>, tag: u16, ty: u16, count: u32, data: Vec<u8>| {
          entries.push((tag, ty, count, data));
      };
      let short = |v: u16| v.to_le_bytes().to_vec();
      let long = |v: u32| v.to_le_bytes().to_vec();
      let bytes_arr = |v: &[u8]| v.to_vec();
      let shorts = |v: &[u16]| {
          v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>()
      };

      // Core TIFF fields.
      push(&mut entries, 256, 4, 1, long(16)); // ImageWidth
      push(&mut entries, 257, 4, 1, long(16)); // ImageLength
      push(&mut entries, 258, 3, 1, short(16)); // BitsPerSample
      push(&mut entries, 259, 3, 1, short(1)); // Compression = uncompressed
      push(&mut entries, 262, 3, 1, short(32803)); // Photometric = CFA
      push(&mut entries, 277, 3, 1, short(1)); // SamplesPerPixel
      push(&mut entries, 284, 3, 1, short(1)); // PlanarConfiguration
      let strip_offsets_entry = entries.len() as u32;
      push(&mut entries, 273, 4, 1, long(0)); // StripOffsets (patched below)
      push(&mut entries, 278, 4, 1, long(16)); // RowsPerStrip
      push(&mut entries, 279, 4, 1, long(512)); // StripByteCounts = 256 × u16

      // DNG fields.
      push(&mut entries, 33421, 3, 2, shorts(&[2, 2])); // CFARepeatPatternDim
      push(&mut entries, 33422, 1, 4, bytes_arr(&[0, 1, 1, 2])); // CFAPattern RGGB
      push(&mut entries, 50706, 1, 4, bytes_arr(&[1, 4, 0, 0])); // DNGVersion
      push(&mut entries, 50710, 1, 3, bytes_arr(&[0, 1, 2])); // CFAPlaneColor
      push(&mut entries, 50711, 3, 1, short(1)); // CFALayout
      push(&mut entries, 50717, 3, 1, short(65535)); // WhiteLevel
      push(&mut entries, 50718, 3, 2, shorts(&[0, 0])); // DefaultCropOrigin
      push(&mut entries, 50719, 3, 2, shorts(&[16, 16])); // DefaultCropSize
      // Identity camera→XYZ matrix, SRATIONAL (num, den).
      let matrix: Vec<u8> = [1i32, 0, 0, 0, 1, 0, 0, 0, 1]
          .iter()
          .flat_map(|n| [n.to_le_bytes().to_vec(), 1i32.to_le_bytes().to_vec()])
          .flatten()
          .collect();
      push(&mut entries, 50721, 10, 9, matrix); // ColorMatrix1
      let neutral: Vec<u8> = [1i32, 1, 1]
          .iter()
          .flat_map(|n| [n.to_le_bytes().to_vec(), 1i32.to_le_bytes().to_vec()])
          .flatten()
          .collect();
      push(&mut entries, 50728, 5, 3, neutral); // AsShotNeutral

      let n = entries.len() as u32;
      let aux_start: u32 = 14 + 12 * n; // IFD start + count + entries + next-IFD

      // Decide inline vs auxiliary placement.
      let mut aux = Vec::<u8>::new();
      let mut entry_offsets: Vec<u32> = Vec::with_capacity(entries.len());
      for (_, _, _, payload) in &entries {
          if payload.len() <= 4 {
              entry_offsets.push(0); // inline
          } else {
              entry_offsets.push(aux_start + aux.len() as u32);
              aux.extend_from_slice(payload);
          }
      }

      let pixel_offset = aux_start + aux.len() as u32;
      let mut out = Vec::<u8>::new();
      out.write_all(b"II").unwrap();
      out.write_all(&42u16.to_le_bytes()).unwrap();
      out.write_all(&8u32.to_le_bytes()).unwrap();
      out.write_all(&(n as u16).to_le_bytes()).unwrap();
      for (i, (tag, ty, count, payload)) in entries.iter().enumerate() {
          out.write_all(&tag.to_le_bytes()).unwrap();
          out.write_all(&ty.to_le_bytes()).unwrap();
          out.write_all(&count.to_le_bytes()).unwrap();
          if i as u32 == strip_offsets_entry {
              out.write_all(&pixel_offset.to_le_bytes()).unwrap();
          } else if payload.len() <= 4 {
              out.extend_from_slice(payload);
              out.extend(std::iter::repeat(0u8).take(4 - payload.len()));
          } else {
              out.write_all(&entry_offsets[i].to_le_bytes()).unwrap();
          }
      }
      out.write_all(&0u32.to_le_bytes()).unwrap(); // next IFD
      out.extend_from_slice(&aux);
      // Pixels: RGGB, all midrange.
      // RGGB rows: even = R G R G…, odd = G B G B…. Channel-distinct
      // values so any demosaic/calibration mixing is visible in dims.
      for y in 0..16u32 {
          for x in 0..16u32 {
              let val: u16 = if y % 2 == 0 {
                  if x % 2 == 0 { 10_000 } else { 9_000 }
              } else if x % 2 == 0 {
                  9_000
              } else {
                  8_000
              };
              out.write_all(&val.to_le_bytes()).unwrap();
          }
      }
      out
  }
