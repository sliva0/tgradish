# Changelog

What changed in each release of tgradish, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The GUI's number fields of whole numbers, as the crop's pixels, no longer
  show a decimal, as "235.0 px", at a scale below 1.
- The GUI's bar of an encode that comes out too large keeps its height,
  rather than at times losing a line and moving what is under it.
- The GUI's integer scale, its field emptied, keeps its scale rather than
  going to ×1/2.
- The GUI's number fields take numbers typed with spaces between their
  thousands, as "1 200", again.
- Ctrl and the arrows no longer push the GUI's crop back from the
  picture's edge while it keeps a shape: it stops growing there instead.
- Ctrl and the arrows make the GUI's crop larger again where an integer
  scale such as ½ holds it to whole blocks, a block at a time.
- A drag on the GUI's preview no longer goes on to crop another file's
  picture when Delete or Ctrl+Z shows another file halfway through it.

## [2.1.0] - 2026-11-18

### Added

- The GUI undoes changes to a file's settings with Ctrl+Z and makes them
  again with Ctrl+Shift+Z or Ctrl+Y, or from the menu behind the gear.
  Each file keeps its own history while it is open.

### Changed

- Making a `.tgs` shows how far it got: the GUI's bars and the command
  line's fill as the art is drawn, fitted and compressed. Where the result
  comes out too large and fitting tries again, each try has a bar of its
  own, as a WebM's encodes do. `--json` prints `advanced` and `missed`
  events for these.
- The GUI's number fields take what is typed on Enter, Tab or a click
  elsewhere, and Escape puts back what was there. They read units and a
  decimal comma, and Up, Down and the wheel step them. The − and + buttons
  repeat while held.
- The GUI's crop snaps to the picture's edges and middle unless Ctrl is
  held, keeps its shape with Shift and its middle with Alt, goes back on
  Escape while dragged, and moves by the arrows once clicked.
- The GUI's timeline shows the time under the pointer, seeks as the button
  goes down, and slides the part used when an end is dragged with Shift.
- The GUI's preview zooms in smaller steps and further, shows its scale,
  and pans across with Shift+wheel. Pixel art is shown at whole screen
  pixels to each of its own.
- The GUI's "Smooth scrolling" setting is now "Motion" (`motion` in the
  config file, which still reads `smooth-scrolling`). Progress, the list,
  notices and dialogs ease rather than jump, and nothing moves where the
  system asks for less motion.
- What scrolls in the GUI shows a scroll bar and fades where there is
  more, and each file keeps its own scroll position.
- The GUI without files is one drop zone, and the bar under the preview
  names the file the result is saved as.
- Fitting a WebM no longer says why it didn't try a lower frame rate, in
  the GUI, the command line or `--json`'s `decided` events.

### Fixed

- On KDE Plasma 6, the GUI's system accent is the accent chosen in
  Plasma's settings, not the darker selection colour made from it.
- A grey or dark accent no longer leaves the GUI's chosen items, main
  button and progress faint on the dark theme.

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

[Unreleased]: https://github.com/sliva0/tgradish/compare/v2.1.0...HEAD
[2.1.0]: https://github.com/sliva0/tgradish/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/sliva0/tgradish/compare/11d7f0d...v2.0.0
