# tgradish 2.0 plan

tgradish makes Telegram stickers and custom emoji: video stickers (`.webm`)
from any video or image, with duration spoofing to get past the 3 second
limit, and animated stickers (`.tgs`) from pixel art. It replaces tgradish
1.x and pixelart2tgs 1.x, which merges into it.

2.0 is a from-scratch Rust rewrite. Nothing is compatible with 1.x: the CLI,
config format and packages are all new. The Python 1.x code lives on
`master` and in the `rewrite-attempt` branch for reference only.

**Release rule:** nothing is merged into `master`, tagged or released until
the whole roadmap below is done: GUI, `.tgs`, packaging and the web app
included. Until then everything happens on `rewrite-v2`.

## Layout

```
crates/
  core/     tgradish-core    WebM, ffmpeg backends, presets, protocol
  cli/      tgradish         the binary: CLI now, GUI later (see GUI)
  tgs/      tgradish-tgs     planned: frames in, .tgs bytes out (docs/tgs.md)
  frames/   tgradish-frames  planned: RGBA animations, pure-Rust decoders
  gui/      planned, if the GUI lives in its own crate linked into the binary
  tgs-lab/  planned, publish = false: render and seam checks, benchmark
xtask/      ffmpeg build, packaging
docs/       plan, protocol, ffmpeg, tgs, web app
```

Front-ends can integrate in two ways, and both stay supported: link the
library crates, or run the binary and use the JSON protocol
(`docs/protocol.md`). The GUI in this repo links the libraries directly;
the protocol is for scripts and other tools.

## Status

Done on `rewrite-v2`, with CI on Linux and Windows:

- WebM inspection and metadata patching (spoofing, watermarks);
- conversion engine: size fitting (`--fit auto` with SSIM scoring),
  progress events, cancellation;
- ffmpeg as separate executables or linked in (`linked` feature), with a
  minimal static ffmpeg built by `cargo xtask ffmpeg`;
- CLI: `convert`, `watch`, `--clipboard`, `spoof`, `inspect`, `describe`,
  presets, config, `ffmpeg status/download/remove`;
- release workflow: single-file binaries with ffmpeg linked in for Linux
  x86-64 (16.8 MB) and Windows x86-64 (18.8 MB), tested on both.

## Roadmap

Order is a proposal; tracks can overlap.

### 1. GUI

- egui (eframe), in this repo, linked against the libraries rather than
  driving the CLI.
- One binary that is both CLI and GUI, see "Shipping one binary" below.
- Forms are generated from the options' JSON Schema, so `.webm` and `.tgs`
  options show up without hand-written UI for each.
- Features to settle: file drop and paste, presets, progress per attempt,
  preview of the result (looping, over a checkerboard for transparency),
  batch queue, `inspect` view, settings (ffmpeg source, config).
- Prefer the glow renderer over wgpu: eframe's docs say it is
  significantly smaller. Measure the size it adds to the binary.

### 2. `.tgs` animated stickers

The plan is in `docs/tgs.md` (milestones T1–T10, written in a separate
planning session). It now also feeds the GUI in this repo.

### 3. Distribution

- **Targets:** Linux x86-64 and Windows x86-64 exist. Linux aarch64 and
  Windows ARM64 can be built on GitHub's arm64 runners, which are free for
  public repositories (`ubuntu-24.04-arm`, `windows-11-arm`). Linux aarch64
  is mostly a matrix entry (native build, ffmpeg's and libvpx's ARM
  assembly need no nasm). Windows ARM64 needs an aarch64 MinGW (llvm-mingw)
  for the ffmpeg cross build and the `aarch64-pc-windows-gnullvm` Rust
  target, or a native build on `windows-11-arm`.
- **Old glibc baseline:** the Linux release is built on `ubuntu-22.04` to
  run on older systems, but that runner image is deprecated from
  2026-09-17 and unsupported from 2027-04-17. Build inside an old-glibc
  container instead (for example `manylinux_2_28`, glibc 2.28, which also
  exists for aarch64). A fully static musl build is not an option once the
  GUI is in the binary, since it has to load the system's graphics
  libraries.
- **Linux build without ffmpeg:** for distributions and users who want the
  system ffmpeg. Either the process backend (works with any ffmpeg 6+ on
  `PATH`, no ABI coupling, the right choice for a generic download) or
  linked dynamically against the system's libav* (ties the binary to one
  ffmpeg major version, fine for distribution packages that rebuild).
- **AUR:** `tgradish` built from source (`depends=(ffmpeg)`, linked
  dynamically or using the process backend) and `tgradish-bin` from the
  release binary. The PKGBUILDs can live in this repo; publishing to the
  AUR needs the user's AUR account.
- **Nix:** a `flake.nix` with the package (built against nixpkgs' ffmpeg)
  and a dev shell (Rust, nasm, meson, ninja, clang for bindgen). Submitting
  to nixpkgs can come later.
- **`tgradish ffmpeg download`:** open question, see below.

### 4. Telegram bot and web app

A Telegram Mini App (web UI inside Telegram) plus a bot, and the same page
usable in a normal browser. Findings and options are in `docs/web-app.md`.
This absorbs the earlier "WebAssembly demo page" and "self-hosted bot"
ideas.

### 5. Telegram probes and release

- T9 from `docs/tgs.md`: upload test stickers through @Stickers and check
  them on Android, Desktop, iOS and web; the WebM side gets the same
  treatment (spoofed durations, watermarks, emoji).
- Merge into `master`, remove the temporary release trigger on
  `rewrite-v2`, tag 2.0.0, publish the drafted release.
- After release: ask before pointing the old pixelart2tgs README at
  tgradish.

## Shipping one binary

One `tgradish` binary is both the CLI and the GUI:

- arguments given → CLI;
- no arguments and not started from a terminal (double-click, desktop
  file, Start menu) → GUI;
- no arguments in a terminal → open question, see below;
- `tgradish gui` always opens the GUI.

On Linux, "started from a terminal" means stdin or stdout is a TTY. The
GUI's libraries (X11, Wayland, OpenGL) are loaded at runtime only when the
GUI starts, so the binary still runs headless.

On Windows a program is either a console or a GUI program. Since Windows 11
24H2, a console program with a `consoleAllocationPolicy` of `detached` in
its manifest gets no console window when started from Explorer, but still
behaves like a normal CLI in a terminal ([Microsoft
docs](https://learn.microsoft.com/windows/console/console-allocation-policy)).
On older Windows the same binary briefly shows a console window when
double-clicked, which it can hide right away; a separate `tgradish-gui.exe`
built as a GUI program is the fallback if that is too ugly.

## Open questions

Answers go here as they come.

- Keep `tgradish ffmpeg download`? Release binaries have ffmpeg built in,
  so it only helps source builds without ffmpeg and `--extra-args`.
- In a terminal with no arguments: show the CLI help (mentioning
  `tgradish gui`) or open the GUI?
- Web app: convert in the browser, on a server, or both?
- Which ARM targets: Linux aarch64, Windows ARM64?
- Track order: GUI first, `.tgs` first, or in parallel?

## Reference

### Telegram limits for video stickers

From <https://core.telegram.org/stickers> (checked 2026-10-08), shared by
stickers and emoji unless noted:

- WebM container, VP9 codec, no audio stream;
- sticker: one side exactly 512 px, the other at most 512 px;
- emoji: exactly 100x100 px;
- at most 3 seconds (this is what duration spoofing gets around);
- at most 30 fps;
- at most 256 KB.

`.tgs` limits are in `docs/tgs.md`.

### ffmpeg facts the design relies on

- libvpx-vp9 first-pass logs are byte-identical for different target
  bitrates, so pass 1 runs once and every bitrate attempt only runs pass 2;
- output size is close to linear in target bitrate, so a secant search
  converges in a few attempts;
- ffmpeg's native VP9 decoder drops alpha; use `-c:v libvpx-vp9` before `-i`
  when decoding stickers for preview or inspection;
- for SSIM, both inputs are retimed from frame numbers
  (`setpts=N/(FPS*TB)`), since WebM's millisecond timestamps pair
  neighbouring frames otherwise;
- ffmpeg before 7.0 stops the encoder at `-frames:v` without flushing it,
  which breaks libvpx's two-pass statistics; frame counts are cut with a
  `trim` filter instead;
- `-f null -` works as the pass-1 output on every OS.

### Size fitting

`--fit` picks what is tuned to get as close to 256 KB as possible:

- `auto` (default): tries a few frame rates, fits bitrate for each, scores
  them with SSIM against the source and keeps the best;
- `bitrate`: two-pass VBR, secant search on bitrate;
- `crf`, `fps`, `length`: search over that value at constant quality;
- `off`: a single encode with the given settings.

### Testing

`references/` is gitignored and holds local test media: `pig.mp4` and
`uhh.mp4` for video stickers, `pixelart/` for `.tgs` (see its README).
Tests that need it or ffmpeg skip themselves when either is missing. Unit
tests build their fixtures in code.
