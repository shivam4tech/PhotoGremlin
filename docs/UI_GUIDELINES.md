# UI Guidelines — the polish bar

Distilled from the [ibelick/ui-skills](https://github.com/ibelick/ui-skills)
collection (baseline-ui, improve-ui, fixing-accessibility,
fixing-motion-performance), adapted to PhotoGremlin's real stack: React +
TypeScript + Zustand over **hand-written CSS driven by design tokens**
(`src/styles/theme.css`) — no Tailwind, no animation library, no component
framework. Where a skill says "use Tailwind utilities", the rule here is its
token-based equivalent.

These are review criteria for every UI change: violations should be quoted
with a concrete fix, like the skills prescribe.

## 1. Tokens are the law

- Every color, radius, shadow, font comes from `--*` tokens in
  `theme.css`; raw hex values in components are violations.
- One accent per view. Restrained cool blue `--accent` is the single affordance
  color. `--positive` is reserved for kept/complete state, while
  `--warning/--danger/--info` communicate their named states, not decoration.
- Both themes must always be checked: if you touch a color, verify dark AND
  light (`data-theme="light"`) contrast.
- New UI needs new tokens only when existing ones genuinely cannot express
  it — add them to both theme blocks together, or don't add them.
- Standard controls are 32 px high; compact toolbar, form and segmented
  controls are 30 px high. Bespoke square icon controls may be smaller only
  when they are isolated from a standard control row.

## 2. Layout & hierarchy baseline

- Spacing steps in multiples of 4px (`4/8/12/16/24/32`); no arbitrary gaps.
- Hierarchy comes from size + weight + `--text/--text-dim/--text-faint`
  dimming — never from new colors.
- Fixed z-index scale only (check `theme.css`); no ad-hoc `z-index: 999`.
- Square elements use width+height pairs consistently via shared classes;
  fixed panels respect `--sidebar-w` / `--topbar-h`.
- Every empty state must offer exactly one clear next action (see
  `EmptyState.tsx`) — an action, not just an explanation.

### Library workspace ownership (Sprint 24)

The Library uses a stable three-pane desktop workspace, adapted from the
source/work-area/task-panel pattern used by photography tools such as
Lightroom Classic:

- the application sidebar is the **source and management rail**. Review views
  are first, followed by current/recent folders, saved views, and links to
  Dashboard, Sessions, Collections, and Saved Views management;
- the center is the **photograph work area**. It owns progress, similarity
  groups, culling/file-operation surfaces, the virtualized grid, and its
  status bar. Filter controls do not consume vertical canvas space;
- the right Library inspector is the **filter task panel**. It owns active
  condition chips, measured quick filters, the advanced condition composer,
  clear, and save-as-view;
- scan, metadata, analysis, similarity, culling, and review commands remain in
  the horizontal Library action bar so long-running operations stay visible.

At the supported 1024 px minimum window width, the rails remain visible and
the center grid stays the only photograph scroller. The inspector has its own
vertical scroll; it must never make the virtual grid measure its contents.
This is a structural convention, not an Adobe visual clone: PhotoGremlin keeps
its own graphite/silver tokens, language, and local-first controls.

### Photo-first culling (Sprint 26)

- Completed scan, metadata, analysis and grouping statistics belong in the
  developer console and local logs, not above the photographs. Running work
  and actionable failures remain visible; successful passes use short notices.
- Cull uses one compact action tray. Bulk marks, Collections, export and file
  actions do not become separate full-width panels; detailed file operations
  open only when requested and retain preview-first safety.
- Normal Library tiles reveal marks on hover, focus or selection. Cull and
  Collections keep marks visible because they are active decision surfaces.
- Rating stars are cumulative: a rating of four illuminates stars one through
  four. The inspector rating control is a minimum threshold (`1+` through
  `5+`), plus Any and Unrated.
- The 12-hue color spectrum is the inspector's primary visual filter. It is
  always visible, supports multi-selection, communicates **match any**, and
  shows selected hues as removable text-labelled chips. Hue is deterministic
  image data, not a color label or aesthetic verdict.
- Brightness, sharpness, contrast, ISO and focal length are dual-range controls
  inside a labelled disclosure. The disclosure opens automatically whenever a
  measured range is active; otherwise it stays closed to keep the inspector
  calm and progressively reveal precision controls. Their tracks use one
  restrained low-to-high cool-blue tonal ramp, so measurement controls remain
  easy to identify without turning the inspector into a multicolor dashboard.
  This is measurement context, never a red/green quality verdict. Exact values are
  hidden at rest and appear in the thumb bubble while the control is being
  adjusted; screen readers receive the same value through the native range
  input and `aria-valuetext`.
- Those five ranges are not offered again by the advanced composer. Legacy
  saved conditions remain visible and removable without silent rewriting.
- Similar, burst and local face-appearance results live in Groups, not above
  the Library grid. Group cards use neutral factual descriptions and open into
  the same virtual photo grid and viewer used by other photo surfaces.

## 3. Typography

- Headings and stat labels may set `text-wrap: balance`; body text
  `text-wrap: pretty`. Never letter-spacing tweaks.
- **All numeric data uses the mono font with `font-variant-numeric:
  tabular-nums`** ("sharpness 62", ISO values, counts, dashboard figures) —
  data columns must not jiggle.
- Dense UI truncates deliberately (`text-overflow: ellipsis` +
  `title=` tooltip), never overflows silently.

## 4. Motion (the premium feel is restraint)

- Animate ONLY compositor properties: `transform`, `opacity`. Never
  animate `width/height/top/left/margin/padding` — use FLIP or opacity.
- Interaction feedback ≤ 200ms, entrances use ease-out. One-shot effects
  only; nothing loops except progress indicators.
- Respect `prefers-reduced-motion: reduce` — transitions collapse to near-
  instant, looping spinners become static states.
- Blur/backdrop-filter never animates on large surfaces (photo grid!);
  `will-change` only during an active animation, then removed.
- No decorative gradients, no glow-as-affordance, no decorative motion. The
  measured range tracks and the functional hue spectrum are the only
  data-encoding gradient exceptions.
  Large translucent surfaces must not use backdrop blur; an opaque semantic
  overlay is clearer over photographs and cheaper to render.
- View changes and disclosure entrances follow the Transitions.dev portable
  CSS pattern: the entering inner surface fades and translates no more than
  8px with an explicit 180ms ease-out transition. Controls may use the same
  timing for a small transform/opacity state change. Never use `transition:
  all`, and never animate the outer layout container.

## 5. Accessibility (non-negotiable)

- Icon-only buttons carry `aria-label`; decorative icons `aria-hidden`.
- Native elements first (`button/a/input`) — divs-with-onClick are
  violations even when they look right.
- Focus must be visible (`:focus-visible` styles exist — keep them),
  dialogs trap focus and restore it to the trigger on close, Escape closes.
- Form errors link via `aria-describedby` + `aria-invalid`; destructive
  actions confirm through the dialog flow (file ops already do — keep it).
- Hover-only reveals need keyboard equivalents.
- Photos in the grid get meaningful alt text ("beach — sunset", never "").
- Critical errors never live in toasts alone (toast + inline/log surface).

## 6. Loading & performance (photo grid specifics)

- Structural skeletons while loading — never blank panes, never spinners
  where a skeleton fits (`ProgressBar.tsx` covers long jobs).
- The grid is `VirtualGrid.tsx`: never fight virtualization with CSS that
  measures content (no auto-height animations inside tiles).
- Thumbnails are the render unit — never decode originals outside the
  viewer; lazy-load below the fold via the grid's own mechanism.
- Batch DOM reads before writes; no scroll-position polling — visibility
  effects use IntersectionObserver.
- Long jobs report progress via events (rule 7 of AGENTS.md); the UI never
  freezes on Rust work.

## 7. Language & copy (product philosophy made visible)

- Measurable characteristics only: "sharpness 62", "high highlight
  clipping", "similar photograph", "eyes closed". NEVER verdicts: "bad",
  "good", "delete this?", "you improved".
- Model-derived tags follow the same honesty rules: show confidence, top-3,
  and honest "unavailable" instead of fake zeros when the model is absent.
- Buttons are verbs ("Move to trash"), errors say what happened and what to
  do next, empty states invite the core loop.

## 8. Review checklist (run before finishing any UI change)

```
[ ] tokens only — no raw colors/radii/shadows
[ ] both themes verified
[ ] one accent per view; semantic colors used semantically
[ ] motion: transform/opacity only, ≤200ms, reduced-motion respected
[ ] icon-only controls have aria-labels; focus visible everywhere
[ ] numbers in mono + tabular-nums
[ ] loading = skeletons/progress, empty = one clear next action
[ ] no layout thrash inside VirtualGrid tiles
[ ] copy passes the "measurable, never verdict" test
```

## Sources

- baseline-ui — spacing/hierarchy/typography/animation/design baselines
- fixing-accessibility — §5 checklist priorities
- fixing-motion-performance — §4 rendering rules (composite vs paint vs layout)
- [Transitions.dev](https://www.ui-skills.com/skills/jakubantalik/transitions-dev)
  — portable CSS entrance/state-change patterns and reduced-motion fallbacks
- improve-ui — evidence-based audit method: findings need contract + runtime
  proof + a single deterministic correction; prefer no finding to a vague one
