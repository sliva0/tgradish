# Changelog

What changed in each release of tgradish, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Fitting a WebM no longer says why it didn't try a lower frame rate, in
  the GUI, the command line or `--json`'s `decided` events.
- The GUI shows a bar for fitting a `.tgs` only while it is being fitted,
  not for one that fits or is kept exactly as it is.
- The GUI shows its accent plainly where it was faint: the timeline's used
  part on the light theme, and, with a grey or dark accent on the dark
  theme, what is chosen, the main button and progress.
- The GUI without files is one drop zone, until the first file is added.
  The bar above the preview says what the mouse does on it only where
  that fits whole, in a wide window, rather than cutting it short. The bar
  under it
  names the result's file, its folder said on hover, and stays on one line.
- The GUI's number fields take a number typed once Enter, Tab or a click
  elsewhere finishes it, and Escape puts back what was there, so what
  follows from them, as the crop does from its ratio, no longer follows
  each digit on the way. They read units and a decimal comma typed with
  the number, and flash where what is typed isn't one. Up and Down, and
  the wheel over a field being typed in, step them, further with Shift,
  and dragging crosses a field's whole range in a few hundred points.
- Held, the − and + of the integer scale and of Settings' scale step on,
  faster after a while; the integer scale's are as tall as the controls
  beside them.

### Fixed

- On KDE Plasma 6, the GUI's accent as the system has it is the accent
  chosen in Plasma's settings, the same as choosing that colour in the
  GUI, not the darker selection colour Plasma makes from it.

## [2.0.0] - 2026-11-16

tgradish rewritten in Rust: one program, with a GUI and a command line,
that makes every kind of Telegram sticker and emoji.

### Added

- A GUI, `tgradish gui [FILES]`: drop files in, crop, cut and preview
  them, and see what Telegram would say of the result.
- Animated stickers and emoji as `.tgs` (`-f tgs`), drawn from pixel art
  and fitted into 64 KiB.
- Static stickers and emoji as WebP (`-f webp`).
- Emoji (`--target emoji`) beside stickers.
- Presets (`--preset fast|balanced|best`, and your own) and a config
  file (`tgradish config`).
- Size fitting by bitrate, quality, frame rate or length (`--fit`), and a
  choice of how a picture is cropped, scaled and timed.
- `tgradish watch DIR`, converting what appears in a folder.
- `tgradish inspect`, which says what a file holds and whether Telegram
  would take it.
- `--json` output and `tgradish describe` for scripts and front-ends, and
  `-q`, which prints only the paths of the files made.
- Shell completions and man pages.
- ffmpeg built in, in the release builds that say so, or the system's.
- `--threads N`, `threads` in the config file and Threads in Settings, which
  limit how many threads a conversion uses.
- Releases for Linux (glibc and musl, x86-64 and aarch64, as archives,
  .deb and .rpm), Windows, the AUR and a Nix flake.

### Changed

- tgradish is no longer a Python package: install a release build or a
  package, or build it with `cargo install tgradish`.
- `convert` takes its inputs as arguments, without `-i`, and its options
  have new names: see `tgradish convert --help`.

### Removed

- The config file of ffmpeg command lines, with `config copyfrom` and
  `config restore`: presets and flags take their place, and
  `--encoder-options` and `--ffmpeg-args` pass on what they don't cover.
- `--multithreading`, `--loop`, `--maxrate`, `--minrate` and
  `--verbosity`.

## [1.0.4] - 2024-10-28

The last release of the Python tgradish, which converted videos to video
stickers with ffmpeg and spoofed their duration.

[Unreleased]: https://github.com/sliva0/tgradish/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0
