# A photo-first library with expressive color filtering

Written against: a0a310c

## Evidence chain

- Surface: `Library` with the filter inspector open and analyzed portrait photographs visible
- Problem: the installed application does not show the current color explorer at all; technical capture-check badges obscure faces; opening measured filters produces eight permanently expanded slider panels; and the advanced-condition selects read as generic browser controls rather than a coherent editing tool.
- Design evidence: `src/features/library/FilterBar.tsx` already places `ColorSpectrumFilter` first; `src/features/library/ColorSpectrumFilter.tsx` already supports a deterministic 12-hue, match-any multi-selection; `src/components/PhotoTile.tsx` unconditionally renders `tile-capture-checks`; `src/features/library/QuickFilterControls.tsx` unconditionally expands every measured range after its parent disclosure opens; `docs/UI_GUIDELINES.md` defines photo-first tiles, progressive disclosure, transform/opacity motion, and a persistent spectrum as the primary visual filter.
- Owner: `src/features/library/FilterBar.tsx` and the library inspector composition
- Scope and affected surfaces: library photo grid, library filter inspector, saved-filter condition editor, existing local color-signature workflow, packaged desktop application
- Uncertainty: visual density and compact inspector behavior require validation at the application's minimum supported width and with both analyzed and unanalyzed collections.

## Design decision

Make the color explorer the calm visual anchor of the filter inspector, remove analysis diagnostics from thumbnail imagery, and turn measured filtering into progressive disclosure: the parent section reveals compact measurement rows, and an individual row reveals its range controls. Keep native form semantics but give advanced selects a labeled, consistent shell. Use restrained 140–180 ms opacity/transform transitions and preserve reduced-motion behavior. Rebuild and reinstall the current package while retaining the existing database and thumbnail cache, because the installed binary predates the color explorer and cleaning user data cannot fix that deployment mismatch.

## Reuse

- Existing `ColorSpectrumFilter`, `QuickFilterControls`, `FilterDisclosure`, `FilterSection`, button/input tokens, focus-visible treatment, and `--ease-out` motion token
- Exemplar: `src/features/library/ColorSpectrumFilter.tsx` for accessible multi-select color semantics and `src/features/library/FilterBar.tsx` for inspector ordering

No new runtime dependency or parallel design system is required. The native `select` remains the accessible control; CSS adds a shared visual wrapper and chevron.

## Changes

1. `src/components/PhotoTile.tsx`
   - Change: remove the unconditional sharpness, clipping, blink, and closed-eye analysis overlay from library thumbnails.
   - Preserve: selection, rating/flag/color-label marks, hover/focus actions, filename and dimension metadata, keyboard interaction, and all underlying measurements.
   - Verify: no analysis badge covers a photograph in the normal grid, while measurements remain usable in filters and photo details.

2. `src/features/library/ColorSpectrumFilter.tsx`
   - Change: strengthen the spectrum's hierarchy with a concise heading, selection count/status, explicit match-any explanation, accessible selected-color controls, and a compact clear action that appears only when useful.
   - Preserve: the existing 12-bin local HSV signature, multi-selection behavior, hue names, keyboard semantics, and unanalyzed-state explanation.
   - Verify: selecting multiple hues keeps each selected state legible and visibly narrows the grid without implying remote analysis.

3. `src/features/library/QuickFilterControls.tsx`
   - Change: render measured characteristics as compact disclosure rows showing their label, current state, and availability; expand only the chosen row to show its dual range control and missing-value/reset actions. Initialize active ranges open and keep inactive ranges collapsed.
   - Preserve: all eight measurable fields, numeric bounds, unmeasured-photo handling, range normalization, and existing filter-state APIs.
   - Verify: opening Measured filters no longer fills the inspector with sliders, active values remain discoverable, and every range is operable with mouse and keyboard.

4. `src/features/library/FilterBar.tsx`
   - Change: refine section copy and add labeled visual shells around the advanced condition field/operator/value selects, including a consistent chevron and compact active-condition summary.
   - Preserve: native select semantics, all filter fields/operators, condition grouping, save-view behavior, and existing state ownership.
   - Verify: advanced conditions scan as one intentional composer rather than three unrelated browser dropdowns, with no custom-menu accessibility regression.

5. `src/styles/theme.css`
   - Change: implement the compact spectrum, disclosure-row, range-panel, and select-shell styling; remove capture-check overlay rules; add state transitions using only opacity and transform at 140–180 ms with the established easing; extend reduced-motion overrides to every new moving element.
   - Preserve: PhotoGremlin's dark neutral palette, density, visible focus, minimum pointer targets, functional spectrum gradient, responsive inspector, and high-contrast legibility.
   - Verify: no `transition: all`, decorative gradient, layout animation, clipped focus ring, or hover-only control is introduced.

6. `docs/UI_GUIDELINES.md`
   - Change: record that diagnostic analysis values never overlay normal library thumbnails, measured ranges use nested progressive disclosure, native condition selects share one styled shell, and color remains the inspector's primary visual control.
   - Preserve: current photo-first, local-only, accessibility, and motion principles.
   - Verify: documentation describes the shipped behavior rather than an aspirational redesign.

## Scope

- Inherit: library folders, review views, saved views, collection-backed library grids, and every route that renders `PhotoTile` or the shared filter inspector.
- Verify: selected-photo marks, empty results, no-analysis state, one and many active color selections, active measured ranges, long localized filenames, minimum window width, keyboard focus, and reduced motion.
- Exclude: color-signature algorithm/schema changes, viewer-detail redesign, automatic analysis, photo-file operations, database deletion, thumbnail-cache deletion, custom select/menu primitives, and unrelated application navigation.

## Validation

- Product: open an existing analyzed collection, select two separated hues, confirm match-any filtering, clear the colors, enable a measured range, save/reopen the filter state, and confirm no diagnostic badge is rendered on any thumbnail.
- Interface: inspect default, hover, focus-visible, selected, disabled, active-filter, expanded/collapsed, empty-results, unanalyzed, narrow-window, and reduced-motion states in the library route.
- System: confirm all styling uses existing tokens/components, native selects remain intact, filtering still flows through current Zustand state and typed IPC, and no dependency or runtime network access is added.
- Repository: `npm run test:rust` → Rust suite passes; `npm test` → frontend suite passes; `npm run typecheck` → no TypeScript errors; `npm run build` → production frontend builds; `source /home/shivam/pg-env.sh && PHOTOGREMLIN_ALLOW_LOW_MEMORY=1 npm run build:app` → debug desktop bundle is produced.

## Stop conditions

- Stop if color-filter state is not owned by the existing library filter store, removing the thumbnail badges would remove the only accessible location for a measurement, nested disclosures cannot preserve native keyboard behavior, or implementation requires a new runtime dependency, IPC/schema change, network access, or deletion of user data.

## Design documentation

- After acceptance and validation: update `docs/UI_GUIDELINES.md` with the shipped thumbnail, inspector hierarchy, disclosure, select, and motion contracts; update `docs/FILTER_ENGINE.md` only if color or measured-filter behavior changes beyond presentation.
