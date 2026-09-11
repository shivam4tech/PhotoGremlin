# Library and filter redesign — progress and resume guide

_Last updated: September 11, 2026_

This is the canonical resume document for the Library/filter redesign. Read this before continuing work. It records what is actually implemented on the current feature branch, what has only been partially solved, and the remaining two-phase plan. Do not treat the existing Advanced modal as the final design.

## Repository checkpoint

- Branch: `feat/library-filter-redesign`
- Base/checkpoint: `8194e47` (`develop`) — `chore: checkpoint before library and filter redesign`
- Completed implementation commits:
  - `d62375eb534727d876e349d411c48ceedc94b145` — `feat: redesign library and filter control surface`
  - `41054bbeaeea54bdd088e8108a02a22cc3e2a34b` — `feat: add staged advanced filter workspace`
- The branch has not been merged into `develop`.
- The next refinement work must remain split into two commits:
  1. `refactor(filters): simplify core filter interactions`
  2. `feat(filters): streamline advanced filtering workflow`

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
- [~] Only one measured row can currently remain expanded because `QuickFilterControls` stores `expandedField: string | null`. This directly conflicts with the latest requirement.

## Current implementation map

- `src/components/filters/FilterBar.tsx`
  - Simple inspector composition, rating/color/quick filters, measured rows, and generic condition composer.
  - Known issue: boolean and other conditions still expose backend-style operators/values such as `is` and `true`.
  - Known issue: candidate insertion needs duplicate prevention and a clearer editor state machine.
- `src/components/filters/QuickFilterControls.tsx`
  - Quick-filter controls and `RangeFilterRow`.
  - Known issue: single-open disclosure state.
- `src/components/filters/AdvancedFiltersDialog.tsx`
  - Current draft-state modal and Apply/Cancel behavior.
  - Known issue: oversized modal, category rail + editor + persistent summary column, strong backdrop, no live preview count.
- `src/components/filters/ActiveFilterList.tsx`
  - Removable applied/draft filter chips, including per-color chips.
- `src/components/filters/FilterPicker.tsx`
  - Searchable keyboard-accessible filter picker.
  - Known issue: active entries are labelled Added, but duplicate prevention must be enforced by the state layer.
- `src/components/filters/filterDiscovery.ts`
  - Substring/alias discovery (for example sharp, mono, ISO, face).
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
- [x] No runtime dependency, network capability, cloud behavior, or external AI was added.

### Known gaps that must not be mistaken for completion

- [ ] Multiple measured rows cannot stay expanded together.
- [~] Numeric rows are more capable but remain vertically/form-heavy in the actual sidebar.
- [~] Quick filters remain too rectangular/heavy in Simple and especially Advanced.
- [~] Search discovers filters but the add/configure flow remains fragmented.
- [ ] The Advanced `Add filter` state machine is not complete or reliable enough for repeated additions.
- [ ] Advanced uses generic database-like boolean/operator controls.
- [ ] Advanced is still a large modal that hides too much of the Library.
- [ ] The persistent third Advanced column must be removed.
- [ ] Draft preview/matching count is not implemented.
- [ ] Editing an already-staged filter is not a clear first-class flow.
- [ ] Duplicate filter conditions are not safely prevented.
- [~] Focus management exists in parts but needs a complete drawer/picker/editor/apply return path.
- [ ] Current dark/light UI has not been visually approved against the latest screenshots after the requested refinements.

## Remaining work — Phase 1: ordinary filtering

Goal: make everyday sidebar filtering fast before changing Advanced.

### Data/presentation model

- [ ] Audit the current filter registry and add a presentation adapter only where needed:
  - `controlType`: boolean, enum, range, number, rating, color, date, text, or multi-select
  - label, category, description, natural operator options, value options
  - min/max/step/unit/default
  - unmeasured support and icon
- [ ] Avoid duplicating backend definitions or changing filter-engine semantics.
- [ ] Explicitly separate disclosure, configured, and active state.

### Independent measured-row disclosure

- [ ] Replace `expandedField: string | null` with a `Set<FilterId>` (or equivalent).
- [ ] Opening Brightness, Sharpness, and Contrast must leave all three open.
- [ ] Closing one must not affect the others.
- [ ] Keep expansion state stable while the sidebar remains mounted.
- [ ] Add an automated multi-expand test.

### Numeric-row refinement

- [ ] Reduce collapsed rows to a compact 40–48px inspector rhythm.
- [ ] Keep value summary, coverage, and chevron legible without large empty tracks.
- [ ] Show slider and exact inputs only when expanded.
- [ ] Keep only the selected range accented.
- [ ] Ensure crisp 14–16px visual thumbs with larger hit regions and keyboard support.
- [ ] Refine exact input stepping and remove conflicting browser spinner styling.
- [ ] Make Reset tertiary and unmeasured choices explicit.
- [ ] For zero recorded values, show “Not recorded in this shoot” and omit/disable the meaningless range.
- [ ] Confirm live updates remain performant; debounce only if needed.

### Simple inspector refinement

- [ ] Final order: header/count, search/add, Active Filters (only when nonempty), Colors, Quick Filters, Rating, Measured, More.
- [ ] Make quick filters lighter and whole-row/whole-chip interactive, with a checkmark and non-color-only selected state.
- [ ] Tighten active chips and handle overflow without unbounded panel growth.
- [ ] Refine swatch checkmarks, labels, focus rings, and light/dark selection visibility.
- [ ] Ensure search surfaces related quick and technical filters for `sharp`, `mono`, `iso`, and `face`.
- [ ] Finish primary/secondary/tertiary button hierarchy.
- [ ] Audit hover, pressed, focus-visible, disabled, transition, and reduced-motion states.

### Phase 1 validation and commit

Manually verify:
1. Brightness + Sharpness + Contrast can all stay open.
2. Closing only Sharpness preserves the others.
3. Slider drag, exact entry, reset, and unmeasured behavior.
4. Quick-filter add/remove, multiple colors, search aliases, active chips.
5. Dark/light modes, keyboard focus, scrolling, and gallery performance.

Then run the full validation sequence below, inspect the UI, stage only Phase 1 files, and commit:
`refactor(filters): simplify core filter interactions`

Do not begin Phase 2 until this commit exists and is green.

## Remaining work — Phase 2: Advanced workflow

Goal: Advanced becomes a context-preserving deeper inspector.

### Replace the modal shell

- [ ] Replace the large centered modal with a responsive right-side drawer/workspace.
- [ ] Target roughly 480–620px on large desktop, with a sensible ~680px maximum.
- [ ] Keep the photo grid visibly recognizable.
- [ ] Remove the persistent third summary/color column.
- [ ] Use no backdrop or a very subtle backdrop; never near-black.
- [ ] Use a short 200–240ms translate/fade transition.
- [ ] Adapt to medium and narrow windows without forcing the desktop layout.
- [ ] Consolidate categories into at most six prominent destinations.

### Explicit draft/editor model

- [ ] Keep `appliedFilters`, cloned `draftFilters`, and `currentEditor` conceptually distinct.
- [ ] Opening Advanced clones applied state.
- [ ] Cancel discards draft state.
- [ ] X closes directly when clean; when dirty, consistently discard like Cancel or show one lightweight confirmation.
- [ ] Apply validates, commits the entire draft, closes the drawer, preserves gallery context, and returns focus to the trigger.
- [ ] Add a debounced draft preview count if calculation cost requires it.

### Fix Add Filter end to end

- [ ] No editor → Add Filter → searchable picker.
- [ ] Selecting a filter opens the correct typed editor.
- [ ] Valid configuration enables a local `Add to filters` action.
- [ ] Add inserts/updates `draftFilters`, updates ACTIVE, resets the editor, and supports adding another filter immediately.
- [ ] Apply promotes the complete draft to applied filters and closes.
- [ ] Prevent unsupported duplicate filters.
- [ ] Clicking a staged filter reopens it for editing; do not require remove/re-add.
- [ ] Provide clear `Save change` versus `Add filter` behavior.

### Typed editors

- [ ] Boolean: natural choices such as Any / Monochrome / Color; never `true`/`false`.
- [ ] Binary analysis: natural terms for Contains faces, Closed-eye candidate, and Possible blink.
- [ ] Enum: compact choices for orientation and other available values.
- [ ] Range/number: compact dual-thumb range plus exact From/To and coverage.
- [ ] Rating: natural At least / Exactly / At most only if supported by current semantics.
- [ ] Text/metadata: searchable available values where the backend already supplies them.
- [ ] Date: From/To only if date filtering actually exists.
- [ ] Expose operators only where meaningful and describe them in natural language.
- [ ] Do not merge Color and Monochrome semantics without verifying backend equivalence.

### Advanced presentation, accessibility, and performance

- [ ] Compact staged Active Filter stack with edit/remove.
- [ ] Lighter two-column quick controls where space permits; no large checkbox cards.
- [ ] Filter picker groups recent/category entries and shows Added states.
- [ ] Popovers remain small and temporary; no nested editor popovers.
- [ ] Remove redundant instructional copy and dead space.
- [ ] Complete keyboard flow: initial search focus, arrow/Enter picker navigation, Escape layering, Cmd/Ctrl+Enter Apply, logical Tab order.
- [ ] Complete ARIA labels/states, focus trapping/restoration, slider semantics, and non-color-only selection.
- [ ] Preserve the drawer shell while swapping editor content with a subtle 120–160ms fade.
- [ ] Avoid drawer keystrokes triggering unnecessary grid rerenders.
- [ ] Avoid per-card layout animation; preserve virtualization and scroll context.
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

- Dark and light Library against the supplied current/reference screenshots.
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
5. Start with Phase 1 independent expansion in `QuickFilterControls.tsx`; this is the clearest current requirement and testable seam.
6. Complete all Phase 1 items, validate, and create the prescribed Phase 1 commit.
7. Only then replace `AdvancedFiltersDialog` with the drawer and complete Phase 2.
8. Validate and create the prescribed Phase 2 commit.
9. Do not merge to `develop` until both phases and manual acceptance are green.

## Definition of done

The redesign is complete only when ordinary filters are compact and live, several numeric controls can stay open, Advanced preserves Library context, Add Filter reliably stages multiple typed conditions, Cancel discards, Apply commits and closes, active state is always visible, keyboard and theme behavior are polished, performance remains stable, required tests/builds pass, both phase commits exist, and manual visual acceptance is recorded.
