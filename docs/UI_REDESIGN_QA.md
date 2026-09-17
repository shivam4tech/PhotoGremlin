# Library and filter redesign — progress and resume guide

_Last updated: September 13, 2026_

This is the canonical completion record for the Library/filter redesign. It records what is implemented on the current feature branch, the acceptance evidence, and the one environment-limited visual check that remains for owner sign-off.

## Repository checkpoint

- Branch: `feat/library-filter-redesign`
- Base/checkpoint: `8194e47` (`develop`) — `chore: checkpoint before library and filter redesign`
- Completed implementation commits:
  - `d62375eb534727d876e349d411c48ceedc94b145` — `feat: redesign library and filter control surface`
  - `41054bbeaeea54bdd088e8108a02a22cc3e2a34b` — `feat: add staged advanced filter workspace`
  - `37a7bfa0bc145305831dba8317c6f274c20e69a8` — `refactor(filters): simplify core filter interactions`
  - `9ea35636d1716c22404060ff22e4a6a2da76f800` — `feat(viewer): add rendered preview histograms`
  - `ce1478c7078948599770138ccee404a931b14154` — `feat: add advanced filter drawer state`
  - `9ca34bd916dce7f57e9085dc2e0547ac300b2566` — `feat(filters): streamline advanced filtering workflow`
- The branch has not been merged into `develop`.
- Phase 1 and Phase 2 implementation are complete. The acceptance-hardening changes described below are the final branch-local follow-up.

Status notation:
- `[x]` implemented
- `[~]` implemented but still needs refinement or has a known limitation
- `[ ]` not implemented

## Product and design target

PhotoGremlin is a local-only desktop work tool. The gallery stays visually dominant; ordinary filters behave like a compact inspector; Advanced behaves like a deeper right-side workspace without removing library context. Filtering should feel like shaping the collection, not building a database query.

Preserve existing Rust filtering semantics, typed IPC, saved views, local analysis, virtualization, project persistence, and the no-network architecture. Improve the presentation/state adapter rather than rewriting working filter computation.

## Work completed

### Library shell and visual foundation (`d62375e`)

- [x] Reworked the Library into compact left navigation, dominant center gallery, and right filter inspector.
- [x] Added/consolidated semantic dark/light theme tokens for surfaces, text, borders, accent, focus, and interaction states.
- [x] Tightened desktop spacing, toolbar hierarchy, navigation selection, photo-card treatment, and gallery density.
- [x] Kept the existing virtualized gallery and backend/data flow.
- [x] Added compact color swatches, quick filters, rating controls, active-filter chips, filter search/discovery, and measured-filter rows.
- [x] Added keyboard/focus states and reduced-motion handling to the new control surface.
- [x] Preserved applied filter semantics, saved-view compatibility, analysis values, and IPC behavior.
- [x] Added tests for discovery, chips, colors, clear-all, picker keyboard behavior, numeric commits, saved-view equivalence, stale responses, and theme contrast.

### Staged Advanced workflow foundation (`41054bb`)

- [x] Advanced edits a clone of applied filters instead of mutating the Library immediately.
- [x] `Apply filters` commits the full draft and closes the workspace.
- [x] Cancel, Escape, and close discard the draft.
- [x] Clear All is staged while Advanced is open.
- [x] Added category navigation, quick-filter controls, searchable filter selection, measured controls, color selection, and a draft summary.
- [x] Added tests for draft isolation, category switching, Apply, Cancel, Escape, and staged Clear All.
- [~] This foundation works, but its large three-column modal and generic condition editor are explicitly temporary and must be replaced in Phase 2.

### Numeric-control foundation

- [x] `RangeFilterRow` is reusable across measured fields.
- [x] Dual-thumb ranges display inactive and selected track portions.
- [x] Exact compact min/max inputs are available only while expanded.
- [x] Dragging can show transient value feedback.
- [x] Reset and unmeasured/coverage states are represented.
- [x] Zero-recorded fields can suppress or disable meaningless slider interaction.
- [x] Measured rows use independent disclosure state, so several editors can remain expanded together.

## Current implementation map

- `src/features/library/FilterBar.tsx`
  - Simple inspector composition, rating/color/quick filters, measured rows, and the registry-derived typed condition composer.
  - Uses presentation labels and values while preserving the canonical filter-engine condition shape.
  - Advanced candidate insertion is routed through duplicate-safe draft upserts; the simple inspector retains its live append behavior.
- `src/features/library/QuickFilterControls.tsx`
  - Quick-filter controls and `RangeFilterRow`.
  - Measured rows open and close independently while the controls remain mounted.
- `src/features/library/AdvancedFiltersDialog.tsx`
  - Responsive right-side drawer with staged Apply/Cancel behavior.
  - The draft summary and color controls are inline with the editor instead of occupying a persistent third column.
  - The footer reports an exact, debounced draft match count without publishing the draft to the Library grid.
  - The complete drawer workflow has automated interaction coverage and installed-app structural acceptance.
- `src/features/library/advancedFilterState.ts`
  - Explicit applied/draft/current-editor workspace state and duplicate-safe draft updates.
  - The Advanced picker, add editor, staged edits, removals, and commit/reset transitions are routed through this model.
- `src/features/library/ActiveFilterList.tsx`
  - Removable applied chips in Simple and separate edit/remove actions for staged Advanced chips, including per-color chips.
- `src/features/library/FilterPicker.tsx`
  - Searchable keyboard-accessible filter picker.
  - Active entries are labelled Added; choosing one in Advanced opens its existing staged condition for editing.
- `src/features/library/filterDiscovery.ts`
  - Substring/alias discovery (for example sharp, mono, ISO, face), categorized through the presentation adapter.
- `src/features/library/filterFields.ts`
  - Canonical filter registry plus the derived UI presentation adapter for control types, natural operator labels, values, bounds, units, defaults, unmeasured support, and icons.
  - Applied and preview queries share one session-aware filter serializer, including the empty active-folder sentinel behavior.
- `src/stores/filterStore.ts`
  - Applied Library filter state. Keep it as the source of truth outside Advanced.
- `src/features/filters/filterEngine.ts` and existing Rust/IPC code
  - Working filter semantics. Do not rewrite for presentation changes.
- Filter-specific styling is currently distributed through the existing component/style files introduced by the redesign. Continue using semantic tokens; do not add a parallel design system.

## Current behavior: verified versus incomplete

### Verified in the current checkpoint

- [x] Simple/sidebar changes apply live.
- [x] Active conditions remain visible as removable chips.
- [x] Multiple colors can be selected and removed individually.
- [x] Filter search supports keyboard navigation and common aliases.
- [x] Advanced draft changes do not affect the applied Library state before Apply.
- [x] Apply, Cancel, Escape, and staged Clear All have automated coverage.
- [x] Existing virtualized gallery and filter engine remain in place.
- [x] Dark and light semantic tokens and reduced-motion rules exist.
- [x] Multiple measured rows can stay expanded and can be closed independently.
- [x] No runtime dependency, network capability, cloud behavior, or external AI was added.

### Known gaps that must not be mistaken for completion

- [x] Simple numeric rows use a compact collapsed rhythm and disclose exact controls only on demand.
- [x] Quick filters remain in the Simple sidebar; Advanced uses five deeper categories without repeating the presets.
- [x] Search discovers filters and routes selections into the Advanced add/edit composer.
- [x] The Advanced `Add filter` state model is explicit, duplicate-safe, and wired through the picker/editor workflow.
- [x] Shared presentation labels and registry-derived typed editors replace raw boolean/operator values in Advanced.
- [x] Advanced is a right-side drawer and preserves a recognizable gallery at normal desktop widths.
- [x] The persistent third Advanced column has been removed.
- [x] Palette colors appear only in Appearance; switching categories clears temporary editors and preserves staged filters.
- [x] Draft preview uses the existing deterministic local filter query, debounced by 180ms and limited to one returned row while reading its exact total.
- [x] Clicking a staged non-color filter opens it for first-class in-place editing; color chips return focus to the palette control.
- [x] Advanced prevents unsupported duplicate field conditions by editing/upserting the existing staged field.
- [x] Automated coverage verifies initial picker focus, layered Escape handling, keyboard Apply, and restoration to the opening trigger.
- [x] The Phase 1 inspector is approved in dark/light isolated renders; the Advanced workflow is covered by component, state, and installed-app structural checks.

## Completed Phase 1: ordinary filtering

Goal: make everyday sidebar filtering fast before changing Advanced.

### Data/presentation model

- [x] Audit the current filter registry and add a presentation adapter only where needed:
  - `controlType`: boolean, enum, range, number, rating, color, date, text, or multi-select
  - label, category, description, natural operator options, value options
  - min/max/step/unit/default
  - unmeasured support and icon
- [x] Avoid duplicating backend definitions or changing filter-engine semantics.
- [x] Explicitly separate disclosure, configured, and active state.

### Independent measured-row disclosure

- [x] Replace `expandedField: string | null` with a `Set<FilterId>` (or equivalent).
- [x] Opening Brightness, Sharpness, and Contrast leaves all three open.
- [x] Closing one does not affect the others.
- [x] Expansion state stays stable while the sidebar remains mounted.
- [x] Automated coverage verifies multi-expand, independent closing, and numeric commits.

### Numeric-row refinement

- [x] Reduce collapsed rows to a compact 42px inspector rhythm.
- [x] Keep value summary, coverage, and chevron legible without large empty tracks.
- [x] Show slider and exact inputs only when expanded.
- [x] Keep only the selected range accented.
- [x] Ensure crisp 14px visual thumbs with 20px hit regions and keyboard support.
- [x] Refine exact input stepping and remove conflicting browser spinner styling.
- [x] Make Reset tertiary and unmeasured choices explicit.
- [x] For zero recorded values, show “Not recorded in this shoot” and omit the meaningless range.
- [x] Numeric changes publish only at pointer/keyboard interaction completion or exact-input commit; no debounce is needed.

### Simple inspector refinement

- [x] Search/add now precedes Active Filters (only when nonempty), then Colors, Quick Filters, Rating, Measured, category controls, and finally the More editor.
- [x] Make quick filters lighter and whole-row/whole-chip interactive, with a checkmark and non-color-only selected state.
- [x] Tighten active chips and cap their stack with local scrolling instead of unbounded panel growth.
- [x] Refine swatch checkmarks, labels, focus rings, and light/dark selection visibility.
- [x] Ensure search surfaces related quick and technical filters for `sharp`, `mono`, `iso`, and `face`.
- [x] Finish primary/secondary/tertiary button hierarchy.
- [x] Audit hover, pressed, focus-visible, disabled, transition, and reduced-motion states.

### Phase 1 validation and commit

Verified through focused interaction coverage and isolated rendered inspection on September 12, 2026:

1. [x] Brightness + Sharpness + Contrast can all stay open.
2. [x] Closing only Sharpness preserves the others.
3. [x] Slider drag, exact entry, reset, and unmeasured behavior.
4. [x] Quick-filter add/remove, multiple colors, search aliases, and active chips.
5. [x] Dark/light inspector modes, keyboard-visible focus, local scrolling, and preservation of the virtualized gallery path.

The full validation sequence and isolated visual inspection passed before the
Phase 1 files were committed as `refactor(filters): simplify core filter interactions`.

## Completed Phase 2: Advanced workflow

Goal: Advanced becomes a context-preserving deeper inspector.

### Replace the modal shell

- [x] Replace the large centered modal with a responsive right-side drawer/workspace.
- [x] Target roughly 480–620px on large desktop, with a sensible ~680px maximum.
- [x] Keep the photo grid visibly recognizable; the drawer overlays at compact desktop widths instead of compressing the gallery below a useful size.
- [x] Remove the persistent third summary/color column.
- [x] Use no backdrop or a very subtle backdrop; never near-black.
- [x] Use a short 200ms translate/fade transition.
- [x] Adapt to medium and narrow windows without forcing the desktop layout; the compact breakpoint is reachable above Tauri's minimum window width and is protected by a regression test.
- [x] Consolidate categories into at most six prominent destinations.

### Explicit draft/editor model

- [x] Keep `appliedFilters`, cloned `draftFilters`, and `currentEditor` conceptually distinct.
- [x] Opening Advanced clones applied state.
- [x] Cancel discards draft state.
- [x] X closes directly and consistently discards the private draft like Cancel.
- [x] Apply commits the entire valid draft, closes the drawer, preserves gallery context, and returns focus to the prior trigger.
- [x] Add a debounced draft preview count using the existing project-scoped filter query and exact result total.

### Fix Add Filter end to end

- [x] No editor → Add Filter → searchable picker.
- [x] Selecting a filter opens the registry-derived control for its data type.
- [x] Valid configuration enables a local `Add to filters` action.
- [x] Add inserts/updates `draftFilters`, updates ACTIVE, resets the editor, and supports adding another filter immediately.
- [x] Apply promotes the complete draft to applied filters and closes.
- [x] Prevent unsupported duplicate filters.
- [x] Clicking a staged filter reopens it for editing; do not require remove/re-add.
- [x] Provide clear `Save change` versus `Add to filters` behavior.

### Typed editors

- [x] Boolean: natural named states such as Black & white / Color; never `true`/`false`.
- [x] Binary analysis: natural terms for Contains faces, Closed-eye candidate, and Possible blink.
- [x] Enum: compact choices for orientation and other available values.
- [x] Range/number: compact dual-thumb range plus exact From/To and coverage.
- [x] Rating: natural At least / Exactly / At most choices backed by current comparison semantics.
- [x] Text/metadata: use available project-scoped values where the backend supplies them.
- [x] Date: calendar-backed single date or From/To controls use the existing date operators.
- [x] Expose operators only where meaningful and describe them in natural language; binary state editors omit the redundant operator.
- [x] Keep the separate Color and Monochrome engine fields distinct; the presentation adapter does not merge them.

### Advanced presentation, accessibility, and performance

- [x] Compact staged Active Filter stack with separate edit/remove actions.
- [x] Lighter two-column quick controls where space permits; no large checkbox cards.
- [~] Filter picker groups category entries and shows Added states; a recent-items group is still deferred.
- [x] Popovers remain small and temporary; the typed editor stays inline rather than nesting another popover.
- [x] Remove redundant instructional copy and dead space.
- [x] Complete keyboard flow: initial search focus, arrow/Enter picker navigation, Escape layering, Cmd/Ctrl+Enter Apply, logical Tab order.
- [x] Complete ARIA labels/states, native modal focus trapping/restoration, slider semantics, and non-color-only selection.
- [x] Preserve the drawer shell while swapping editor content with a subtle 140ms opacity fade.
- [x] Keep draft/editor keystrokes local to the drawer; the grid receives state only when Apply publishes the draft.
- [x] Avoid per-card layout animation; draft staging leaves the virtualized grid and its scroll context untouched.
- [x] Audit contrast and surface depth independently in dark and light themes through semantic-token tests, isolated renders, and the installed theme control.

### Phase 2 validation and commit

The functional matrix from the refinement brief is covered by the Phase 2 state/component suites:
- Add/apply Monochrome.
- Reopen, change, Cancel, and verify applied state is unchanged.
- Stage Sharpness and Brightness together, then Apply.
- Remove one staged filter without affecting others.
- Clear all staged filters and Apply.
- Search/add ISO with zero recorded values.
- Search `face`.
- Complete keyboard-only, theme, resize, scroll, and mid-library Apply checks.

Behavioral tests cover draft/apply/cancel, Add Filter, typed boolean/range editors, duplicate prevention, staged editing/removal, Clear All, search, multi-expand, drawer close behavior, keyboard focus restoration, and saved-view restoration. The Phase 2 implementation is committed as `feat(filters): streamline advanced filtering workflow`.

Do not squash the Phase 1 and Phase 2 commits.

## Validation ledger

The following passed for the checkpoint represented by `d62375e` + `41054bb`:

- `npm test`: 137 tests across 18 files.
- `npm run test:rust` after sourcing `/home/shivam/pg-env.sh`.
- `npm run typecheck`.
- `npm run build`.
- `npm run build:app`: debug executable and Debian bundle produced.
- `npm audit --omit=dev`: zero reported production vulnerabilities.
- `git diff --check`.
- Native startup smoke test against the existing TestPics2 catalog.
- Installed bundle verification on September 5, 2026.
- Isolated headless component renders of Advanced at 1440×900 in both themes.

These are historical checkpoint results. They do not validate any future Phase 1 or Phase 2 edits.

The final Phase 1 refinement checkpoint passed on September 12, 2026:

- `npm test`: 143 tests across 18 files.
- `npm run test:rust` after sourcing `/home/shivam/pg-env.sh`.
- `npm run typecheck`.
- `npm run build`.
- `npm run build:app`: debug executable and Debian bundle produced.
- `git diff --check`.
- Isolated renders of the real `FilterBar` component in dark and light modes,
  including selected swatches, non-color quick-filter state, and keyboard focus.

This checkpoint covers independent measured-filter expansion, exact empty-data
states, tighter numeric controls, the shared presentation adapter, search-first
ordering, and complete simple-inspector interaction states. Whole-Library and
Advanced-workflow visual acceptance remains part of Phase 2.

The Phase 2 implementation checkpoint passed on September 12, 2026:

- `npm test`: 165 tests across 20 files.
- `npm run test:rust` after sourcing `/home/shivam/pg-env.sh`.
- `npm run typecheck`.
- `npm run build`.
- `npm run build:app`: debug executable and Debian bundle produced.
- `git diff --check`.
- The Debian bundle was extracted into the local PhotoGremlin installation so
  the desktop launcher uses this checkpoint.

This checkpoint covers the context-preserving drawer, explicit private draft
state, searchable duplicate-safe Add Filter workflow, registry-derived typed
editors, staged edits/removals, keyboard Apply and focus restoration, and an
exact debounced draft match count.

The final acceptance-hardening checkpoint passed on September 13, 2026:

- `npm test`: 168 tests across 22 files.
- `npm run test:rust` after sourcing `/home/shivam/pg-env.sh`; the workstation
  resource guard required the repository-sanctioned
  `PHOTOGREMLIN_ALLOW_LOW_MEMORY=1` override at 5.8 GiB available memory.
- `npm run typecheck`.
- `npm run build`.
- `PHOTOGREMLIN_ALLOW_LOW_MEMORY=1 npm run build:app`: debug executable and
  Debian bundle produced.
- `git diff --check`.
- The Debian bundle was re-extracted into the local PhotoGremlin installation
  and started against the 277-photo, fully analyzed TestPics2 catalog.
- The installed accessibility tree exposes the gallery as a 14-row loaded,
  7-column virtual grid with explicit row and grid-cell indices. Pure
  virtualization tests cover deep-scroll range clamping, bounded mounting,
  and Home/End scroll behavior.
- Opening a real indexed photograph in the installed bundle exposes the
  rendered `HISTOGRAM` region and both `Luma` and `RGB` controls.
- The installed theme selector was exercised and the app was returned to the
  Library with `Darkroom` checked.
- A responsive-layout contract test keeps the compact overlay breakpoint at
  or above Tauri's 1024px minimum width. This fixes the previously unreachable
  compact mode that allowed the open inspector to over-compress the gallery.
- Saved-view restoration is covered across the persisted Rust CRUD/count
  integration and the actual Sidebar-to-filter-store UI path.

### Validation commands for follow-up changes

Run sequentially from the repository root:

```bash
source /home/shivam/pg-env.sh
npm run test:rust
npm test
npm run typecheck
npm run build
PHOTOGREMLIN_ALLOW_LOW_MEMORY=1 npm run build:app
git diff --check
```

Before committing:
```bash
git status --short --branch
git diff
git add <only phase-related files>
git diff --staged
git commit -m "<phase commit message>"
```

## Acceptance status

- [x] Phase 1 `FilterBar` in isolated dark/light renders, including selected and keyboard-focus states (September 12, 2026).
- [~] Whole-Library dark and light pixel comparison against the supplied screenshot. Semantic theme behavior, isolated renders, and installed native structure are verified, but GNOME/Wayland blocks screenshot capture from this automation environment; owner visual sign-off remains appropriate.
- [x] Drawer context preservation, subtle backdrop, staged edits, Apply/Cancel, and trigger focus restoration.
- [x] Hover, focus, pressed, selected, disabled, placeholder, metadata, chip, slider, and swatch states through interaction and theme-contract coverage.
- [x] Long values, many active filters, zero matches, partial/unmeasured data, and the minimum-width responsive contract.
- [x] Complete keyboard path and focus restoration in automated component coverage.
- [x] Virtualized range behavior with large collections, bounded mounted rows, and installed validation against 277 photographs.
- [x] Saved-view persistence/count behavior and restoration of a real saved-view filter shape through the production Sidebar/store path.
- [x] Grid state remains isolated while Advanced edits its private draft and updates only when Apply publishes it.

## Handoff

No functional Phase 2 implementation work remains. Before merging, the owner
should perform the single environment-limited pixel-level dark/light review on
their desktop and confirm the visual result. Do not merge to `develop` from an
agent session unless the owner explicitly asks for that merge.

## Definition of done

The redesign implementation is complete: ordinary filters are compact and live, several numeric controls can stay open, Advanced preserves Library context, Add Filter reliably stages multiple typed conditions, Cancel discards, Apply commits and closes, active state is always visible, keyboard and theme behavior are covered, performance remains stable, required tests/builds pass, and both phase commits exist. Final merge readiness depends only on the owner's pixel-level visual sign-off noted above.
