# Changelog

All notable changes to tgradish, newest first. This file follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The GUI in Ukrainian, Russian, Brazilian Portuguese, Spanish, Indonesian, Uzbek, Turkish, German, French, and American English. The translations are first drafts awaiting review by native speakers, and corrections are welcome (see CONTRIBUTING.md). The GUI follows the system's language, or the one chosen in Settings. A system set to English of a region that spells it as the US does (such as `en-US` or `en-PH`) gets American English; any other English gets British.
- Language in the GUI's Settings, marked with a globe, lists each language under its own name, with the system's first, showing which language that is. A change applies at once to everything shown, what was said before it too, such as a result's problems, a conversion's failure and the notices along the bottom. While the `TGRADISH_LANG` environment variable sets the language, Settings shows it and says so instead of offering a choice.
- `tgradish config init` writes the example config where tgradish reads it, creating its directory, and never overwrites an existing config.
- In the `tgradish-core` library, `resolve::resolve` layers a preset's options and any number of layers over them into a conversion's, checks them, and sets aside values that have no effect, saying who set each.

### Changed

- The CLI now says how good a WebM encode is in the GUI's words, as how much it looks like the input (for example, "attempt 3 already looks 99.6% like the input"), instead of as "99.6% SSIM". `-v` names the measure, SSIM, beside each score, and the help for `--fit` says what `auto` keeps by it. In `--json` output only the wording of `decided` events' `message` changes.
- `--lossless` (or `"lossless"` in `--options-json`) with a fit that doesn't keep a constant quality (`auto`, `bitrate` or `crf`) now fails with exit status 2 before any input is read, instead of failing each input with 1. A preset's `lossless` there is ignored with a warning instead.
- Warnings about options that have no effect, such as `crf` with `--fit bitrate`, now come right after each input's `started` event, before any other warning.
- The GUI now checks a preset's values before converting, as the CLI does. An out-of-range value, even one the other settings leave without effect (such as `fake-duration = 5.0` with `spoof = "never"`), now fails the conversion with a message naming the preset, instead of failing later or being ignored.
- After updating, a result from an earlier session shows as out of date once if it was made with integer scaling over a preset's `crop` or `stretch` resize, or with sprite sheet frames set while the file wasn't read as a sheet.
- The GUI now says in its own words what Telegram would refuse or frown on in a TGS result or a TGS sticker inspected, such as its drawing cost ("Drawing cost: 28271, over the limit of 24000; shapes: 25724, layers: 283, 9 each"), its layer count or a frame rate of 30 fps, and the warnings of a conversion, such as a video trimmed to 3 s with spoofing off, instead of as the CLI says them. In the GUI's other languages they were in English.
- In the `tgradish-core` library, `webp::Options` is now its own type holding only the options static WebP uses, instead of WebM's `webm::Options`. `webp::Request` and `FormatOptions::Webp` take it; `webp::PICTURE_OPTIONS` and `webp::picture_options` are removed (use `webp::Options::from(&webm_options)`).
- In the `tgradish-core` library, `Presets::resolve` is renamed `Presets::options`.
- In the `tgradish-core` library, what is one file's own (`target`, `tag`, `show-layers`, `hide-layers`, `crop`, `start`, `length`, `title`, `sheet` and `sheet-frames`) moves out of `webm::Options`, `webp::Options` and `tgs::Options`, which now hold just what a preset can set, into the new `options::Selection`. Each format's `Request` takes it as `selection`, and `options::Settings` pairs it with a format's options, read and written as one flat object as `--options-json` has it. `convert::Request` takes `settings` instead of `options`, `resolve::resolve` layers `Settings`, and `presets::PER_FILE`, `presets::shareable` and `FormatOptions::target` are removed (use `Selection::keys` and `Settings::target`).
- In the `tgradish-tgs` library, `check::Issue` has a `finding`, a `check::Finding` with what the check measured, for wording it otherwise than its `message` does, and is no longer `Eq`. In the `tgradish-core` library, `Event::Warning`'s `message` is an `events::Warning`, which says what it warns of with what was measured, and is serialised as the same message as before; `webm::plan` gives these. `--json` output is unchanged.

### Removed

- `tgradish describe`, which printed JSON Schemas of options and events. `--json` output is unchanged.

### Fixed

- Converting to WebP no longer fails when the preset sets an out-of-range value that only WebM uses, such as `tempo = 50.0`. Only the options WebP uses are checked.
- A preset saved from the GUI now converts in the CLI as the file it was saved from did in the GUI. Previously it could keep an integer scale the GUI wasn't using, without integer scaling, which the CLI then scaled by.
- The command line in the GUI's Commands section no longer includes `--sheet-frames` while the file isn't read as a sprite sheet, which the CLI refused.
- `tgradish inspect` now uses the `[ffmpeg]` setting from `config.toml` when inspecting videos, matching `convert`.
- `tgradish watch` no longer permanently skips an input when an earlier input wrote the same output path before being deleted (for example, `cat.mp4` after `cat.png`). With `--overwrite`, the existing output is now replaced as expected.
- Stickers no longer come out blank and pass checks when snapping to the pixel scale or a square crop removes thin lines or lone pixels. Conversion now reports that the art is empty, and size fitting no longer reduces art to nothing.
- Cancelling a queued (unstarted) conversion in the GUI no longer highlights the file as converted or replays its result fade-in.
- GUI warnings now sit above the bar under the preview when it wraps to two lines, instead of over its first line.
- The Inspect and About dialogs now scroll inside short windows or at high UI scales, keeping the Close button visible.
- Every GUI dialog now stays inside the window at UI scales of 2× and above in the smallest window, where Settings, Keyboard shortcuts and Your presets ran off its edges. What doesn't fit scrolls between the title and the buttons, Settings puts each label over its controls where they don't fit beside it, the shortcuts' descriptions wrap, and buttons that don't fit side by side go on in a second row.
- Screen readers now announce labels for GUI number fields and report what progress bars represent and their current progress.
- Holding an arrow key on the GUI crop now keeps moving it, instead of moving it a pixel or two and then taking the keyboard focus elsewhere.
- Pressing Escape in a GUI number field after stepping it with the arrow keys or the wheel now also puts back what the step changed elsewhere, such as the crop an integer scale holds, and leaves nothing to undo.
- Typing an integer scale in the GUI and clicking elsewhere to finish now makes one undo step, instead of an extra one for the crop the scale shrinks, which undoing only shrank again.
- A long path in GUI Settings, such as the ffmpeg found on PATH (for example `C:\ProgramData\Chocolatey\bin\ffmpeg.exe`), the example folder for results, or an ffmpeg that wasn't found, is now cut short in its middle where it doesn't fit, keeping the file's name and the words around it, and is shown whole on hover. Previously its end was cut off, hiding the file's name and "found on PATH".

## [2.1.1] - 2026-11-18

### Changed

- The minimum GUI scale is now 50% (previously 20%) so the file list and its icon always fit. Out-of-range `scale` values in `config.toml` are now clamped to `0.5–5` instead of resetting to `1`.

### Fixed

- The GUI no longer crashes when displaying images larger than the GPU texture limit (such as wide sprite strips or large panoramas). The preview scales them down to fit while conversion still reads the full image.
- Pressing Delete or Ctrl+Z mid-drag on the GUI preview no longer applies the crop to the newly selected file.
- Aspect-locked crops in the GUI now stop at the image edge instead of being pushed back from it when a corner is dragged or Ctrl+Arrow is pressed, and newly drawn crops now start at the initial press point.
- Ctrl+Arrow once again enlarges the GUI crop by one block at a time when integer scaling (such as `1/2`) snaps it to whole blocks.
- The GUI crop area now shows its keyboard focus ring immediately when focused via Tab or Shift+Tab, not only after moving it with arrow keys.
- Whole-number fields in the GUI no longer display `.0` (such as `235.0 px`) at scales below 100%, accept spaces as thousands separators (such as `1 200`) again, and clearing the integer-scale field now preserves its current value.
- Pressing Escape in an unchanged GUI number field now closes the enclosing dialog immediately; only edited fields consume the first Escape to revert.
- The GUI now keeps its icon and title above the file list at scales below 100%, and aligns the list header buttons with the preview header buttons at fractional scales such as 125% and 150%.
- The GUI settings panel below the preview no longer goes blank when resizing the file list while scrolled to the bottom.
- Progress bars for oversized WebM encodes now keep their height, and `.tgs` size-fitting bars keep displaying the first attempt's size while retrying.
- Restoring a removed file or undoing with Ctrl+Z now reflects any presets renamed in the meantime.

## [2.1.0] - 2026-11-18

### Added

- Per-file undo/redo for GUI settings via Ctrl+Z and Ctrl+Shift+Z / Ctrl+Y (or the gear menu). Each open file keeps its own history.

### Changed

- `.tgs` conversion now reports progress through drawing, size fitting, and compression in both the GUI and CLI. When size fitting retries an oversized result, each attempt gets its own progress bar (matching WebM). `--json` emits `advanced` and `missed` events for these steps.
- GUI number fields now commit on Enter, Tab, or focus loss, and revert on Escape. They accept units and decimal commas, step with Up/Down arrows and the mouse wheel, and repeat when holding the `−` and `+` buttons.
- GUI cropping now snaps to image edges and centres (hold Ctrl to disable), locks aspect ratio with Shift, scales around the centre with Alt, cancels a drag on Escape, and moves with arrow keys once clicked.
- The GUI timeline now displays the hovered timestamp, seeks immediately on pointer press, and shifts the selected range when dragging an endpoint with Shift.
- The GUI preview now zooms further in finer steps, displays the current zoom level, pans horizontally with Shift+wheel, and renders pixel art at whole screen pixels per art pixel.
- Renamed the GUI "Smooth scrolling" setting to "Motion" (`motion` in `config.toml`; `smooth-scrolling` is still accepted on read). Progress bars, lists, notices, and dialogs now animate smoothly unless the system prefers reduced motion.
- Scrollable GUI areas now display scroll bars with edge fades, and each file preserves its own scroll position.
- When empty, the entire GUI window acts as a drop zone, and the bar below the preview now shows the output filename.
- WebM size fitting no longer logs why it skipped trying a lower frame rate (in the GUI, CLI, and `--json` `decided` events).

### Fixed

- On KDE Plasma 6, the GUI now reads the actual system accent colour from Plasma settings instead of the darker selection colour derived from it.
- Grey and dark accent colours no longer make selected items, primary buttons, and progress bars hard to see in dark mode.

## [2.0.0] - 2026-11-16

Complete rewrite of tgradish in Rust as a single binary with both a GUI and CLI, supporting all Telegram sticker and emoji formats.

### Added

- Desktop GUI (`tgradish gui [FILES]`) with drag-and-drop, cropping, trimming, preview, and Telegram limit checks.
- Animated `.tgs` stickers and emoji (`-f tgs`) generated from pixel art and fitted to 64 KiB.
- Static WebP stickers and emoji (`-f webp`).
- Custom emoji target (`--target emoji`) alongside stickers.
- Built-in presets (`--preset fast|balanced|best`), custom user presets, and a config file (`tgradish config`).
- Automatic size fitting by bitrate, quality (CRF), frame rate, or duration (`--fit`), plus cropping, scaling, and timing controls.
- Directory watcher (`tgradish watch DIR`) to convert new files automatically.
- File inspector (`tgradish inspect`) to check media details and Telegram compatibility.
- Machine-readable `--json` output, `tgradish describe` for schemas, and `-q` to print only output paths.
- Shell completions and man pages.
- Optional built-in ffmpeg in release builds, or system ffmpeg.
- A per-conversion thread limit: `--threads N`, `threads` in `config.toml`, and Threads in GUI Settings.
- Prebuilt releases for Linux (glibc and musl, x86-64 and aarch64, tarballs, `.deb`, and `.rpm`), Windows, AUR packages, and a Nix flake.

### Changed

- tgradish is no longer distributed as a Python package. Install a prebuilt release, system package, or run `cargo install tgradish`.
- `convert` now takes input files as positional arguments without `-i`, and its options have new names (see `tgradish convert --help`).

### Removed

- The old ffmpeg command-line config file, along with `config copyfrom` and `config restore`. Use presets, standard flags, `--encoder-options`, or `--ffmpeg-args` instead.
- `--multithreading`, `--loop`, `--maxrate`, `--minrate`, and `--verbosity`.

## [1.0.4] - 2024-10-28

Final release of the Python version of tgradish, which converted videos to WebM stickers via ffmpeg and spoofed their duration.

[Unreleased]: https://github.com/sliva0/tgradish/compare/v2.1.1...HEAD
[2.1.1]: https://github.com/sliva0/tgradish/compare/v2.1.0...v2.1.1
[2.1.0]: https://github.com/sliva0/tgradish/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0
