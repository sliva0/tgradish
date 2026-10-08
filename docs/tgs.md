# TGS plan: pixel art animations to Telegram animated stickers

Written 2026-10-08 in a planning session in `../pixelart2tgs`, for whoever
builds this in the tgradish repo. The overall roadmap is in `docs/PLAN.md`.

## Goal

tgradish 2.0 makes `.webm` video stickers. This track adds `.tgs` animated
stickers (gzipped Lottie JSON) made from pixel art animations with
transparent backgrounds. It replaces the old Python tool
[pixelart2tgs](https://github.com/sliva0/pixelart2tgs), which is now merged
into tgradish. The GUI in this repo covers both conversions, and the `.tgs`
side uses the same CLI protocol as the `.webm` side.

What matters most, in order:

1. **Effectiveness:** the bigger and longer the animation that still fits
   in one valid sticker, the better. Go as far as you can.
2. **Quality:** pixel-exact by default, and it must look good in every
   Telegram client: no seams, no fringes.
3. **Speed:** nice to have. Spending seconds, even a minute at the highest
   effort, is fine if it buys size.

There is no limit on the input size. Large art (256x256 and up) with a short
animation should work as long as the result fits the byte budget. Nothing
from pixelart2tgs 1.x needs to stay compatible: CLI, pip package, output
structure, all can go.

## Decisions already made with the user

- Merged into the tgradish repo. The GUI is part of this repo too (decided
  after this plan was written, see `docs/PLAN.md`).
- Lossless output is the default whenever it fits. When it doesn't, lossy
  reductions are allowed, but the result must clearly say it is lossy and
  what was lost (CLI text and `--json` events).
- The 1 MB limit on uncompressed JSON in 1.x was a guess. Use the real
  limits below.
- The user will upload test stickers to Telegram and check them on iOS when
  asked (step T9).

## Reminders for the user

The agent must remind the user of these at the right time:

- **Before T4 (encoder work):** `references/pixelart/` already holds the
  user's 2022 Deltarune set: 60 inputs plus the `.tgs` files 1.x made and
  Telegram accepted; see its README. Current 1.x fits all of them under 64
  KB, so ask the user for more files covering the gaps the README lists:
  - files too big for 1.x, large sprites, long busy animations;
  - palette cycling and dithering;
  - idle loops with mostly static pixels;
  - APNG, partial alpha;
  - more Aseprite files.
- **T9:** ask the user to upload probe stickers through @Stickers and check
  them on Android, Desktop, iOS and web.
- **After release:** ask before changing the sliva0/pixelart2tgs README to
  point at tgradish (outward-facing).

## Telegram requirements

From <https://core.telegram.org/stickers> and
<https://core.telegram.org/animated_stickers> (checked 2026-10-08):

- canvas exactly 512x512, objects must not leave the canvas;
- 60 fps;
- at most 3 seconds (`op - ip <= 180` at 60 fps);
- at most 64 KB file size (the gzipped `.tgs`);
- must loop;
- must not use: auto-bezier keys, expressions, masks, mattes, layer effects,
  images, solids, texts, 3D layers, **merge paths**, star shapes, gradient
  strokes, repeaters, time stretching, time remapping, auto-oriented layers.

1.x put a merge paths item (`"ty":"mm"`) in every group and Telegram
accepted it anyway, so the server doesn't check everything on that list.
The stickers in `references/pixelart/1x-uploaded/` were accepted in 2022
with merge paths, strokes, no `"tgs":1` key and non-integer `op` values
like 39.6.
T9 found (see `docs/probes.md`): the 3 seconds are counted in seconds,
not frames, and 30 fps is allowed too; custom emoji are 512x512 like
stickers, and a 100x100 one is refused. Telegram also refuses stickers
with too much of something, layers, rectangles or JSON, that the rules
don't mention; the second round of probes finds what.

### Limits in the clients

These were found in client source code and are stricter than the docs in
places. Treat them as hard limits and check every output against them:

- **Telegram Desktop** rejects Lottie content over 2 MiB
  (`kMaxFileSize = 2 * 1024 * 1024` in desktop-app/lib_lottie
  `lottie/lottie_common.h`, checked in `ContentError` in
  `lottie_animation.cpp`). The gzip unpacker in lib_ui allows 5 MiB. Use 2
  MiB of uncompressed JSON as the limit, with some margin. This matters:
  rectangle-heavy output compresses about 17:1, so a 64 KB `.tgs` is about
  1.1 MB of JSON, and better compression pushes closer to the limit.
- **tlottie** (see renderers) has default parse limits that Telegram
  Android doesn't override (`TMessagesProj/jni/lottie.cpp` passes no
  limits). The relevant ones, from `src/composition/limits.rs`:
  - 2 720 layers in total, 2 715 shape layers with paint;
  - 20 480 shape elements per layer;
  - 5 120 paints (fills/strokes) per layer;
  - 5 120 cumulative geometry items observed by paints per layer (in
    practice: rectangles or paths feeding fills in one layer);
  - 4 355 points per path, coordinates within ±165 389;
  - 2 048 keyframes per animated property;
  - 256 assets, 18 005 expanded precomp references;
  - nesting depth 175, input 16 MiB.

  Big inputs will hit the per-layer limits, so the encoder must split work
  across layers (see the seam invariant for how to split safely).

## Renderers

Which renderer draws stickers matters, because they differ and 1.x had
seams that looked different on desktop and mobile.

As of 2026-10-08:

- **tlottie** (<https://github.com/dkaraush/tlottie>): Rust, MIT, zero
  dependencies, WASM support, written by a Telegram developer. Not on
  crates.io, git only. Used by Telegram Android (submodule in
  DrKLO/Telegram `TMessagesProj/jni/tlottie`), Telegram Desktop (lib_lottie
  links `external_tlottie`), Web A (`src/lib/tlottie` in Ajaxy/telegram-tt)
  and Web K (morethanwords/tweb has a tlottie worker and still ships
  rlottie-wasm).
- **rlottie** (Samsung/rlottie, Telegram fork TelegramMessenger/rlottie):
  what all clients used before tlottie; older clients and third-party apps
  still do.
- **iOS** (TelegramMessenger/Telegram-iOS) has rlottie, LottieCpp
  (ali-fareed/lottiecpp) and lottie-ios submodules. Which one draws
  stickers was not checked; the user checks iOS by hand in T9.

Differences found so far: a path without the `i`/`o` tangent arrays renders
correctly in tlottie and wrong in rlottie. 1.x writes empty tangents
(`"i":[[],[],...]`), and both accept that. Every output decision must be
tested in at least tlottie and rlottie.

## What 1.x does (for reference)

The Python source is in `../pixelart2tgs/src/pixelart2tgs/` (git `master`).

1. Each frame is split into 4-connected regions of one colour, and each
   region is normalised to its top-left corner, so identical shapes compare
   equal.
2. Equal shapes are chained across frames greedily (closest position, same
   colour preferred). Each chain becomes one group with hold keyframes for
   position, colour and opacity.
3. Each group holds the contour paths, a merge paths item, a 50% opaque
   stroke 0.5/scale wide (meant to hide seams), a fill and a transform.
4. One layer scales everything to 512 px. Output is `gzip -9`, with
   warnings at 64 KB and at 1 MB of raw JSON. Animations over 3 s are sped
   up.

Problems: lots of overhead per group, one ring-shaped path for every
outline (the outer contour plus every hole inside it), seams despite the
stroke, merge paths, and no detection of upscaled input.

## Measurements from the planning session

Prototypes are in `references/tgs-prototype/`: throwaway Python run with
`uv` (numpy, pillow, scipy, rlottie-python, zopfli). The tlottie CLI path in
`verify_tl.py`/`seams.py` points to a build in `/tmp` that may be gone.
Every row below was checked pixel-exact at pixel centres in both rlottie
and tlottie.

`ralsei.gif`, which is `references/pixelart/Ralsei_battle_start.gif`
(detected as 2x upscaled, so encoded as 48x47):

| encoding | gzip -9 | zopfli | raw JSON |
| --- | --- | --- | --- |
| 1.x (current tool) | 11 402 | | 173 628 |
| each frame separately, one path per same-colour region | 12 842 | 11 063 | 139 376 |
| each frame separately, painter's layers, paths | 8 354 | 7 116 | 86 834 |
| painter's layers, rectangles | 6 165 | 5 670 | 95 908 |
| rectangles kept alive across frames (layer `ip`/`op`) | 6 067 | 5 576 | 99 903 |

Takeaways:

- Painter's layers save about a third (contour vertices 6 462 → 4 596).
- Rectangles beat paths by 20-26% compressed, even though the JSON is
  bigger.
- zopfli beats `gzip -9` by 8-15%.
- Everything together is about half the size of 1.x. Keeping things alive
  across frames barely helped here only because this animation is a dance
  where almost every pixel changes. On idle loops it should matter a lot.

Seams, measured by `seams.py`:
- rendered at 100, 160, 237 and 512 px, frames 0 and 4;
- composited over magenta and compared with an ideal area-averaged render;
- a "leak" is a pixel fully inside the sprite where the rendered alpha is
  below 0.9;
- a "fringe" is a pixel inside the sprite with a channel off by more than
  40.

| encoding | rlottie leaks / fringe | tlottie leaks / fringe |
| --- | --- | --- |
| 1.x | 10 144 / 4 789 | 9 963 / 5 623 |
| each frame separately, one path per region | 10 485 / 8 077 | 10 293 / 8 672 |
| painter's layers, rectangles | 0 / 593 | 0 / 571 |
| rectangles kept alive across frames | 160 / 1 026 | 159 / 1 025 |

The last row is the lesson: one colour split into separately drawn pieces
brings seams back. That is what the seam invariant below prevents.

Also verified in both renderers: fill without `o`, empty transform
`{"ty":"tr"}`, layers without `ind`/`nm`/`ddd`/`sr`/`ao`, a top level
without `nm`/`assets`/`ddd`, fractional `ip`/`op`, and colours rounded to 2
decimals. Not verified yet: dropping `st` from layers or `r` from
rectangles, and adding `"tgs":1` (Telegram's exporter writes it; 1.x never
did).

## Layout

```
crates/
  core/    tgradish-core   existing: WebM, ffmpeg, presets, protocol
  tgs/     tgradish-tgs    new: frames in, .tgs bytes out
  frames/  tgradish-frames new: RGBA animation type and pure-Rust decoders
  cli/     tgradish        existing: gains .tgs conversion
  tgs-lab/ (publish = false) render checks, seam checks, corpus benchmark
xtask/     existing
```

- **`tgradish-tgs`** must not depend on ffmpeg, processes, the filesystem
  or `tgradish-core`, and must build for `wasm32-unknown-unknown` (add a
  CI check). Watermark text is passed in by the caller. Threads (rayon or
  similar) only behind a feature so WASM still works.
- **`tgradish-frames`** holds the shared animation type (RGBA frames plus
  durations) and the pure-Rust decoders. The WebM side can use it later,
  for example to feed Aseprite files to ffmpeg as raw video. If it stays
  tiny, folding it into `tgs` is fine.
- **`tgs-lab`** depends on tlottie (git) and, with its `rlottie` feature,
  on the `rlottie` crate (0.5.4) built from Telegram's fork
  (`vendor-telegram`, needs git, cmake and libclang; a system librlottie
  found by pkg-config takes precedence). Keep tlottie out of publishable
  crates' normal dependencies, since it isn't on crates.io.
- **CLI:** one binary. `.tgs` output is chosen with `--format tgs` or an
  `-o` ending in `.tgs`; fit this into the CLI as it exists by then.
  `.tgs` has its own options (the WebM options mostly don't apply), so
  `describe` must expose both formats with their own option schemas,
  presets and events. Change the protocol as needed and bump
  `PROTOCOL_VERSION`; no GUI exists yet, so breaking it is fine. Update
  `docs/protocol.md`.
- `tgradish inspect` should also accept `.tgs`. It shows canvas, fps,
  duration, file and raw size, layer/shape/keyframe counts and features
  used, plus issues against Telegram's rules, the 2 MiB limit and tlottie's
  limits. This is useful for any `.tgs`, not just ours.

Reuse what's already in the workspace (clap, serde, schemars, thiserror,
flate2, the presets and config code) instead of adding parallel versions.

## Pipeline

```
decode       file → Animation (RGBA frames + durations)
normalise    → PixelAnim (palette-indexed frames on the 60 fps grid)
reduce       → PixelAnim (lossy, only when fitting needs it; every step recorded)
encode       → Scene (z-ordered primitives with lifetimes)
lay out      → Lottie model (layers, groups, within limits)
serialise    → compact JSON → zopfli gzip → .tgs
check        → Telegram rules, 2 MiB raw limit, tlottie limits
```

### Decode

- GIF, APNG, animated WebP: the `image` crate's animation decoders (they
  handle frame disposal and blending). Treat GIF delays of 0-1 cs as 100
  ms, the way browsers do. Files whose frames all say 0 get 100 ms too.
- Aseprite: our own parser and renderer (`frames/src/aseprite`). asefile
  0.3.8 was used first, but it panics on malformed files (31 explicit
  panics and unchecked indexing), ignores tile flips and is unmaintained,
  and files will come from untrusted uploads in the web app. Ours checks
  every read, renders every blend mode (ported from asefile's port of
  Aseprite's `blend_funcs.cpp`), groups composited separately when the
  file asks for it, linked cels, per-cel z-index and tilemaps, and refuses
  flipped tiles rather than drawing them wrong. A tag chooses which loop
  to export; ping-pong loops don't repeat their end frames.
- Decoding has limits (`frames::Limits`: the largest side and a byte
  budget for all decoded pixels), so hostile files fail instead of
  exhausting memory. The web app can set them lower.
- PNG sprite sheets (columns and rows, optional frame count, trailing
  transparent cells dropped) and image sequences (the CLI sorts the files).
- Video and anything else (2.x): decode with `tgradish-core`'s ffmpeg to
  raw RGBA, then use the pixelate mode (see Later).

### Normalise

- **Detect the native pixel scale.** Start with the GCD of all same-colour
  run lengths across frames, then make it robust: uneven nearest-neighbour
  scales (for example 2.5x), a grid offset, and objects moved by less than
  one art pixel. Fall back to 1 when unsure, and report the detected
  scale.

  Done (T2) as an exact cell grid: columns that equal their left
  neighbour in every row of every frame join its cell, and the same for
  rows. That is lossless and covers integer and uneven scales, grid
  offsets and art pixels cut by the canvas; the encoder works on cells and
  the serialiser maps cell edges back to input pixels. The reported scale
  is the GCD of the inner cell sizes. When it is smaller than a scale most
  colour edges fit (80% or more), that one is reported as likely, and
  forcing it with the pixel scale option snaps the rest to its grid
  (lossy, counted in the report). Snapping is also a candidate for the
  first fit reduction. In the test set:
  - 22 GIFs are exact 2x and `animation_hammer.gif` is exact 4x, cut
    through an art pixel on the right. The README's "probably 2x"
    `Ralsei_battle_item`, `Spamton_battle_head_enlarge` and
    `Susie_battle_act` are exact once cropped;
  - `Noelle_battle_act` and `Spamton_overworld_glitched_laugh` are 2x with
    a handful of edges off the grid (99.98% fit), `Spamton_trembling` 88%;
  - the rest are 1x, many with a few columns or rows that join.
- Crop to the bounding box of opaque pixels over all frames (option: keep
  the original canvas).
- Merge identical consecutive frames into one with the summed duration.
- Snap frame times to whole 60 fps frames with error diffusion, so the
  total length stays right and `ip`/`op` are integers.
- Longer than 3 s: speed up (1.x behaviour, default) or trim, plus
  start/length options like the WebM side.
- Alpha: GIF alpha is on/off. Partial alpha (APNG, WebP) becomes fill
  opacity; each distinct (rgb, alpha) is one palette entry.
- Warn when the input doesn't look like pixel art (no pixel grid, hundreds
  of colours); it will work but be large.

### Encode: the model

Treat the animation as a 3D block of coloured voxels (x, y, frame). Cover it
with z-ordered primitives, each with a colour, a geometry and a lifetime,
so that at every pixel and frame the topmost primitive has the right
colour. All geometry is in art pixel units on whole-pixel boundaries, and
one layer transform scales it to the 512 canvas.

Keep an internal pixel-level rasteriser for `Scene`, so most correctness
tests don't need a Lottie renderer. The invariant: rasterising the scene
reproduces every normalised frame exactly.

**Painter's layers.** Colours are drawn in a chosen order. A colour's
shape `S` must contain its own pixels `P`, must not cover transparent
pixels or colours drawn earlier, and may cover pixels of colours drawn
later, which paint over it. So `P ⊆ S ⊆ P ∪ later`. Outlines become solid
silhouettes and holes disappear.

**Seam invariant (the most important rule).** In every frame, for every
pair of 8-adjacent opaque pixels whose visible paint groups differ, the
lower of the two groups must also cover the other pixel. A paint group is
one fill with its geometry in one group, which renderers rasterise as one
path.

Why it works:
- every anti-aliased inner edge is then drawn over a fully opaque, correct
  colour, so it blends only the two right colours;
- only the outer silhouette blends with transparency, and that is correct;
- it doesn't depend on any renderer's coverage maths;
- it covers seams between colours, seams between separately drawn pieces of
  one colour (the 160 leaks above) and corners.

It replaces 1.x's stroke trick. Painter's layers make it nearly free: the
"don't care" pixels of later colours are exactly where lower shapes need to
reach. Check the invariant in code (debug assertion plus tests) on every
`Scene`.

**Primitives.**
- Rectangles grouped under one fill per colour, as a partition: no
  overlaps. (The plan first said overlaps were fine because one non-zero
  path has no inner seams. They aren't: both renderers anti-alias a path by
  adding up the coverage of its parts, so pixels on an outer edge two
  rectangles share get double coverage. Abutting rectangles are fine, their
  shared edges cancel.) Start with greedy maximal rectangles, then improve
  the cover heuristic.
- Rectilinear paths (with empty tangents) where they come out cheaper.
- To try: 1 px strokes along pixel centres for straight runs; the even-odd
  fill rule for dithering and checkerboards (a checkerboard is O(n) XORed
  stripes instead of O(n²) squares). Choose per shape by measured cost.

**Time.**
- Lifetimes use layer `ip`/`op`, the cheapest way to show and hide
  something.
- Content that doesn't change over a range of frames is stored once.
- Splitting a colour by lifetime must keep the seam invariant (overlap by a
  pixel, or keep the pieces in one group).

**Optimiser.** These choices form one search problem:
- colour order (global, maybe changed per range);
- how far each shape grows into later colours' pixels;
- the rectangle cover;
- keeping something alive vs redrawing it;
- later, motion.

Fit a cost model to real compressed sizes from the corpus (marginal bytes
per rectangle, per path vertex, per group, per layer, per keyframe). Start
greedy, then run local search or simulated annealing under the effort
budget. Score the final few candidates with the real compressor. Trying
every colour order is only possible for about 7 colours or fewer; above
that use greedy plus swaps. Everything must stay roughly linear in pixels ×
frames at low effort so large inputs work.

### Encode: ideas for v2

Measure each idea on the corpus and keep only what wins:

- Motion: find blocks that move between frames by matching them, the way
  video codecs find motion vectors. Encode them as group position hold
  keyframes, with integer offsets so they stay on the pixel grid. Whole
  sprite bobbing becomes one position keyframe track on the layer (or a
  parent null layer).
- Precomps (assets + `ty:0` layers) for sprites repeated at several
  positions or times. Watch tlottie's asset and expansion limits.
- Palette cycling: the same geometry with fill colour hold keyframes.
- Per-frame colour order overrides.
- Probing Telegram's validator for a way past 3 s (`fr`/`op` tricks), the
  TGS version of the WebM duration spoof. T9 found none: Telegram counts
  seconds, refusing both 360 frames at 60 fps and 180 at 30 fps.

### Lay out and serialise

- Layers are listed top first. Each layer is a shape layer (`ty:4`) with
  `ip`/`op`, the scale/centre transform (identical in every layer, so it
  compresses to almost nothing, or a parent null; measure which) and its
  groups. Each group is `[geometry..., fill, {"ty":"tr"}]`.
- Respect tlottie's per-layer limits by splitting into more layers, while
  keeping the seam invariant.
- Only write fields every renderer needs. Each omission must pass both
  renderers in `tgs-lab` and the T9 upload.
- Numbers: integers wherever possible; colours as the shortest decimal
  that rounds back to the same 8-bit value. Avoid `-0` and exponents.
  Rectangle centres are `.5` for odd sizes; measure the alternatives
  (doubled units, 4-vertex paths for odd sizes, a group anchor).
- Order output for deflate's 32 KB window: a stable key order, primitives
  sorted the same way everywhere, so unchanged content produces identical
  byte runs close together.
- Compress with the `zopfli` crate (0.8.3, pure Rust, gzip output);
  iterations depend on effort.
- Watermark: top-level `nm` with tgradish's signature (1.x had a `--label`
  option for this). The user also likes marks that are harder to strip
  (WebM got some); any hidden mark must pass both renderers and T9.
- Final check: ≤ 64 KB, raw ≤ 2 MiB (with margin), tlottie limits, ≤ 180
  frames, no forbidden features.

### Fit (when lossless doesn't fit)

Same spirit as `--fit` on the WebM side. Suggested modes:

- `auto` (default): lossless if it fits; otherwise apply the least visible
  reductions until it fits, and report them;
- `lossless`: never reduce; report the size issue instead;
- `off`: one encode with whatever reductions the options ask for.

Reductions, roughly from least to most visible:

1. merge frames that differ in only a few pixels;
2. keep small flickering regions static;
3. merge perceptually close colours (distance in OKLab);
4. drop frames, giving their time to neighbours;
5. remove isolated single pixels;
6. downscale (pixel-art aware) as a last resort;
7. trim, only if the options allow it.

Measure loss as a perceptual error weighted by frame duration. Greedily
pick the step with the best bytes saved per unit of error, and bisect the
strength of the last step. Report every applied step in a `finished` event
(`lossy: true` plus the list of steps and their sizes) and as a visible
warning in text mode. Fitting must also respect the 2 MiB raw limit, which
may favour paths over rectangles for very large outputs.

Options to expose (names should fit the WebM side; reuse `speed`
fast/balanced/best as the effort setting): target (sticker/emoji), fit, the
reduction toggles and limits, crop, behaviour for long inputs, start and
length, the pixel scale override, and the watermark.

## Testing

- **Unit and property tests in `tgs`.** Generate synthetic animations:
  random sprites, outlines, dithering, palette cycling, bobbing, large
  canvases, many colours. Check that encoding then rasterising the `Scene`
  reproduces the frames exactly, that the seam invariant holds, that the
  limits hold and that the JSON parses.
- **`tgs-lab verify`** renders with tlottie in-process and rlottie when
  available:
  - pixel centres must match exactly;
  - the seam check as in `seams.py` (several sizes, over magenta, against
    an area-averaged ideal) must find 0 leaks and fringes at about the
    anti-aliasing noise level.

  Run the tlottie checks in normal CI on Linux and Windows, and rlottie in
  a Linux job (`rlottie` in `ci.yml`). ThorVG and lottie-web are optional
  extras.
- **`tgs-lab bench`** runs the corpus through the encoder and prints a
  table of compressed size, raw size, time, losses and render time
  (tlottie). It compares against a saved baseline, so every algorithm
  change is measured. The starting baseline is the "1.x now" column in
  `references/pixelart/README.md` (pixelart2tgs `master`, `gzip -9`).
  `1x-uploaded/` has what an older 1.x produced.
- Corpus files and tests that need them live in `references/` and skip when
  it's missing, like the WebM tests.

## Milestones

This is one track of the roadmap in `docs/PLAN.md`; 2.0.0 waits for it.
The web app there may also replace the "Web page" and "Bot" items under
"Later".

- **T1 (done):** move this plan to `docs/tgs.md` and link it from
  `docs/PLAN.md`. Create `tgs`, `frames` and `tgs-lab` (`info` and `render`
  through tlottie) and add the WASM check to CI.
- **T2 (done):** `frames`: decoders. `tgs`: normalisation (scale
  detection, crop, timing, alpha); `tgs-lab normalise` prints the report.
- **T3 (done):** Lottie model, serialiser, zopfli, limits checker, `Scene`
  rasteriser, seam invariant checker; `tgs-lab verify`. **Ask the user for
  more test files now** (see Reminders; asked 2026-10-08).

  Notes from T3:
  - The writer's defaults are the fields 1.x's accepted stickers and the
    prototypes had: no `"a"` in properties, `"st":0` on layers,
    `"r":{"k":0}` on rectangles, paths in 1.x's form (first point
    repeated, empty tangents), no `"tgs":1`. `lottie::Style` switches the
    optional ones for T9.
  - tlottie and rlottie truncate colour and opacity to 8 bits, so each is
    written as the shortest decimal in `[v + 0.02, v + 0.98] / 255`
    (at most 3 decimals). Every cell centre then matches exactly in both
    renderers, translucent colours included.
  - Layout: Lottie units are art pixels, counted from where the art pixel
    grid starts, so inner edges are integers even when the canvas cuts art
    pixels; the longer side fills the canvas, centred.
  - `check` counts shapes, paints and painted geometry like tlottie's
    parser; it runs on any Lottie, for `inspect` later.
  - `encode::runs` (a layer per frame, a group per colour, a rectangle per
    run) is the baseline: exact at cell centres, but it leaks at seams
    (about 64 000 leaking pixels over all frames at 4 sizes for
    `Ralsei_battle_start.gif`). A hand-made scene that keeps the seam
    invariant has 0 leaks and 0 fringes in both renderers.
  - `tgs-lab verify` computes the ideal render from exact pixel overlaps
    instead of the prototype's 8x8 supersampling.
- **T4 (done):** encoder v1: painter's layers + seam invariant + rectangle
  covers + lifetimes. Target: at least 2x the content of 1.x across the
  corpus, 0 leaks in both renderers.

  Notes from T4 (`encode::painter`, measured with `tgs-lab bench`):
  - One global colour order. A colour's shape must hold its cells and
    every 8-neighbour of a later opaque colour, and may hold any other cell
    of a later opaque colour. Covers are partitions (see Primitives).
  - Lifetimes: a colour keeps one shape while one shape fits every frame,
    so every frame still has one group per colour. Pieces with equal
    lifetimes share layers when nothing in between is shown at the same
    time. Worth 12%.
  - The order: a colour's cost depends only on the set of colours above
    it, so the cheapest order (by rectangles and groups) is exact over
    subsets up to 10 colours, greedy up to 256. Worth 4% over larger
    bounding boxes first. Rectangle count is a weak proxy for compressed
    bytes: covering column by column gave fewer rectangles and bigger
    files.
  - Format: coordinates start half an art pixel early so odd sizes (most
    rectangles are 1 wide or high) have whole centres: 0.8%. Half-pixel
    units and putting `"s"` before `"p"` were worse. Leaving out `"st"`
    and `"r"` saves 6% and is the default; T9 confirmed Telegram accepts
    it.
  - Result: 345 404 bytes for the corpus against 691 738 for 1.x
    (zopfli against 1.x's `gzip -9`): 2.00x. Every file matches at cell
    centres in tlottie and rlottie, with 0 leaks.
  - Fringes remain, all from conflation: anti-aliasing treats coverage as
    alpha, so a colour drawn between the two colours meeting at an edge
    shows through that edge's pixels when its hidden part ends there,
    by up to a quarter of the colour difference. Extensions the seam
    invariant forces cause most of them. Letting shapes reach freely
    under later colours gave 16x fewer fringes on `Ralsei_battle_start`
    than reaching only away from uncoverable cells (55 against 904 on
    frames 0 and 4; the prototype had 571), and 12% more over the corpus.
    T5 should weigh fringes in the order search and keep hidden edges off
    visible ones where it costs nothing.
- **T5 (done):** `tgs-lab bench`, cost model, optimiser.

  Notes from T5:
  - `tgs-lab bench` prints sizes against 1.x and saves runs to compare
    later ones with (`--save`, `--against`).
  - A linear cost model fitted to the corpus (bytes per rectangle, group
    and layer) was useless: 28% mean error, since groups and rectangles
    grow together. So the optimiser compares candidates by their real
    `gzip -9` size (`file::quick_size`), which ranks like zopfli.
  - Effort levels, named like the WebM side's speed: `fast` (larger
    colours first), `balanced` (the subset search), `best` (balanced,
    then neighbouring colours swapped while the real size shrinks, at most
    200 tries). Best is 2.4% smaller than balanced, about 2.05x smaller
    than 1.x over the corpus, at up to 2.4 s for the largest file.
  - Order: T7 and T8 come before T6, so `.tgs` works end to end (and the
    GUI has its protocol) before the open-ended encoder experiments, which
    are then measured through the whole pipeline.
- **T6 (done):** encoder v2 experiments: motion, precomps, palette
  cycling, mixing primitives, even-odd, strokes. Keep what the bench shows
  is better.

  Measured on the corpus first: only 21 of 645 frames are a shifted copy
  of an earlier frame (17 of them in `Spamton_trembling`), so whole-sprite
  motion is a niche; but 70% of visible cells keep their colour from one
  frame to the next, and 30% (median 15%, up to 93%) for the whole loop.

  Kept: splitting a colour into a core and deltas (`Settings::split`). The
  core draws the cells that keep the colour over a stretch of frames, once;
  deltas below it draw the rest. A delta must reach under the core where
  they meet; the core can only keep a cell whose later-colour neighbours
  are its own or hidden under later colours for the whole stretch, and may
  only reach under cells hidden for the whole stretch (otherwise it shows
  cells it doesn't guard). A colour is split when that is cheaper counting
  a layer per delta. 2.7% smaller over the corpus (20% on idle-ish files
  like `pizza_dude`), now 2.06x smaller than 1.x at balanced effort.

  Tried and dropped: two tiers, every colour's cores above every colour's
  deltas, so the deltas would share each frame's layer again (splitting
  per colour adds 76% layers, which eats most of the 8% fewer
  rectangles). 22% larger over the corpus: deltas must reach under every
  core and cores can't reach under deltas, so rectangles grew 14%, and
  with every colour split, layers grew 20% instead of shrinking.

  Left for later (see Later): motion as position keyframes, which only
  files like `Spamton_trembling` would gain from; fringe-aware colour
  orders, which are about looks rather than size. Precomps, palette
  cycling and other primitives weren't tried: in this corpus colours
  rarely cycle and sprites rarely repeat, while 70% of cells staying put
  is what the split already uses.
- **T7 (done):** fit and lossy reductions, with reporting.

  Notes from T7 (`reduce`, `sticker::make`):
  - Reductions: snap to the likely pixel grid, merge colours close in
    OKLab, merge frames that barely differ, drop the least different
    frames (never the first, the preview), despeckle, downscale. Trimming
    stays an explicit option of normalising. Each has a ladder of
    strengths; the error of a result is the OKLab distance (plus alpha
    difference) of every input pixel at every 60 fps frame, averaged.
  - Fit: each round tries every kind at its next strength and takes the
    most bytes saved per unit of error; the last step is weakened by
    bisection when it overshoots by more than 10%. Estimates are fast
    encodes scaled by how the real effort and zopfli did on the original;
    if the real result still doesn't fit, the target drops 4% and fitting
    goes on.
  - `fit: lossless` never reduces and reports the size instead. The
    plan's `off` mode isn't needed: reductions only run when asked to fit.
  - susie_fortnite tiled 2x2 (77 KB lossless) fits at 62 KB after
    despeckling and dropping a tenth of the frames, in 5.5 s.
- **T8 (done):** CLI: `convert` to `.tgs`, `describe`/protocol, presets (sticker
  and emoji for `.tgs`), `inspect` for `.tgs`.

  Notes from T8:
  - `tgradish-core::tgs` holds the `.tgs` options (`TgsOptions`, layered
    like WebM's), events and file handling (inputs, sequences with numbers
    sorted as numbers, sheets, atomic output); `tgradish-tgs` gained a
    `schema` feature for the JSON Schemas and cancellation between steps.
  - Format: `--format`, else `-o`'s extension, else the preset's, else
    webm. Shared flags where the meaning matches (target, start, length,
    speed as effort, title, watermark; `--lossless` means "never reduce";
    `--fps` is the frame rate of sheets and sequences); flags of the other
    format are an error. New: `--long`, `--reductions`, `--keep-canvas`,
    `--pixel-scale`, `--tag`, `--sheet`, `--sheet-frames`, `--sequence`.
  - Presets have a format (`tgs-sticker`, `tgs-emoji`, `tgs-fast` built
    in; files set `format` or take their base's); `config.toml` has
    `tgs-preset`. Protocol 2 describes both formats, see
    `docs/protocol.md`. `inspect` reads `.tgs` with `check`.
  - The default effort for `.tgs` is best: seconds, for the smallest
    stickers. The sticker's name is "made with tgradish VERSION", or the
    title with that added; `--watermark=false` leaves only the title.
  - Emoji use the same 512x512 canvas as stickers, which T9 confirmed is
    what Telegram wants.
- **T9:** Telegram probes. Generate a set of `.tgs` files for the user to
  upload through @Stickers:
  - minimal JSON without optional fields;
  - with and without `"tgs":1`;
  - raw size near 2 MiB;
  - many layers and rectangles near tlottie's limits;
  - precomps, keyframes and the 3 s tricks;
  - an emoji.

  The user checks acceptance and how they look on Android, Desktop, iOS
  and web. Adjust the encoder to the results.

  First round (`tgs-lab probes`, results in `docs/probes.md`): the
  output, precomps, keyframes, 30 fps and 512x512 emoji are accepted;
  longer than 3 s and 100x100 emoji are refused, and so were the three
  probes with very many layers, rectangles or JSON. Second round
  (`tgs-lab limits`): ladders of each of those and real art, to find the
  limit and make the encoder keep under it. Waiting for the user's
  results.
- **T10:** release as part of tgradish 2.0, which waits for the whole
  roadmap.

Later (2.x):
- **Encoder:** motion as position keyframes for shaking or bobbing
  sprites; colour orders that avoid fringes (see T4's notes); precomps
  for repeated sprites, palette cycling, if a corpus shows them.
- **Pixelate mode:** video or any image (through ffmpeg) to pixel art,
  then to TGS.
- **Web page:** GIF to TGS in the browser. `tgs` is pure Rust, so this is
  cheap, and it could share the page with the WebM spoof demo.
- **Bot:** `.tgs` output in the Telegram bot.
