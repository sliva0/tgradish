# Contributing

## How it fits together

| Crate | What it holds |
| --- | --- |
| `crates/frames` (`tgradish-frames`) | RGBA animations with frame times, and pure-Rust decoders for GIF, PNG and APNG, WebP, JPEG, BMP, Aseprite, sprite sheets and image sequences. Works on bytes only, so it builds for `wasm32-unknown-unknown`. |
| `crates/tgs` (`tgradish-tgs`) | Pixel art to Telegram `.tgs` stickers: normalising the art to its grid, the painter's encoder that draws it as rectangles in layers, the reductions that fit it into 64 KiB, packing, and Telegram's limits and checks. No files, processes or ffmpeg either, and threads only with its `threads` feature. |
| `crates/core` (`tgradish-core`) | What the command line and the window share: planning WebM conversions and fitting them to the size limit, image stickers, `.tgs` conversion of files, WebM inspection and duration spoofing, presets, the config file, and the events front-ends follow. ffmpeg runs through a `Backend`: the `ffmpeg` and `ffprobe` programs, or with the `linked` feature its libraries, used to do exactly what those command lines do. |
| `crates/gui` (`tgradish-gui`) | The egui window, which calls core directly. |
| `crates/cli` (`tgradish`) | The binary: arguments and terminal output. It includes the window unless built with `--no-default-features`. |
| `crates/corpus` | Test art drawn in code, and video clips made with ffmpeg, so nothing in the repository belongs to anyone else. `tgs-lab corpus DIR` writes it all out. |
| `crates/tgs-lab` | Encoder research: benchmarks, renders checked against tlottie and rlottie, and the probes that measured Telegram's limits. A workspace of its own with its own `Cargo.lock`, so tlottie, which isn't on crates.io, and rlottie stay out of what packagers fetch: run it from its folder, as `cargo run -- corpus DIR`, or with `--manifest-path crates/tgs-lab/Cargo.toml`. |
| `xtask` | Release builds with ffmpeg built in, packages, licences and icons; its module docs list the tasks. |

### Times in videos

Times given by users, as cuts, frame times and previews, count from the
video stream's first frame. In some files that comes after the file's
start, as when sound starts first; ffmpeg's `-ss` counts from the file's
start, so every reader seeks with `Probe::seek`, which adds the difference,
and `Backend::frame_starts` gives frame times with it taken off.

A video cut keeps the frames that start at or after its start, as ffmpeg
does, so a cut meant to start on a frame is written a little before it
(`convert::read_from`), never more than a millisecond, in case times are
rounded on the way. Pixel art is cut by overlap instead: a frame is kept
when any of it falls within the cut. The window's timeline writes cuts by
both rules (`timeline::Frames`), and the tests
`counts_time_from_where_the_video_starts`, `finds_where_frames_start` and
`cuts_where_converting_keeps_the_frames_meant` hold them.

### Fitting `.tgs` stickers

Fitting works on estimates: a quick encode packed by `gzip -9`, scaled to
what the real encoder and zopfli make of it. The scale is measured once the
art comes near the limit. Each round keeps the reduction that saves the
most bytes for the least visible change, and when the real result still
misses, fitting aims a little lower and goes again. `crates/tgs/src/sticker.rs`
explains each constant involved.

The encoder's weights, such as `TILE` and `CORE_SHARE`, were measured on
the corpus. `cargo run --release -- bench --save before.tsv` in
`crates/tgs-lab` records how it encodes now, and after a change `bench --against
before.tsv` compares;
`--verify` also renders every result and counts wrong cells. tlottie's
limits in `crates/tgs/src/limits.rs` come from its source, at the version
`tgs-lab` depends on, and Telegram's from stickers `tgs-lab probes DIR` makes
for @Stickers to accept or refuse.

## Building and testing

```console
cargo test --workspace
```

needs `ffmpeg` and `ffprobe` on PATH, with libvpx, and libx264 for the
video clips of the corpus; tests that need them print "skipped" and pass
without. `nix develop` gives everything below.

- The built-in ffmpeg: `cargo test --workspace --features tgradish/linked`,
  with ffmpeg's development files, pkg-config and libclang.
- `.tgs` on one thread, as for the web: `cargo test -p tgradish-tgs`.
- tgs-lab, which renders with tlottie, and with rlottie too:
  `cargo test --manifest-path crates/tgs-lab/Cargo.toml --features rlottie`
  (cmake, ninja and clang).
- The window as pictures, which needs a GPU:
  `TGRADISH_SCREENSHOTS=/tmp/shots cargo test -p tgradish-gui -- --ignored screenshot`,
  into a folder that exists. `tgradish-readme.png` is the README's
  `.github/screenshot.png`.

CI also runs `cargo fmt --all --check`, clippy with `-D warnings` with and
without the window, `cargo xtask icons --check` and `cargo xtask licenses
--check`.

Keep your own test files in `references/`, which git ignores.

## Writing

Comments say why, in plain British English, and doc comments go on items;
the code says what. Commit messages say what changed and why.

## Licences

tgradish is MIT. Every binary carries every notice it needs
(`crates/core/build.rs`), which `tgradish licences` prints and About in the
window shows. After changing dependencies, run `cargo xtask licenses` with
cargo-about 0.9.2, which CI uses too. `cargo xtask ffmpeg` keeps
`licenses/ffmpeg` up to date. ffmpeg stays LGPL: never enable its GPL
parts. Paths and commands say "licenses", prose says "licences".

## Releasing

1. Set the version in `Cargo.toml`: `[workspace.package]` and the
   workspace's own crates under `[workspace.dependencies]`; in
   `crates/cli/tgradish.exe.manifest`, with a fourth number, 0; and
   `pkgver` in each `packaging/aur/*/PKGBUILD`. Then refresh `Cargo.lock`
   with `cargo update --workspace` and commit it, as release builds use
   `--locked`. A test checks the manifest, and the release workflow the
   PKGBUILDs.
2. `cargo xtask local-release` builds every variant into
   `target/release-artifacts`, Windows with mingw-w64. Try them.
3. Push a `vX.Y.Z` tag. The release workflow builds everything, tests the
   packages on Ubuntu, Debian, Fedora, openSUSE and Windows, and drafts a
   release with the archives, packages, checksums and ffmpeg's sources.
   Check the draft, then publish it.
4. In each `packaging/aur/*` folder, `updpkgsums`, then
   `makepkg --printsrcinfo > .SRCINFO`, and push to the AUR.

None of the crates is on crates.io yet. If the libraries are published,
it is in the order they depend on each other: `tgradish-frames`,
`tgradish-tgs`, `tgradish-core`, `tgradish-gui`, then `tgradish`. The
corpus, tgs-lab and xtask are `publish = false`, and stay that way.
