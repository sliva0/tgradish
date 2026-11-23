# Contributing

## Repository layout

| Crate | Purpose |
| --- | --- |
| `crates/tgs` (`tgradish-tgs`) | Converts pixel art into `.tgs` animated stickers and validates any `.tgs` file against Telegram's limits. Its `frames` module decodes images, animations, and Aseprite files into RGBA frames with durations. |
| `crates/core` (`tgradish-core`) | Shared engine for the CLI and GUI: conversion for all formats, ffmpeg integration, presets, config loading, and `inspect`. |
| `crates/gui` (`tgradish-gui`) | Desktop GUI built with egui. |
| `crates/cli` (`tgradish`) | Main binary: argument parsing and terminal output. Includes the GUI by default unless built with `--no-default-features`. |
| `crates/corpus` | Synthetic test media generated in code so tests do not rely on third-party files. |
| `crates/tgs-lab` | Encoder research tools: benchmarks and render checks against `tlottie` and `rlottie`. Separate workspace; run from its directory (or with `--manifest-path crates/tgs-lab/Cargo.toml`). |
| `xtask` | Developer tasks: building ffmpeg and release binaries, packaging, releasing, updating licence notices, and generating icons. |

Run `cargo doc --open` to read each crate's module documentation.

## Building and testing

```console
cargo test --workspace
```

This requires `ffmpeg` and `ffprobe` on `PATH` with `libvpx` and `libx264`. Tests that need ffmpeg are skipped if it is absent, or fail if `TGRADISH_REQUIRE_FFMPEG=1` is set (as in CI). Running `nix develop` provides all tools below except `cargo-about`. tgradish is built and tested on Linux and Windows.

- **Built-in ffmpeg backend:** `cargo test --workspace --features tgradish/linked` (requires ffmpeg development libraries, `pkg-config`, and `libclang`).
- **tgs-lab:** `cargo test --manifest-path crates/tgs-lab/Cargo.toml --features rlottie` (requires `cmake`, `ninja`, and `clang`).
- **GUI screenshots (requires a GPU):** `TGRADISH_SCREENSHOTS=DIR cargo test -p tgradish-gui -- --ignored screenshot`. This generates `tgradish-readme.png`, which is used as `.github/screenshot.png`.

CI also checks `rustfmt`, `clippy` with `-D warnings`, `cargo xtask icons --check`, `cargo xtask licenses --check`, and `cargo xtask release check`. See `.github/workflows/ci.yml` for exact commands.

Put personal test files in `references/` (gitignored).

## Writing style

Use comments to explain *why*, not *what* the code already shows. Write commit messages that state what changed and why.

### Translations

GUI strings live in `crates/gui/i18n/en-GB/tgradish.ftl` in [Fluent](https://projectfluent.org/fluent/guide/) format, with comments noting where each string appears. CLI output stays in English because scripts parse it.

To add a translation, copy that file into a directory named after the language tag (for example, `crates/gui/i18n/de/tgradish.ftl`). Missing messages fall back to English. Set `TGRADISH_LANG=de` to test a language, or `TGRADISH_LANG=pseudo` to test longer accented English strings for layout overflow. Scripts not covered by the bundled fonts (such as Chinese or Arabic) need a matching font added first.

Run `cargo xtask strings` to generate `target/strings/index.html`, which shows every GUI message alongside screenshots and all CLI help pages.

## Licences

tgradish is MIT-licensed. Every binary embeds its own licence and the notices for everything built into it (kept in `crates/core/licenses`). After changing dependencies, regenerate them with `cargo xtask licenses` using `cargo-about` 0.9.2 (`cargo install --locked cargo-about@0.9.2 --features cli`). Keep ffmpeg under the LGPL — never enable its GPL components.

## Releasing

1. Run `cargo xtask release prepare X.Y.Z`. This updates versions across all files, turns `[Unreleased]` in `CHANGELOG.md` into the dated release section (used as GitHub release notes), updates both lockfiles, and bumps `RUSTUP_TOOLCHAIN` in `.github/workflows/release.yml` to your active stable Rust (run `rustup update` first). `tgradish-tgs` is versioned independently: `prepare` reports whether it changed since the last release, and `--tgs X.Y.Z` bumps it. `prepare` does not commit; it prints the commit, tag, and push commands. Run `cargo xtask release check` to verify all version references match.
2. Run `cargo xtask local-release` to build all release variants into `target/release-artifacts`, named as the release names them (requires `mingw-w64` for Windows and `musl-gcc` for musl), and test them.
3. Commit and push the `vX.Y.Z` tag. The release workflow checks the tag against the version, builds and tests all targets, and creates a draft GitHub release with the changelog notes. Review the draft and publish it.
4. Run `cargo xtask release aur` to update the AUR clones in `.aur/` (gitignored) from `packaging/aur` with checksums from the published release, write each `.SRCINFO` with `makepkg --printsrcinfo`, and commit each one. Pass `--push` to push them. Requires `makepkg` and an SSH key registered on the AUR. PKGBUILDs in this repo keep `SKIP` checksums and have no `.SRCINFO`. To update only AUR packaging without a new release, bump `pkgrel` in the PKGBUILD, commit, and run `cargo xtask release aur`.
5. Run `cargo xtask release crates` to publish each crate whose current version is not yet on crates.io, in dependency order (it prompts before publishing). Pass `--dry-run` to test packaging and building without publishing. If re-packaging a version you already packaged locally, delete any matching `tgradish-*` directories under Cargo's `registry/src` first so Cargo does not reuse cached copies.

`rust-version` in `Cargo.toml` sets the MSRV (minimum supported Rust version) for the whole workspace and is checked in CI.
