# Changelog

What changed in each release of tgradish, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - Unreleased

tgradish rewritten in Rust: one program, with a window and a command line,
that makes every kind of Telegram sticker and emoji.

### Added

- A window, `tgradish gui [FILES]`, which also opens when tgradish is
  started from a file manager or a menu, or with files opened in it from
  one: drop files in, crop, cut and preview them, and see what Telegram
  would say of the result. It reopens the files of last time, laid out as
  they were.
- Shell completions (`tgradish completions SHELL`) and man pages, which
  packages install.
- Animated stickers and emoji as `.tgs` (`-f tgs`), drawn from pixel art:
  GIF, PNG and APNG, WebP, Aseprite files, sprite sheets, image sequences
  and videos of pixel art, fitted into 64 KiB.
- Image stickers and emoji as WebP (`-f webp`).
- Emoji (`--target emoji`) beside stickers.
- Presets (`--preset fast|balanced|best`, and your own), and a config file
  in the user's config folder (`tgradish config path`, `config show`,
  `config example`).
- Size fitting by bitrate, quality, frame rate or length (`--fit`), and a
  choice of how a picture is cropped, scaled and timed.
- Fitting a WebM stops at the first encode that uses 96% to 100% of the
  limit, predicts each next setting from the encodes so far, and skips an
  encode predicted to gain under 1% of the limit or any likeness to be
  seen. `--fit auto` tries lower frame rates only where nothing fits, or
  where the best that fits looks poor and looked better for more bits.
  Why fitting stops, or tries a lower frame rate or not, is said: in
  `convert`'s output, as `decided` events with `--json`, and under the
  table of encodes in the window.
- `tgradish watch DIR`, converting what appears in a folder.
- `tgradish inspect`, which says what a WebM, WebP or `.tgs` file holds
  and whether Telegram would take it.
- `--json` output for scripts and front-ends, as JSON Lines, and `tgradish
  describe`, which gives the schemas of its options and events.
- ffmpeg built in, in the release builds that say so, or the system's.
- Releases for Linux (glibc and musl, x86-64 and aarch64, as archives,
  .deb and .rpm), Windows, the AUR and a Nix flake.

### Changed

- tgradish is no longer a Python package: install a release build or a
  package, or build it with `cargo install tgradish`.
- Results are named `NAME.sticker.webm` (or `.emoji`, `.tgs`, `.webp`)
  beside their input, where 1.x wrote `NAME.webm`.

### Removed

- The config file of ffmpeg command lines, with `config copyfrom` and
  `config restore`: presets and flags take their place, and
  `--encoder-options` and `--ffmpeg-args` pass on what they don't cover.
- `--multithreading`, `--loop`, `--maxrate`, `--minrate` and
  `--verbosity`.

### Coming from 1.x

| 1.x | 2.0 |
| --- | --- |
| `pip install tgradish` | a release build or package, or `cargo install tgradish` |
| `tgradish convert -i clip.mp4` | `tgradish convert clip.mp4` |
| `tgradish convert -i clip.mp4 -o out.webm` | `tgradish convert clip.mp4 -o out.webm` |
| `tgradish spoof in.webm out.webm` | the same, or `tgradish spoof in.webm -o out.webm` |
| `--guess-value`, `-g` | `--fit` |
| `--iterations`, `-it` | `--attempts` |
| `-min`, `-max` | `--fit-range MIN..MAX` |
| `--framerate`, `-fr` | `--fps` |
| `--bitrate`, `-bt` | `--bitrate`, `-b` |
| `-crf` | `--crf` |
| `--lossless`, `-ll` | `--lossless` |
| `--best_quality`, `-bq` | `--speed best`, or `--preset best` |
| `--scaling squared`, `-sc squared` | `--resize stretch` |
| `--verbosity` | `-v`, `-vv` or `-q` |
| `tgradish config showpath` | `tgradish config path` |

## [1.0.4] - 2024-10-28

The last release of the Python tgradish, which converted videos to video
stickers with ffmpeg and spoofed their duration.

[2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0
