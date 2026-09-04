# Beta testing guide

PhotoGremlin's beta is a local-first library, measurement, organization and
review application for working photographers. It makes no runtime network
requests and has no account, telemetry or remote-inference path. Test with a
copy or backed-up catalog first: normal browsing and review are
non-destructive, but confirmed file operations intentionally change files.

## Beta boundary

- Source photographs stay in their existing folders unless the photographer
  explicitly previews and confirms a rename, move, copy, trash, or permanent
  delete operation.
- Review decisions are reversible local catalog state. They do not rate,
  delete or edit a photograph.
- Editor handoff passes at most 500 normal source paths to the configured local
  executable. It does not modify Lightroom or Capture One catalogs, create
  sidecars or alter originals. Larger kept sets use **Export originals**.
- Sequence comparison is deliberately limited to four local thumbnails. It is
  decision context, not a pixel-level editing or automatic selection tool.
- Optional face detection is local, can be disabled, and is not required for
  scanning, measurements, filters, grouping, file operations or review.

## Photographer smoke pass

1. Start with a fresh app-data directory and add a small folder containing a
   realistic mix of JPEG and supported RAW files. Confirm indexing counts and
   thumbnail placeholders are understandable.
2. Run metadata, deterministic analysis and similarity grouping. Keep the UI
   interactive while work is active, cancel one pass, then resume it.
3. Repeat with a multi-thousand-photo project. Scroll deeply, filter, reopen
   the project and confirm the grid and review position resume without a large
   memory spike.
4. Exercise Black & white, Color, Dark, Bright, Landscape, Portrait, and
   Contains faces quick views. Confirm exclusive pairs replace each other while
   an unrelated ISO or review-state condition stays active. Then exercise
   brightness, sharpness, contrast, ISO and focal-length ranges with
   mouse and keyboard. The track should show a field-specific measured scale;
   exact values should appear only while the control is adjusted and remain
   available to assistive technology.
5. In Cull, open **Export** and **More** above selected photographs. Each menu
   must be opaque, aligned, evenly spaced and above every tile mark. Escape and
   an outside click should close it.
6. Review a shoot with `K`, `X`, `L`, `Backspace`, `U` and navigation keys.
   Leave and reopen Review to verify the focused moment is restored.
7. On a grouped moment, press `C`. Compare two through four neighboring frames,
   zoom and pan each pane, verify every pane stays synchronized, then focus a
   different frame back into Review. Escape must close only the dialog.
8. Finish a fully decided shoot. Verify the factual totals, kept Library view,
   review-again action, and configured local editor handoff. Also verify the
   over-500-file message directs the photographer to preview-first export.
9. Preview a rename, move, copy, trash, and permanent-delete operation. Test a
   destination collision, a source removed between preview and confirm, and
   each destructive confirmation. Permanent deletion must show the exact path,
   warn that recovery is impossible, and affect only the confirmed indexed
   file. No item may be silently overwritten or deleted from a filter/review
   decision alone.
10. Restart after each workflow. Confirm decisions, collections, saved views,
    settings and generated-thumbnail cache controls remain consistent.
11. Disconnect networking for the full pass. The workflow must remain fully
    usable because runtime networking is not part of the product.

## Platform matrix

The automated Rust, React/TypeScript, frontend and Linux Tauri bundle gates run
for this beta. The Linux release package is produced on this workstation.
Windows and macOS installers and their native picker, trash, application-launch
and file-manager behavior require the same manual smoke pass on those operating
systems before public distribution.

## Known beta limits

- No direct Lightroom or Capture One catalog mutation or sidecar interchange.
- No full-resolution compare view; Review comparison uses bounded thumbnails.
- No video, cloud sync, account, team workflow, telemetry or remote AI.
- No automatic aesthetic winner or quality verdict. Measurements and decisions
  remain factual and photographer-controlled.
- Installer signing and notarization are release-owner tasks per platform.

## Useful reports

Include the PhotoGremlin version, operating system, approximate project size,
file types, the smallest reproducible sequence and the relevant local log
excerpt. Do not attach client photographs unless you independently choose to;
the app never uploads them and useful issue reports normally need no pixels.
