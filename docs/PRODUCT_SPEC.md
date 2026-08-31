# Product Specification — PhotoGremlin

> A privacy-first local photo intelligence and organization tool for photographers.
> Explore. Understand. Organize. Your photography.

This page distills the product contract. Engineering docs in this folder describe
how it is built; this file describes what the product is and is not.

## The problem

Photographers accumulate huge collections (1,000 to hundreds of thousands of
files). Existing tools are powerful but often complicated, expensive,
cloud-dependent, heavy, editor-centric, and bad at simple organizational tasks.

PhotoGremlin focuses on a narrow problem:

> Help photographers interrogate and organize their own photographs.

The core loop:

```
import folder → analyze locally → filter by measurable traits →
select → rename/move/copy/trash or explicitly delete → save view → read statistics
```

PhotoGremlin does not decide which photographs are good. It gives
photographers information and tools that help them decide.

## Pillars

1. **Explore** — thumbnail grid, viewer, metadata, technical measurements,
   similarity groups, optional local visual intelligence.
2. **Filter** — combine technical (sharpness, brightness, contrast, saturation,
   clipping), visual (monochrome/color/dark/bright), orientation, camera/lens,
   exposure (ISO/aperture/shutter/focal length), date, and local-intelligence
   (faces/smiles) conditions.
3. **Organize** — selection, group rename (templates), move/copy/trash and
   separately confirmed permanent deletion with safety, saved views (dynamic
   filters), collections (curated sets).
4. **Understand** — first-class dashboard: totals, averages, distributions,
   trends, session comparison, camera/lens analytics, selection ratios. The
   dashboard is not an afterthought; the data architecture serves it from day one.

## Three non-negotiable philosophies

1. **Photographers decide.** The software reports measurable characteristics,
   never aesthetic verdicts. No "bad photo". Only "sharpness 62", "high
   highlight clipping", "similar photograph".
2. **Data stays local.** No account, no cloud, no telemetry, no runtime network
   requests. Works fully offline.
3. **AI is optional intelligence, not the foundation.** Every core feature
   (scan, thumbnails, EXIF, analysis, filters, similarity, statistics, file
   operations, collections) works with AI disabled. Optional local models
   (face/smile) are the smallest possible layer.

## Differentiation

Not "we have AI". It is:

```
local analysis + powerful filtering + safe file operations +
similarity + session management + photography statistics +
historical comparisons + privacy
```

The product is a photographer's long-term workspace: each indexed shoot feeds
statistics that can be compared with previous sessions ("your average ISO was
higher in indoor sessions" — never "you're bad at indoor photography").

## What we deliberately do not build

Cloud accounts, authentication, backend API, online database, social features,
sharing, full photo editing, color grading, video, mobile, subscriptions,
ads, telemetry, external AI APIs, huge AI models. A focused desktop utility.

## Success criterion (v0.1)

A photographer opens PhotoGremlin, picks a folder with thousands of photos,
and can: scan it, browse thumbnails, filter (e.g. sharp + portrait + color +
ISO < 1600 + faces), select, rename, move, save the filter as a view, open the
dashboard, and compare two sessions — entirely offline.
