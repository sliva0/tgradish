# Contributing

## How it fits together

| Crate | What it holds |
| --- | --- |
| `crates/tgs` (`tgradish-tgs`) | Makes `.tgs` stickers from pixel art, and checks any `.tgs` against Telegram's limits. Its `frames` module reads pictures, animations and Aseprite files into RGBA frames with durations. |
| `crates/core` (`tgradish-core`) | What the command line and the GUI share: conversion to every format, ffmpeg, presets, the config file and `inspect`. |
| `crates/gui` (`tgradish-gui`) | The GUI, made with egui. |
| `crates/cli` (`tgradish`) | The binary: arguments and terminal output. It includes the GUI unless built with `--no-default-features`. |
| `crates/corpus` | Test inputs drawn in code, so nothing in the repository belongs to anyone else. |
| `crates/tgs-lab` | Encoder research: benchmarks, and renders checked against tlottie and rlottie. A workspace of its own: run it from its folder. |
| `xtask` | Release builds with ffmpeg built in, packages, releasing, licences and icons. |

Each crate's own docs (`cargo doc --open`) say how it is laid out.

## Building and testing

```console
cargo test --workspace
```

needs `ffmpeg` and `ffprobe` on PATH, with libvpx and libx264. Tests that
need them are skipped without, or fail with `TGRADISH_REQUIRE_FFMPEG=1`,
as in CI. `nix develop` gives everything below but cargo-about. tgradish
is built and tested for Linux and Windows.

- With ffmpeg built in: `cargo test --workspace --features tgradish/linked`,
  which needs ffmpeg's development files, pkg-config and libclang.
- tgs-lab:
  `cargo test --manifest-path crates/tgs-lab/Cargo.toml --features rlottie`,
  which needs cmake, ninja and clang.
- Pictures of the GUI, which need a GPU:
  `TGRADISH_SCREENSHOTS=DIR cargo test -p tgradish-gui -- --ignored screenshot`.
  `tgradish-readme.png` among them is the README's
  `.github/screenshot.png`.

CI also runs rustfmt, clippy with `-D warnings`, `cargo xtask icons
--check`, `cargo xtask licenses --check` and `cargo xtask release check`;
`.github/workflows/ci.yml` has the exact commands.

Keep your own test files in `references/`, which git ignores.

## Writing

Comments say why; the code says what. Commit messages say what changed
and why.

### Translations

The GUI's text is in `crates/gui/i18n/en-GB/tgradish.ftl`, in
[Fluent](https://projectfluent.org/fluent/guide/), and each message's
comment says where it is shown. The command line stays English, since
scripts read its output.

A translation is a copy of that file in a folder named for its language,
as `crates/gui/i18n/de/tgradish.ftl`. What it leaves out shows in English.
`TGRADISH_LANG=de` picks a language for a run, and `TGRADISH_LANG=pseudo`
shows English with longer, accented words, to see where text won't fit.
A script the GUI's fonts lack, as Chinese or Arabic, needs a font added
first.

`cargo xtask strings` writes `target/strings/index.html`: every message
of the GUI beside pictures of where it shows, and the command line's help
and messages.

## Licences

tgradish is MIT, and every binary carries the notices of what is built
into it, kept in `crates/core/licenses`. After changing dependencies, run
`cargo xtask licenses` with cargo-about 0.9.2 (`cargo install --locked
cargo-about@0.9.2 --features cli`). ffmpeg stays LGPL: never enable its
GPL parts.

## Releasing

1. `cargo xtask release prepare X.Y.Z` sets the version everywhere it is
   written, dates the changelog's Unreleased as its section, which becomes
   the release's notes, and refreshes both lock files. `tgradish-tgs` has
   a version of its own: `prepare` says whether it changed since the last
   release, and `--tgs X.Y.Z` raises it. It also raises `RUSTUP_TOOLCHAIN`
   in `.github/workflows/release.yml`, the Rust releases are built with,
   to the stable Rust here, so `rustup update` first. It commits nothing,
   and says what to commit, tag and push. `cargo xtask release check`,
   which CI runs, says whether every place agrees.
2. `cargo xtask local-release` builds every variant into
   `target/release-artifacts`, named as the release names them; it needs
   mingw-w64 for Windows and musl-gcc for musl. Try them.
3. Commit, and push the `vX.Y.Z` tag. The release workflow checks the
   tag is the version, builds and tests everything, and drafts a release
   with the changelog's notes. Check the draft, then publish it.
4. `cargo xtask release aur` brings the AUR packages, cloned in `.aur/`,
   which git ignores, up to `packaging/aur`, with the checksums of the
   published release, and commits each; `--push` pushes them. It needs
   makepkg, and an SSH key the AUR knows. The repository's own PKGBUILDs
   keep `SKIP` for checksums. To change the AUR packages alone, raise
   `pkgrel` in the PKGBUILD and its `.SRCINFO`, commit, and run it.
5. `cargo xtask release crates` publishes to crates.io the crates whose
   version it lacks, in the order they depend on each other, once asked;
   with `--dry-run` it packages and builds them instead. Before packaging
   again at a version already packaged, delete the `tgradish-*` folders
   under cargo's `registry/src`, or the old copies are used.

`rust-version` in `Cargo.toml` is the oldest Rust that builds every crate,
which CI checks.
