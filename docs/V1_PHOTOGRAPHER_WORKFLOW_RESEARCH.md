# V1 photographer workflow research

**Status:** product research and recommendation only (2026-08-28).  This
document proposes no feature implementation and makes no change to the offline,
local-first product contract.

## Executive finding

PhotoGremlin has a credible foundation for *interrogating a library*: local
scan, thumbnails, EXIF, deterministic technical measurements, safe operations,
saved views, collections, sessions and similarity groups. Its largest V1 gap is
not another measurement or another image model.

It lacks a **fast, session-first review workflow that turns a shoot into a
small, understood set of deliberate selections.**

The product's potential USP is therefore:

> **Local Shoot Review** — a private, explainable review lane that lets a
> photographer move from an unreviewed shoot to an edit-ready selection in one
> fast pass, without surrendering the final choice to an opaque score.

This is stronger and more defensible than "AI culling." It unites the things
the app already owns locally—session, technical signals, similarity and safe
marks—into a concrete outcome: *I know what I will edit or deliver next.*

It also respects the product rule that photographs are never judged as
artistically good or bad. The product can say "eyes closed", "sharpness 42",
"same capture run", or "high highlight clipping". The photographer chooses
which emotionally important or intentionally blurred frame stays.

## What exists today, and why it feels incomplete

The current Library has valuable ingredients but no primary review lane:

| Existing capability | Present behavior | Workflow consequence |
|---|---|---|
| Session-scoped library | Sessions can open the Library with a `session_id` filter. | Useful context, but it does not begin a guided review. |
| Similar/burst groups | dHash grouping and a fixed three-second burst window present cards to open. | The user must discover groups, inspect them, then return to the grid; a group is not a decision unit. |
| Cull button | Enables manual Keep / Reject marking on individual tiles. | It is a mode switch, not a fast linear pass with progress, comparison and undo. |
| Technical analysis | Whole-image sharpness, brightness, contrast, saturation and clipping are filterable. | Useful evidence, but not framed around the subject or a specific decision. |
| Filters and saved views | An exact, safe structured AND query can be stored dynamically. | Technically sound, but photographers must already know the field, threshold and spelling they need. |
| Safe organization | Selected files can be marked, collected, renamed, moved, copied, trashed or contact-sheeted. | A strong finishing layer that is disconnected from a decisive review flow. |

The result is understandable: the app is good at **finding a set** but not yet
good at **helping a person finish a shoot**. That distinction explains why the
filter system can feel hit-or-miss even when its SQL and data model are sound.

## What photographers say they need

This is directional qualitative research, not a representative survey. It
combines a professional-association article, two working-photographer workflow
write-ups, and a practitioner discussion. Vendor claims are not used as product
truth; the recurring patterns are more useful than any promised accuracy rate.

| Evidence | Practical reading | Product implication |
|---|---|---|
| A wedding photographer writing for the Professional Photographers of America describes rapid next/previous comparison, keyboard marks, focus checks, and a repeatable keep-first pass; she reports roughly halving the post-shoot sorting time. [PPA workflow article](https://www.ppa.com/ppmag/articles/photo-mechanic-power-tips) | Speed is not merely analysis throughput. It is uninterrupted, mouse-light decision cadence. | Review must have keyboard-first keep/reject/skip, instant advance, undo, and a quick comparison view. |
| A wedding/elopement photographer rejected fully automated culling because auditing it erased the time saving. She preferred sequence-level sharpness and eye information while retaining every final selection; one 397-image session became 188 selected images in 10m28s. [Narrative Select workflow](https://adventureweddingacademy.com/narrative-select-workflow/) | Assistance earns trust when it reduces comparison work rather than claims authority. | Show the evidence and candidates in a sequence; never hide a frame or imply that a machine made the final choice. |
| A recent photographer workflow comparison argues that a closed-eye signal cannot, by itself, distinguish a blink from laughter, a kiss, or a glance down; it reports manually reviewing a 10,000-RAW wedding in about two hours. [Jessica Whitaker’s workflow comparison](https://www.jessicawhitaker.co/blog/narrative-select-vs-aftershoot) | Emotion and intention are the irreducible exception cases. Fast human review remains the goal. | Closed-eye, motion blur and technical warnings belong in an assist lane with a visible reason and an easy override, never in an auto-delete lane. |
| In a WeddingPhotography practitioner thread, users report both substantial time savings and cases where automated tools excluded the emotionally important frame or chose the wrong frame in a sequence. [Practitioner discussion](https://www.reddit.com/r/WeddingPhotography/comments/1ednf50/anyone_using_an_auto_culling_software/) | Trust is conditional: photographers will accept an imperfect assistant if it reduces the set they inspect and preserves their control. | Start with conservative triage, keep all originals accessible, and measure whether review time falls—not whether an abstract score looks clever. |

### Repeated workflow pattern

The desirable loop is consistent across the evidence:

```text
copy + back up → open one shoot → make fast keep decisions by sequence
→ compare only real alternatives → apply technical/face signals as evidence
→ review the short unresolved/warning set → hand off selected photos
```

Photographers do not primarily ask a desktop tool to edit pixels for them.
They ask it to eliminate **waiting, hunting, repetitive comparison, context
switching and fear of missing the one important frame**.

## The V1 promise to test with photographer friends

### One-sentence promise

“Open a shoot, move through it at full speed, understand every warning, and
finish with a safe, reversible edit-ready selection—without uploading your
photos anywhere.”

### The moment that should feel like magic

After local analysis, a photographer opens *Review this shoot*. Instead of an
unbounded grid, they see a progress-aware queue of decision units:

1. a sequence or a single ungrouped photograph;
2. two to four candidates in capture order, with measured evidence available
   on demand;
3. one action—Keep, Reject, or Leave undecided—performed by keyboard;
4. immediate advance, undo, and a clear remaining count;
5. a final **Needs attention** queue for groups with ambiguity or technical
   warnings, followed by **Selected** as the safe hand-off set.

No cloud, no automatic deletion, no “best aesthetic photo” claim. Explicit
file removal remains behind the preview and confirmation flow. The magic is
that the boring mechanical work disappears while the photographer remains the
author of every final selection.

## Recommended V1 scope

The right V1 is a narrow vertical slice, not a larger general DAM or a promise
to replicate commercial AI culling products.

### Must ship for a credible friend test

1. **Session-first Review mode**
   - A clear entry from a session: “Review this shoot,” with the session photo
     count and analysis readiness state.
   - Stable review states: unreviewed, kept, rejected, and needs-attention.
   - Progress and resumability: “143 of 397 reviewed; 28 need attention.”
   - Keep/reject/clear, next/previous, and undo available without using a
     mouse. Existing marks stay non-destructive and reversible.

2. **Sequence as the review unit**
   - Reuse existing burst/similar groups, but present them inline in the
     review queue rather than as a separate discovery panel.
   - Give each group a compact compare surface with synchronized zoom only
     when the photographer asks for it.
   - Default to capture order; never silently suppress a member of a group.
   - Use existing technical measurements as labels and optional ordering
     evidence, not a verdict or an auto-selection.

3. **Explainable technical triage**
   - A conservative, opt-in “Needs attention” queue: high clipping, low
     sharpness, or an optional local face/eye signal when available.
   - Every surfaced reason is inspectable and dismissible per image/group.
   - Avoid all “auto-reject” language. Intentional blur, closed eyes during a
     laugh, and imperfect but meaningful photographs must remain easy to keep.

4. **A visible finish line**
   - A persistent summary of reviewed, kept, rejected and unresolved counts.
   - One-click dynamic views/collections for `Kept`, `Rejected`, `Unreviewed`
     and `Needs attention`.
   - A safe next step from `Kept`: add to collection, apply rating/flag/label,
     create a contact sheet, or open the existing rename/move/copy/trash flow.

### Explicitly defer from V1

- A global “quality” or aesthetic score.
- Automatic permanent deletion or hiding rejects by default.
- Genre-specific auto-rejection profiles, eye-state model training, learned
  personalization and automated best-of-group selection.
- Full RAW editing, cloud delivery, client proofing, semantic chat search, or
  a general-purpose project-management layer.

These can follow only after real shoot reviews show where time is actually
lost. The planned culling document remains useful input, but it should be
reframed from “Aftershoot-style scoring” to “fast human review with local
evidence.”

## Why filters feel hit-or-miss

The current filter engine is robust: structured conditions, a single Rust-side
registry, parameterized SQL, dynamic saved views and inclusive date ranges are
all good decisions. The weakness is *interaction and discoverability*, not
query correctness.

| Current limitation | Why it misses for a photographer | Better V1 behavior |
|---|---|---|
| One generic form: field → operator → value → Add | It asks people to translate an intent (“find the high-ISO indoor portraits I have not reviewed”) into a database expression. | Offer task-oriented starting chips and editable natural-language labels: `Unreviewed`, `Kept`, `Potential focus issues`, `High ISO`, `Portraits`, `This shoot`. |
| AND-only conditions | “Wedding + reception + (high ISO **or** flash)” and similar real queries are impossible. | Add a small, visible **Match all / any** group model. Do not expose arbitrary boolean programming first. |
| Exact text fields with no suggestions | A person cannot know the canonical camera, lens or scene string, so zero-result queries feel broken. | Show known values, counts and type-ahead suggestions from the current scope; normalize camera/lens display names. |
| No value distribution or result preview while composing | Thresholds like sharpness ≥ 60 are guesses, and different cameras/scenes have different ranges. | Show a live result count before Apply and a compact histogram/range hint with common pivots such as “lowest 10%” or “highest 10%.” |
| No review-state fields | Keep/reject exists in the library but cannot be used as a documented filter facet. | Treat review status as a first-class facet: unreviewed, kept, rejected, needs attention. This is essential to resuming work. |
| Session is engine-level but not a normal filter control | The library can be session-scoped through the Sessions view, but a user cannot compose a session condition in the filter picker. | Provide a scope control above filters: `All library` / `This shoot` / selected sessions. Keep it separate from query conditions. |
| Static capture-time ascending order | A review task wants capture order; an investigation might need newest, filename, rating, sharpness, or number of faces. | Add explicit sort modes with a clear default per context. Review mode should stay capture order. |
| Saved views start blank | The mechanism is sound, but it makes every user invent their own workflow. | Ship a small editable preset shelf: `Unreviewed`, `Needs attention`, `High ISO`, `No metadata`, `Portraits`, `Recent imports`. |
| Empty results only say “no photographs match” | It does not identify the condition responsible or offer recovery. | Show per-condition result impact, plus “remove the most restrictive condition” and “include photographs not yet analyzed” choices. |

### Filter design principles

1. **Scope before query.** “This shoot” is usually the first choice; it should
   not require constructing a hidden numeric session condition.
2. **Facets before formulae.** Offer values and counts that exist in this
   collection before asking for free-form values or thresholds.
3. **Preview before commitment.** Change the result count as a draft changes;
   make zero results explainable.
4. **Intent labels before metric labels.** “Potential focus issues” can expand
   to a transparent technical condition; the photographer can inspect and
   edit it.
5. **Preserve the advanced builder.** The existing structured builder is the
   expert escape hatch and the saved-view representation. It should remain the
   canonical engine rather than being replaced by fuzzy search.

## V1 acceptance test with photographer friends

Use real, backed-up shoots rather than a random corpus. Ask 4–6 photographers
across at least portrait, wedding/event, and landscape/documentary work to use
the same task:

> “Starting with an unreviewed 300–1,000 photo shoot, create the set you
> would send to editing and find a second useful subset without external help.”

Record only local, consented test notes:

- time to first keep and time to a finished selection;
- number of clicks/keyboard decisions per kept photograph;
- percentage of review actions completed by keyboard;
- how often a suggested warning or group was useful, ignored or misleading;
- whether every photographer can resume an interrupted review unaided;
- the exact words users use when a filter fails them;
- whether they would trust the selection state enough to begin editing.

Success is not an AI accuracy percentage. A successful V1 lets a photographer
review less mechanical material, make their own calls faster, and explain why
the chosen images are there.

## Priority order

1. Make review state and session-first keyboard review real.
2. Bring sequence comparison into that review loop.
3. Make filters discoverable through scope, facets, live feedback and presets.
4. Test the workflow with real shoots and revise the queue.
5. Only then consider eye-state, blur-type or per-genre local intelligence.

That sequence gives PhotoGremlin a focused, privacy-preserving reason to exist
alongside editors and cataloguers: it becomes the calm, fast place where a
photographer decides what their shoot means before any editing begins.
