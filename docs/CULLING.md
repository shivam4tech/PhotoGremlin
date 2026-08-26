# Culling (Sprint 19–21 plan — Aftershoot-style scenario culling)

Status: **planned 2026-08-18, approved.** Research + frozen plan for the
"select a scenario, get the best pictures ranked" workflow. Sprint 17–18
(scene classification) ships first; this builds on it.

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
| Eye-state (blink) | ❌ see below |
| Blur *type* (motion vs defocus) | ❌ synthetic training, $0 |
| Subject-region sharpness | ⚙️ existing sharpness inside YuNet box |
| Smile/expression | ⏳ v0.2 item, pulled into culling scope later |
| Genre profiles | ⚙️ config only |
| Personalization | ⚙️ v2: learn from user's own marks |

## Eye-state licensing decision (researched 2026-08-18)

CEW and RT-BENE are research-only (unusable commercially). Viable paths:

- **A (preferred): OCEC pretrained ONNX model** (PINTO model zoo) — MIT
  licensed, same integration pattern as YuNet.
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

## Sprint map

- **19 — cull engine**: models (blur-type CNN, eye-state via A/B) + Rust core:
  group → weighted score → rank → buckets → write ratings/labels (never
  delete). Genre profiles as config.
- **20 — cull review UI**: genre picker, strictness sliders, survey mode,
  side-by-side swap.
- **21 (v2) — personalization**: tiny local model trained on the user's own
  flags/ratings; no external corpus.

## Philosophy compliance

We suggest and rank; reasons are measurable ("eyes closed", "motion blur");
nothing is deleted; the photographer decides — identical to hard rule 10's
language discipline.
