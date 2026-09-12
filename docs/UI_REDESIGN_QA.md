# Library and filter redesign — progress and resume guide

_Last updated: September 12, 2026_

This is the canonical resume document for the Library/filter redesign. Read this before continuing work. It records what is actually implemented on the current feature branch, what has only been partially solved, and the remaining two-phase plan. Do not treat the existing Advanced modal as the final design.

## Repository checkpoint

- Branch: `feat/library-filter-redesign`
- Base/checkpoint: `8194e47` (`develop`) — `chore: checkpoint before library and filter redesign`
- Completed implementation commits:
  - `d62375eb534727d876e349d411c48ceedc94b145` — `feat: redesign library and filter control surface`
  - `41054bbeaeea54bdd088e8108a02a22cc3e2a34b` — `feat: add staged advanced filter workspace`
  - `refactor(filters): simplify core filter interactions` — final Phase 1 refinement checkpoint (this document is committed with it)
- The branch has not been merged into `develop`.
- Phase 1 is complete. The remaining Phase 2 refinement must be committed separately as
  `feat(filters): streamline advanced filtering workflow`.

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
  - The complete drawer workflow still needs whole-application visual acceptance.
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
- [x] Quick filters are lightweight in Simple and use a compact two-column layout in Advanced where space permits.
- [x] Search discovers filters and routes selections into the Advanced add/edit composer.
- [x] The Advanced `Add filter` state model is explicit, duplicate-safe, and wired through the picker/editor workflow.
- [x] Shared presentation labels and registry-derived typed editors replace raw boolean/operator values in Advanced.
- [~] Advanced is now a right-side drawer; whole-Library visual acceptance is still pending.
- [x] The persistent third Advanced column has been removed.
- [x] Draft preview uses the existing deterministic local filter query, debounced by 180ms and limited to one returned row while reading its exact total.
- [x] Clicking a staged non-color filter opens it for first-class in-place editing; color chips return focus to the palette control.
- [x] Advanced prevents unsupported duplicate field conditions by editing/upserting the existing staged field.
- [x] Automated coverage verifies initial picker focus, layered Escape handling, keyboard Apply, and restoration to the opening trigger.
- [~] The Phase 1 inspector is approved in dark/light isolated renders; the Advanced drawer and whole-workflow Phase 2 matrix remain open.

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

## Remaining work — Phase 2: Advanced workflow

Goal: Advanced becomes a context-preserving deeper inspector.

### Replace the modal shell

- [x] Replace the large centered modal with a responsive right-side drawer/workspace.
- [x] Target roughly 480–620px on large desktop, with a sensible ~680px maximum.
- [~] Keep the photo grid visibly recognizable; the drawer geometry preserves it, but visual acceptance is pending.
- [x] Remove the persistent third summary/color column.
- [x] Use no backdrop or a very subtle backdrop; never near-black.
- [x] Use a short 200ms translate/fade transition.
- [~] Adapt to medium and narrow windows without forcing the desktop layout; responsive rules exist, but visual acceptance is pending.
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
- [ ] Audit contrast and surface depth independently in dark and light themes.

### Phase 2 validation and commit

Run the full functional matrix from the refinement brief:
- Add/apply Monochrome.
- Reopen, change, Cancel, and verify applied state is unchanged.
- Stage Sharpness and Brightness together, then Apply.
- Remove one staged filter without affecting others.
- Clear all staged filters and Apply.
- Search/add ISO with zero recorded values.
- Search `face`.
- Complete keyboard-only, theme, resize, scroll, and mid-library Apply checks.

Add behavioral tests for draft/apply/cancel, Add Filter, typed boolean/range editors, duplicate prevention, staged editing/removal, Clear All, search, multi-expand, and drawer close behavior. Then run the full validation sequence, stage only Phase 2 files, and commit:
`feat(filters): streamline advanced filtering workflow`

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
exact debounced draft match count. The whole-application visual acceptance
matrix below remains open.

### Validation commands for every remaining phase

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

## Manual visual acceptance still required

- [x] Phase 1 `FilterBar` in isolated dark/light renders, including selected and keyboard-focus states (September 12, 2026).
- Whole-Library dark and light comparison against the supplied current/reference screenshots.
- Drawer/context preservation and backdrop strength.
- Hover, focus, pressed, selected, disabled, placeholder, metadata, chip, slider, and swatch contrast.
- Long filenames, many active filters, zero matches, partial/unmeasured data, and narrow windows.
- Complete keyboard path and focus restoration.
- Repeated filter/cull workflow and scrolling with hundreds of photographs.
- Saved-view create/restore using real combinations.
- Grid remains stable and responsive when sidebar filters update live and Advanced applies a draft.

## Exact resume procedure

1. Read this document and `AGENTS.md`.
2. Run `git status --short --branch`; preserve all unrelated work.
3. Confirm the branch is `feat/library-filter-redesign` and inspect commits after `8194e47`.
4. Inspect the six files in “Current implementation map” before editing.
5. Preserve the completed Phase 1 behavior and tests.
6. Replace `AdvancedFiltersDialog` with the drawer and complete Phase 2.
7. Validate and create the prescribed Phase 2 commit.
8. Do not merge to `develop` until Phase 2 and the remaining manual acceptance are green.

## Definition of done

The redesign is complete only when ordinary filters are compact and live, several numeric controls can stay open, Advanced preserves Library context, Add Filter reliably stages multiple typed conditions, Cancel discards, Apply commits and closes, active state is always visible, keyboard and theme behavior are polished, performance remains stable, required tests/builds pass, both phase commits exist, and manual visual acceptance is recorded.
