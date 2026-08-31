# Culling and shoot review

Status: **Sprint 19 review workflow shipped.** The first V1 culling tool is a
fast, local, photographer-led review flow — not an automatic aesthetic judge.
For an indexed folder/session, **Review this shoot** builds capture-time order
and reuses existing burst and similar-frame groups as decision context. Every
photo appears once, even when similarity groups overlap; burst context wins an
overlap because the nearby frames are usually the useful comparison.

The reviewer offers three reversible local states:

- `selected` — keep for the photographer's next step;
- `rejected` — a deliberate non-destructive rejection; and
- `needs_attention` — return later without pretending the image is wrong.

No review decision moves, renames, exports, trashes, rates, or labels a file.
The keyboard flow is `K` keep, `X` reject, `L` later, `Backspace` clear,
`U` undo, and arrow keys (or `H`/`J`) to move between moments. A session's
normal Library view now also has one-click `Unreviewed`, `Kept`, and `Needs
attention` filters. `review_state is-null` is the stable unreviewed queue.

## Cull workspace (Sprint 26)

The filterable grid is the bulk workspace. It begins with photographs rather
than completed-operation reports or group cards. A single compact tray shows
kept/rejected/later counts and exposes cumulative bulk ratings, Collection
membership, exports and preview-first file actions. Contact sheets are an
export format (printable PNG reference pages), not a Collection or original
file handoff.

The tray's Export and More menus use the shared popover layer above the photo
grid, so tile controls never paint through their opaque surfaces. Both menus
use the same two-line option rows and close as soon as an action is chosen.

Sprint 27 removes the former 20,000-decision IPC ceiling. Selection rows are
cursor-paged in 5,000-row-or-smaller chunks and joined to the active session,
so a large project cannot leak decisions from another shoot or silently lose
older decisions. The frontend waits for the active session before loading the
map. Review also stores `unit_index` and the focused photo per session; opening
**Review this shoot** resumes the last decision context, while the decisions
themselves remain independently durable in `selections`.

Rating/flag/label writes increment a shared `marksVersion`. Library queries,
Collection and Group grids and the open viewer refresh from that version, so
every photo surface reflects a successful mark mutation without reopening the
view. Similar, burst and local face-appearance cards live in the dedicated
Groups workspace; removing them from above the Library grid does not make the
grouping result unreachable.

## Finish and editor handoff (Sprint 28)

When every indexed photograph in a shoot has a decision, Review changes to a
finish state instead of silently wrapping back to the queue. It reports the
kept/rejected/later totals and offers three reversible next steps: open the kept
set in a configured local editing application, return to a Library view filtered
to kept photographs, or review the decisions again.

The editing application is chosen in Settings and stored as a machine-level
preference. PhotoGremlin passes normal source paths to that executable; it does
not edit a Lightroom or Capture One catalog, create sidecars, move originals,
or change decisions. Direct launch is capped at 500 files because desktop
applications and operating-system argument limits vary. Larger kept sets stay
available through **Export originals**, which uses the existing preview-first
copy workflow. See `EDITOR_HANDOFF.md` for the safety boundary.

## Sequence comparison (Sprint 29)

Grouped review moments expose **Compare sequence** and the `C` shortcut. The
dialog shows a capture-order window of at most four photographs around the
focused frame. It fetches the same bounded local thumbnails as Review rather
than decoding originals, and one zoom control plus normalized scroll position
keeps every pane synchronized. A photographer can focus any frame back into
the review moment without changing its decision.

Comparison captions show only the filename and current reversible decision.
There is no automatic winner, score, or aesthetic language. Limiting the
surface to four thumbnails keeps memory use predictable on large shoots while
still covering the adjacent frames needed for a burst or similarity decision.

## Deferred scoring research

The following was explored as a future configurable assistance layer. It is
not V1 behavior, and no score/bucket below is currently shown or written by
the product. Any future implementation must retain the workflow above and
explain measured signals without making an aesthetic verdict.

## What Aftershoot does (research summary)

1. **Genre selection** — the photographer picks the shoot type; the AI weighs
   factors per genre.
2. **Scores every image 1–100** across "30+ factors": sharpness, exposure,
   blinks, facial expressions, lighting, background clearness, composition.
3. **Groups duplicates/similar frames** (lenient → extreme clustering) and
   picks the best of each group.
4. **Buckets**: Selected / Highlights / Duplicates / Blurry / Closed Eyes /
   Warnings, with per-bucket strictness sliders.
5. **Non-destructive**: never deletes; writes ratings/labels; the
   photographer reviews and swaps; learns preferences from overrides.
6. Runs locally.

## Genre matrix (24 scenarios → reject signals)

| Genre | Auto-reject signals | Twist |
|---|---|---|
| Wedding | closed eyes (groups!), blink, motion blur, blown dress highlights | expression beats sharpness |
| Sports | motion blur on subject, focus behind action, underexposure | peak-action in burst |
| Wildlife | soft eye focus, subject too small, clipped limbs | eye sharpness is king |
| Birding | wing blur, focus miss, tiny in frame | |
| Portrait | closed eyes, soft iris focus, squint | |
| Headshot/Corporate | blink, unsharp eyes, harsh shadows | set consistency |
| Newborn | — sleeping baby is NOT a blink reject | disable blink filter |
| Family/Children | blink, motion blur | |
| Maternity | soft focus, unflattering shadows | |
| Event/Party | closed eyes in groups, low-light blur | |
| Concert | underexposure, light blowout, motion blur | grain tolerance higher |
| Street | — blur/motion often intentional | technical filters relaxed |
| Documentary | — emotion > technical | invert weights |
| Landscape | blown highlights, underexposure, haze | bracket-pick bursts |
| Astrophotography | star trailing, noise, focus miss | special thresholds |
| Macro | DoF miss on subject plane, shake | |
| Product/E-com | hero-object focus miss, color cast, glare | studio consistency |
| Food | focus miss, color cast, reflections | |
| Real estate | tilted verticals, blown windows, HDR ghosts | |
| Architecture | tilt, distortion, blown sky | |
| Fashion/Editorial | garment/face softness, pose dupes | |
| Travel | blocked subject, exposure | |
| Pet | motion blur, closed eyes, cut-offs | |
| Boudoir | soft focus, expression | |

Pattern: **the same measurable features with per-genre weights/toggles** —
profiles are pure config, not ML.

## Gap analysis

| Cull ingredient | PhotoGremlin today |
|---|---|
| Sharpness/exposure/clipping/contrast/noise proxy | ✅ Sprint 4 |
| Duplicate/burst grouping + best-of-group | ✅ Sprints 8+16 |
| Face presence/count | ✅ YuNet (Sprint 9) |
| Ratings/flags/labels write-back | ✅ Sprint 13 |
| Eye-state candidates | ✅ OCEC-S per detected eye (Sprint 33); burst-context blink follows in Sprint 34 |
| Blur *type* (motion vs defocus) | ❌ synthetic training, $0 |
| Subject-region sharpness | ⚙️ existing sharpness inside YuNet box |
| Smile/expression | ⏳ v0.2 item, pulled into culling scope later |
| Genre profiles | ⚙️ config only |
| Personalization | ⚙️ v2: learn from user's own marks |

## Eye-state licensing decision (researched 2026-08-18, implemented Sprint 33)

CEW and RT-BENE are research-only (unusable commercially). Viable paths:

- **A (selected): OCEC-S pretrained ONNX model** (PINTO model zoo) — MIT
  licensed, embedded locally using the same runtime boundary as YuNet.
- **B (fallback): train our own** on `MichalMlodawski/closed-open-eyes`
  (HuggingFace, ODC-By v1.0 — attribution-only, commercial-safe).
- C: MediaPipe face-mesh landmarks + EAR threshold (Apache-2.0) — new runtime
  dependency, last resort.

## Training-data adjustments

- Scene corpus (Sprint 17): unchanged; doubles as genre context.
- Blur-type: synthetic motion/defocus kernels applied to our own CC-BY thumbs.
- Eye-state: path A/B above.
- No aesthetic-judgment corpora (KonIQ/SPAQ etc.) — research licenses +
  violates "no verdicts".

## Deferred sprint map

- **Future cull engine**: models (blur-type CNN; eye state is now available) + Rust core:
  group → weighted score → rank → buckets → write ratings/labels (never
  delete). Genre profiles as config.
- **Future assisted review UI**: genre picker, strictness sliders, survey mode,
  side-by-side swap.
- **Future (v2) personalization**: tiny local model trained on the user's own
  flags/ratings; no external corpus.

## Philosophy compliance

We suggest and rank; reasons are measurable ("eyes closed", "motion blur");
nothing is deleted; the photographer decides — identical to hard rule 10's
language discipline.
