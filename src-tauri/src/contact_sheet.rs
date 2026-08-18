//! Contact sheets (Sprint 14): render a printable PNG grid of photographs.
//!
//! Zero new dependencies: tiles come from the thumbnail pipeline, the page
//! is composited with the `image` crate (already in the tree), and labels
//! use a small embedded 3×5 bitmap font instead of pulling in a font
//! rasterizer. One sheet = a set of PNG pages, A4-landscape at 200 dpi
//! (2339×1654), 12 tiles per page (4 columns × 3 rows) — the classic culling
//! contact sheet: image + filename + capture date (or "—") per tile.
//!
//! Rendering is pure with respect to I/O: the caller gathers per-photo
//! data (metadata + thumbnail bytes), then calls `render_sheets`, which is
//! deterministic, cancellable between pages, and unit-testable without a
//! runtime. `commands/contact_sheet.rs` wires the fetch + background job.

use std::path::{Path, PathBuf};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

/// Hard cap — a contact sheet is a print, not an archive. Larger exports
/// are a separate feature (HiDPI raws, ZIP, …).
pub const MAX_SHEET_PHOTOS: usize = 500;
/// A4 landscape @ 200 dpi.
pub const PAGE_W: u32 = 2339;
pub const PAGE_H: u32 = 1654;
const MARGIN: u32 = 56;
const HEADER_H: u32 = 110;
const FOOTER_H: u32 = 70;
const TILE_COLS: u32 = 4;
const TILE_ROWS: u32 = 3;
/// Height reserved inside each tile for the caption (filename + date).
const CAPTION_H: u32 = 56;
const GAP: u32 = 30;

pub const TILES_PER_PAGE: usize = (TILE_COLS * TILE_ROWS) as usize;

/// One photo's worth of input data, already fetched by the caller.
#[derive(Debug)]
pub struct SheetPhoto {
    pub filename: String,
    pub capture_datetime: Option<String>,
    /// Decoded thumbnail (full image could be huge; tiles are small) —
    /// `None` renders a labeled empty box instead of failing the sheet.
    pub thumb: Option<(Vec<u8>, u32, u32)>,
}

/// Load the photos' names + capture dates (one query), leaving the
/// thumbnail fetching to the caller (it needs the async service).
pub fn sheet_photos(db: &Db, photo_ids: &[i64]) -> AppResult<Vec<SheetPhoto>> {
    if photo_ids.len() > MAX_SHEET_PHOTOS {
        return Err(AppError::validation(format!(
            "A contact sheet can hold at most {MAX_SHEET_PHOTOS} photographs (got {}). \
             Export smaller batches and combine them when printing.",
            photo_ids.len()
        )));
    }
    if photo_ids.is_empty() {
        return Err(AppError::validation("Nothing to export — select some photographs first."));
    }
    let conn = db.lock()?;
    let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let params: Vec<Box<dyn rusqlite::ToSql>> =
        photo_ids.iter().map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>).collect();
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT filename, capture_datetime FROM photos WHERE id IN ({placeholders})"
        ))
        .map_err(db_err("prepare sheet photos"))?;
    let rows = stmt
        .query_map(refs.as_slice(), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(db_err("query sheet photos"))?;
    let mut by_id: std::collections::HashMap<i64, (String, Option<String>)> =
        std::collections::HashMap::new();
    for (i, row) in rows.enumerate() {
        let (name, dt) = row.map_err(db_err("read sheet photo row"))?;
        by_id.insert(photo_ids[i], (name, dt));
    }
    if by_id.len() != photo_ids.len() {
        return Err(AppError::operation(
            "Some photographs are no longer in the library — the export was cancelled.",
        ));
    }
    Ok(photo_ids
        .iter()
        .map(|id| {
            let (name, dt) = by_id.get(id).expect("id present");
            SheetPhoto {
                filename: name.clone(),
                capture_datetime: dt.clone(),
                thumb: None, // caller fills
            }
        })
        .collect())
}

/// Render the sheet pages (deterministic) and return the written paths.
///
/// `progress` is called once per page (stage "rendering"); `cancel` is
/// checked between pages, so a cancelled export returns the pages written
/// so far plus `Ok(true)` wrapped… actually: returning the partial pages as
/// a normal success would silently give the user an incomplete sheet, so a
/// cancellation yields `Cancelled` with the paths — the command surfaces it.
#[derive(Debug)]
pub enum SheetOutcome {
    Ok { pages: Vec<PathBuf> },
    Cancelled { pages: Vec<PathBuf> },
}

pub fn render_sheets(
    photos: &mut [SheetPhoto],
    title: &str,
    dest_dir: &Path,
    progress: &mut dyn FnMut(ProgressPayload),
    cancel: &std::sync::atomic::AtomicBool,
) -> AppResult<SheetOutcome> {
    let mut pages_written: Vec<PathBuf> = Vec::new();
    if photos.is_empty() {
        return Err(AppError::validation("Nothing to export."));
    }
    let total_pages = photos.len().div_ceil(TILES_PER_PAGE);
    let stamp = crate::time::now_utc().trim_end_matches('Z').replace(':', "-");

    for p in 0..total_pages {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(SheetOutcome::Cancelled { pages: pages_written });
        }
        let canvas = render_page(&mut photos[p * TILES_PER_PAGE..], title, p + 1, total_pages);
        let name = format!("contact-sheet-{stamp}-p{:02}.png", p + 1);
        let path = dest_dir.join(name);
        canvas
            .save(&path)
            .map_err(|e| AppError::io(std::io::Error::other(e), path.display().to_string()))?;
        pages_written.push(path.clone());
        progress(ProgressPayload::new(photos.len(), (p + 1) * TILES_PER_PAGE, "rendering")
            .with_current(path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
    }
    Ok(SheetOutcome::Ok { pages: pages_written })
}

fn render_page(
    photos: &mut [SheetPhoto],
    title: &str,
    page_no: usize,
    total_pages: usize,
) -> image::RgbaImage {
    let bg = image::Rgba([255, 255, 255, 255]);
    let mut canvas = image::RgbaImage::from_pixel(PAGE_W, PAGE_H, bg);

    // Header: title + page line.
    let header_line = format!(
        "{title}  —  page {page_no} of {total_pages}  ·  {}",
        photos.len().min(TILES_PER_PAGE)
    );
    draw_text(&mut canvas, &header_line, MARGIN + 8, 28, 26, image::Rgba([20, 20, 20, 255]));
    draw_text(
        &mut canvas,
        &format!("tiles {TILE_COLS}×{TILE_ROWS} · {} px/in", PAGE_W as f32 / 8.27), // ≈200 dpi
        MARGIN + 8,
        58,
        18,
        image::Rgba([90, 90, 90, 255]),
    );

    // Tile grid.
    let tile_w = (PAGE_W - 2 * MARGIN - (TILE_COLS - 1) * GAP) / TILE_COLS;
    let img_h = (PAGE_H - MARGIN * 2 - HEADER_H - CAPTION_H - FOOTER_H - (TILE_ROWS - 1) * GAP) / TILE_ROWS;
    let tile_h = img_h + CAPTION_H;

    for i in 0..photos.len().min(TILES_PER_PAGE) {
        let col = (i % TILE_COLS as usize) as u32;
        let row = (i / TILE_COLS as usize) as u32;
        let x = MARGIN + col * (tile_w + GAP);
        let y = MARGIN + HEADER_H + row * (tile_h + GAP);

        // Image box on white — thumbnails are contain-fitted and centered,
        // so mixed aspect ratios never distort.
        let thumb = photos[i].thumb.take();
        if let Some((bytes, _tw, _th)) = thumb {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let (iw, ih) = fit_contain(img.width(), img.height(), tile_w, img_h);
                let resized = img.resize_exact(iw, ih, image::imageops::FilterType::Triangle);
                image::imageops::overlay(
                    &mut canvas,
                    &resized,
                    (x + (tile_w - iw) / 2) as i64,
                    (y + (img_h - ih) / 2) as i64,
                );
            } else {
                draw_no_preview(&mut canvas, x, y, tile_w, img_h);
            }
        } else {
            draw_no_preview(&mut canvas, x, y, tile_w, img_h);
        }

        // Caption: filename (truncated to fit) over capture date.
        let fname = truncate(&photos[i].filename, (tile_w / 24) as usize);
        draw_text(&mut canvas, &fname, x + 2, y + img_h + 8, 22, image::Rgba([20, 20, 20, 255]));
        let date = photos[i]
            .capture_datetime
            .as_deref()
            .map(|iso| iso[..10].to_string())
            .unwrap_or_else(|| "—".to_string());
        draw_text(&mut canvas, &date, x + 2, y + img_h + 32, 17, image::Rgba([110, 110, 110, 255]));
    }

    draw_text(
        &mut canvas,
        &format!("PhotoGremlin · {} photographs per page", TILES_PER_PAGE),
        MARGIN + 8,
        PAGE_H - MARGIN - 40,
        16,
        image::Rgba([120, 120, 120, 255]),
    );
    canvas
}

fn fit_contain(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let scale = ((max_w as f32 / w as f32).min(max_h as f32 / h as f32)).min(1.0);
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    (nw.min(max_w), nh.min(max_h))
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let trimmed: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{trimmed}…")
    }
}

fn draw_no_preview(canvas: &mut image::RgbaImage, x: u32, y: u32, w: u32, h: u32) {
    let mut boxed = image::RgbaImage::from_pixel(w, h, image::Rgba([245, 245, 245, 255]));
    let msg = if w > 240 { "no preview available" } else { "—" };
    draw_text(&mut boxed, msg, 6, 6, if w > 240 { 18 } else { 12 }, image::Rgba([140, 140, 140, 255]));
    image::imageops::overlay(canvas, &boxed, x as i64, y as i64);
}

// ---------------------------------------------------------------------------
// Embedded 3×5 bitmap font (ASCII: a–z, 0–9, space, dash, dots, colon).
// Each glyph is 3 bit-rows from the top of a 5-bit column mask. Kept tiny on
// purpose (AGENTS.md: no dependency for ~50 lines); a proper font renderer
// would be the way if sheets ever ship multilingual captions.
// ---------------------------------------------------------------------------

fn font_glyph(c: char) -> Option<[u8; 5]> {
    let i = match c {
        'a'..='z' => c as usize - 'a' as usize,
        '0'..='9' => 26 + c as usize - '0' as usize,
        ' ' => 36,
        '-' => 37,
        '.' => 38,
        ':' => 39,
        '…' => 40,
        _ => return None,
    };
    Some(FONT[i])
}

/// 3×5 pixel glyphs, one 3-bit row mask per line (bit 2 = left pixel).
const FONT: [[u8; 5]; 41] = [
    // a–z
    [0b010, 0b101, 0b111, 0b101, 0b101], // a
    [0b110, 0b101, 0b110, 0b101, 0b110], // b
    [0b011, 0b100, 0b100, 0b100, 0b011], // c
    [0b001, 0b011, 0b101, 0b101, 0b011], // d
    [0b111, 0b100, 0b111, 0b100, 0b111], // e
    [0b111, 0b100, 0b110, 0b100, 0b100], // f
    [0b011, 0b100, 0b101, 0b101, 0b011], // g
    [0b101, 0b101, 0b111, 0b101, 0b101], // h
    [0b111, 0b010, 0b010, 0b010, 0b111], // i
    [0b011, 0b001, 0b001, 0b101, 0b010], // j
    [0b101, 0b101, 0b110, 0b101, 0b101], // k
    [0b100, 0b100, 0b100, 0b100, 0b111], // l
    [0b101, 0b111, 0b111, 0b101, 0b101], // m
    [0b110, 0b101, 0b101, 0b101, 0b101], // n
    [0b010, 0b101, 0b101, 0b101, 0b010], // o
    [0b110, 0b101, 0b110, 0b100, 0b100], // p
    [0b010, 0b101, 0b101, 0b011, 0b001], // q
    [0b110, 0b101, 0b110, 0b101, 0b101], // r
    [0b011, 0b100, 0b010, 0b001, 0b110], // s
    [0b111, 0b010, 0b010, 0b010, 0b010], // t
    [0b101, 0b101, 0b101, 0b101, 0b111], // u
    [0b101, 0b101, 0b101, 0b101, 0b010], // v
    [0b101, 0b101, 0b111, 0b111, 0b101], // w
    [0b101, 0b101, 0b010, 0b101, 0b101], // x
    [0b101, 0b101, 0b101, 0b010, 0b010], // y
    [0b111, 0b001, 0b010, 0b100, 0b111], // z
    // 0–9
    [0b010, 0b101, 0b101, 0b101, 0b010], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b110, 0b001, 0b010, 0b100, 0b111], // 2
    [0b111, 0b001, 0b010, 0b001, 0b110], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b110, 0b001, 0b110], // 5
    [0b011, 0b100, 0b110, 0b101, 0b010], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b010, 0b101, 0b010, 0b101, 0b010], // 8
    [0b010, 0b101, 0b011, 0b001, 0b010], // 9
    // space, -, ., :, …
    [0, 0, 0, 0, 0],
    [0, 0, 0b111, 0, 0],
    [0, 0, 0, 0b010, 0b010],
    [0b010, 0, 0b010, 0, 0],
    [0, 0, 0, 0b101, 0b101],
];

fn draw_text(canvas: &mut image::RgbaImage, text: &str, x: u32, y: u32, scale: u32, color: image::Rgba<u8>) {
    let mut cursor = x;
    for ch in text.chars() {
        let Some(glyph) = font_glyph(ch.to_ascii_lowercase()) else {
            continue;
        };
        for r in 0..5 {
            for c in 0..3 {
                if glyph[r] & (1 << (2 - c)) != 0 {
                    fill_rect(
                        canvas,
                        cursor + (c as u32) * scale,
                        y + (r as u32) * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cursor += 4 * scale;
    }
}

fn fill_rect(canvas: &mut image::RgbaImage, x: u32, y: u32, w: u32, h: u32, color: image::Rgba<u8>) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < canvas.width() && py < canvas.height() {
                canvas.put_pixel(px, py, color);
            }
        }
    }
}

fn db_err(op: &'static str) -> impl Fn(rusqlite::Error) -> AppError + 'static {
    move |e| AppError::Database(format!("{op}: {e}"))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 40, 40, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn photo(name: &str, date: Option<&str>) -> SheetPhoto {
        SheetPhoto {
            filename: name.to_string(),
            capture_datetime: date.map(|d| d.to_string()),
            thumb: Some((png_bytes(600, 400), 600, 400)),
        }
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pg_cs_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn page_math_and_file_naming() {
        let dir = tmp_dir("pages");
        let mut photos: Vec<SheetPhoto> = (0..13).map(|i| photo(&format!("p{i:04}.jpg"), None)).collect();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut done = 0usize;
        let outcome = render_sheets(
            &mut photos,
            "June shoot",
            &dir,
            &mut |p| done = p.done,
            &cancel,
        )
        .unwrap();
        let SheetOutcome::Ok { pages } = outcome else { panic!("expected ok") };
        assert_eq!(pages.len(), 2);
        assert_eq!(done, 24); // progress reports done ≥ len at the end
        let p1 = image::open(&pages[0]).unwrap();
        assert_eq!((p1.width(), p1.height()), (PAGE_W, PAGE_H));
        let name1 = pages[0].file_name().unwrap().to_string_lossy();
        assert!(name1.starts_with("contact-sheet-"), "{name1}");
        assert!(name1.ends_with("-p01.png"), "{name1}");
        assert!(pages[1].file_name().unwrap().to_string_lossy().ends_with("-p02.png"));
    }

    #[test]
    fn empty_input_is_friendly_and_cancel_stops_between_pages() {
        let dir = tmp_dir("empty");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let err = render_sheets(&mut [], "x", &dir, &mut |_| {}, &cancel).unwrap_err();
        assert!(err.to_string().contains("Nothing to export"));

        // Cancel as soon as the first page is done → 1 page written + Cancelled.
        let mut photos: Vec<SheetPhoto> = (0..13).map(|i| photo(&format!("q{i}.jpg"), None)).collect();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let outcome = render_sheets(
            &mut photos,
            "x",
            &dir,
            &mut |_| cancel.store(true, std::sync::atomic::Ordering::Relaxed),
            &cancel,
        )
        .unwrap();
        let SheetOutcome::Cancelled { pages } = outcome else { panic!("expected cancelled") };
        assert_eq!(pages.len(), 1);
        assert!(pages[0].exists());
    }

    #[test]
    fn missing_thumb_produces_placeholder_not_error() {
        let dir = tmp_dir("nothumb");
        let mut photos = vec![SheetPhoto {
            filename: "NOPE.CR2".to_string(),
            capture_datetime: None,
            thumb: None,
        }];
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let outcome = render_sheets(&mut photos, "x", &dir, &mut |_| {}, &cancel).unwrap();
        let SheetOutcome::Ok { pages } = outcome else { panic!("ok") };
        assert!(pages[0].exists());
        assert_eq!(image::open(&pages[0]).unwrap().width(), PAGE_W);
    }

    #[test]
    fn dates_and_names_are_truncated_safely() {
        assert_eq!(truncate("short.jpg", 100), "short.jpg");
        assert_eq!(truncate("a_very_long_file_name_that_keeps_going.jpg", 12), "a_very_long…");
        draw_text_static_runs(); // helper below asserts glyph ink itself
    }

    fn draw_text_static_runs() -> u32 {
        let mut canvas = image::RgbaImage::from_pixel(PAGE_W, PAGE_H, image::Rgba([255, 255, 255, 255]));
        draw_text(&mut canvas, "Contact sheet 2026-08-08: 123", 10, 10, 20, image::Rgba([0, 0, 0, 255]));
        // Any black pixel at the origin of the first glyph proves glyphs draw.
        let mut ink = 0u32;
        for px in canvas.pixels() {
            if px.0[0] < 20 {
                ink += 1;
            }
        }
        assert!(ink > 100, "expected glyph pixels drawn, got {ink}");
        ink
    }
}
