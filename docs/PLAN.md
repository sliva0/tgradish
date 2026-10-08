# tgradish 2.0 plan

tgradish makes Telegram stickers and custom emoji: video stickers (`.webm`)
from any video or image, with duration spoofing to get past the 3 second
limit, and animated stickers (`.tgs`) from pixel art. It replaces tgradish
1.x and pixelart2tgs 1.x, which merges into it.

2.0 is a from-scratch Rust rewrite. Nothing is compatible with 1.x: the CLI,
config format and packages are all new. The Python 1.x code lives on
`master` and in the `rewrite-attempt` branch for reference only.

**Release rule:** nothing is merged into `master`, tagged or released until
the whole 2.0 roadmap below is done: `.tgs`, the GUI and packaging included.
Until then everything happens on `rewrite-v2`. The Telegram web app comes
after 2.0.

## Layout

```
crates/
  core/     tgradish-core    WebM, ffmpeg backends, presets, protocol
  cli/      tgradish         the binary: CLI and GUI (see "Shipping one binary")
  tgs/      tgradish-tgs     frames in, .tgs bytes out (docs/tgs.md)
  frames/   tgradish-frames  RGBA animations, pure-Rust decoders
  gui/      tgradish-gui     the window (egui), linked into the binary
  tgs-lab/  publish = false: render and seam checks, benchmark
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
  presets, config, `ffmpeg status`;
- release workflow: single-file binaries with ffmpeg linked in for Linux
  x86-64 (26.5 MB), Linux aarch64 (20.7 MB) and Windows x86-64 (23.7 MB),
  and Linux builds that use the system's ffmpeg (12.2 and 10.8 MB), each
  tested on its own system; sizes include the window;
- `.tgs`: T1 (crates, WASM check), T2 (decoders, normalisation), T3 (Lottie
  writer, checks, `tgs-lab verify` in tlottie and rlottie), T4 (encoder v1:
  half the size of 1.x, no seams), T5 (effort levels, bench baselines), T7
  (fitting with lossy reductions), T8 (`.tgs` in the CLI, protocol 2). T6
  (encoder v2: colours split into a lasting core and per-frame changes;
  other experiments are in `docs/tgs.md`);
- GUI: `tgradish gui`, or tgradish started outside a terminal;
- distribution: the targets above, AUR packages and a Nix flake, with
  publishing steps for the user in `docs/packaging.md`. Next: T9 probes
  and the release.

## Roadmap

In this order; the GUI waits for `.tgs` because it needs the final shape of
both formats' options, presets and events.

### 1. WebM cleanups

Decided changes that affect the option and protocol shape:

- **`--encoder-options NAME=VALUE`** (repeatable, done) tunes libvpx with
  both backends: the process backend passes `-NAME:v VALUE`, the built-in
  one sets the encoder option directly. Examples: `tune-content=screen`,
  `aq-mode=2`, `sharpness=4`, `arnr-strength=3`, `g=60`, `qmax=50`.
  `--extra-args` stays for raw ffmpeg arguments, but only with ffmpeg as a
  separate program, since the built-in ffmpeg has no command line.
- **ffmpeg as a separate program** stays only for the system ffmpeg: the
  ffmpeg-less Linux build, AUR, Nix and source builds (done). `tgradish
  ffmpeg download`, the published ffmpeg archives and the downloader's
  HTTP/TLS dependencies are gone, and the xtask builds only ffmpeg's
  libraries. Building from source on Windows has short instructions in
  `docs/ffmpeg.md`.

### 2. `.tgs` animated stickers

The plan is in `docs/tgs.md` (milestones T1–T10, written in a separate
planning session). T1–T8 come here; T9 (upload probes) and T10 (release)
are part of the last step. T8 settles how `describe`, presets and events
cover two formats, which the GUI then builds on.

### 3. GUI

- egui (eframe), in this repo, linked against the libraries rather than
  driving the CLI.
- One binary that is both CLI and GUI, see "Shipping one binary" below.
- Forms are generated from the options' JSON Schema, so `.webm` and `.tgs`
  options show up without hand-written UI for each.
- Done: files by dialog, drag and drop or paste (files, paths, images);
  folders and several images as the frames of one `.tgs`; presets with the
  form showing what differs from them; a queue run one job at a time with
  progress, log and cancelling; a looping preview of the result over a
  checkerboard (`.tgs` from the final animation, WebM decoded by the
  backend); an inspect view; settings (output folder, overwriting, default
  presets, ffmpeg) saved to `config.toml`. Headless tests drive it with
  egui_kittest.
- glow rather than wgpu, as eframe's docs say it is much smaller. The
  window adds about 8 MB to the Linux binary (4.4 MB without it); the
  window's dependencies are built for size (`opt-level = "s"`), which
  saves 1.5 MB without slowing conversions. egui's fonts (1.4 MB) stay, so
  emoji and symbols in file names show; Wayland support stays too.

### 4. Distribution

Done:

- **Targets:** Linux x86-64, Linux aarch64 (built natively on
  `ubuntu-24.04-arm`; ffmpeg's and libvpx's ARM assembly need no nasm) and
  Windows x86-64.
- **Old glibc baseline:** Linux releases are built in `manylinux_2_28`
  containers (AlmaLinux 8, glibc 2.28) instead of on the `ubuntu-22.04`
  runner, which is unsupported from 2027-04-17; packaging fails if a
  binary needs a newer glibc. A fully static musl build is not an option
  with the GUI in the binary, since it loads the system's graphics
  libraries.
- **Linux build without ffmpeg:** the process backend, which works with any
  ffmpeg 6+ on `PATH` without ABI coupling, released next to each Linux
  build with ffmpeg linked in.
- **Licenses:** archives list every Rust crate in them with its license
  (cargo-about), besides ffmpeg's licenses and sources.
- **AUR and Nix:** PKGBUILDs for `tgradish` (from source,
  `depends=(ffmpeg)`) and `tgradish-bin` (the system-ffmpeg release
  builds) in `packaging/aur`, and `flake.nix` with the package (built
  against nixpkgs' ffmpeg, tests run in the sandbox) and a dev shell, built
  by the `nix` workflow. Publishing to the AUR needs the user's account;
  `docs/packaging.md` has the steps. Submitting to nixpkgs can come later.

### 5. Telegram probes and release

- T9 from `docs/tgs.md`: upload test stickers through @Stickers and check
  them on Android, Desktop, iOS and web; the WebM side gets the same
  treatment (spoofed durations, watermarks, emoji).
- Merge into `master`, remove the temporary release trigger on
  `rewrite-v2`, tag 2.0.0, publish the drafted release.
- After release: ask before pointing the old pixelart2tgs README at
  tgradish.

### After 2.0

- **Telegram bot and web app:** a Mini App (web UI inside Telegram) plus a
  bot that puts results into the user's sticker packs, and the same page in
  a normal browser. Platform findings and limits are in `docs/web-app.md`.
  It absorbs the earlier "WebAssembly demo page" and "self-hosted bot"
  ideas, and the "Web page" and "Bot" items in `docs/tgs.md`.
- Windows ARM64: needs an aarch64 MinGW (llvm-mingw) for the ffmpeg cross
  build, or native builds on `windows-11-arm`.
- Pixelate mode from `docs/tgs.md`.

### Designing for the web app now

The web app is planned, so 2.0's APIs should not rule it out:

- `tgradish-tgs` and `tgradish-frames` build for `wasm32-unknown-unknown`,
  checked in CI.
- Library entry points work on bytes and callbacks, not paths: file
  handling stays in the CLI, GUI and thin `*_file` helpers. WebM inspection
  and patching already work this way.
- Options, presets and events stay plain serialisable data with JSON
  Schemas, so a web page can render the same forms as the GUI.
- Conversion stays behind the `Backend` abstraction, so a browser backend
  (WebCodecs or ffmpeg in WebAssembly) can be added later.
- Nothing in the libraries assumes threads, a filesystem or processes
  unless it sits behind a feature.

## Shipping one binary

One `tgradish` binary is both the CLI and the GUI:

- arguments given → CLI;
- no arguments and not started from a terminal (double-click, desktop
  file, Start menu) → GUI;
- no arguments in a terminal → the CLI help, which mentions `tgradish gui`;
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
double-clicked; tgradish frees it when no other process shares it, which
closes it. A separate `tgradish-gui.exe` built as a GUI program is the
fallback if that is too ugly. The manifest is embedded with
`embed-manifest`, which needs no Windows tools when cross-compiling.

## Decisions and open questions

Decided 2026-10-08:

- GUI with egui, in this repo, in the same binary as the CLI.
- No arguments in a terminal shows the CLI help.
- `ffmpeg download` goes. `--encoder-options` works with both backends,
  `--extra-args` only with ffmpeg as a separate program.
- ARM: Linux aarch64 in 2.0, Windows ARM64 later.
- Order: WebM cleanups, `.tgs`, GUI, distribution, probes and release.
- The web app comes after 2.0. Conversion in the browser is preferred if
  Telegram's WebViews allow what it needs (file input, getting files out,
  threads); the probe in `docs/web-app.md` decides. The probe needs a test
  bot (BotFather) and somewhere to host the page, which only the user can
  set up.
- AUR and Nix are prepared at the end, with instructions for the user.

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
