# Contact sheets (Sprint 14 — done 2026-08-18)

Print-ready sheets of selected photographs: thumbnails on a grid, one failed
photo per tile shows a placeholder, captions under each tile, pages named
`contact-sheet-{yyyy-mm-ddThh-mm-ss}-pNN.png` and written into a folder the
user picks.

## Limits & layout

- `MAX_SHEET_PHOTOS = 500` per export (validated in `sheet_photos`).
- Page: 2339 × 1654 px = A4 landscape at 200 dpi.
- 4 columns × 3 rows = 12 tiles per page; 56 px margin, 30 px gap, 110 px
  header (title), 70 px footer, 56 px caption under each tile.
- No extra crates: a 3×5 bitmap font (41 glyphs: `a-z 0-9 space - . : …`)
  is embedded in `src-tauri/src/contact_sheet.rs` for captions.

## Flow

1. `ExportSheetButton` (selection bar in the Library) → `pick_folder` →
   `export_contact_sheet` (Tauri command).
2. The command validates the destination (exists + writable probe), claims
   the export job slot in `AppState` (one export at a time), and spawns an
   async task: thumbnail fetch phase (`ThumbKind::Sheet`, width cap 800) then
   `spawn_blocking` → `render_sheets`.
3. `render_sheets` is pure (progress callback + `AtomicBool` cancel); the
   cancel flag is checked between pages and cancellation returns the pages
   already written (`SheetOutcome::Cancelled`).
4. Progress and completion arrive through `contact-sheet-progress` /
   `contact-sheet-complete` events; a wrong destination or a dead photo row
   is reported without touching files.

## Frontend

- `ExportSheetButton.tsx` (features/library): folder pick, busy state,
  notice on completion (or error/cancel).
- `ContactSheetCompletePayload` (`types/api.ts`) carries `files`, `error`,
  `cancelled`; `exportContactSheet`/`stopExport` live in `lib/ipc.ts`.
- The LibraryView selection bar hosts the button. Cancellation is exposed
  over IPC (`stop_export`) — no cancel UI yet, one export at a time.