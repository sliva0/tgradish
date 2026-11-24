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

GUI strings live in `crates/gui/i18n/<tag>/tgradish.ftl` in [Fluent](https://projectfluent.org/fluent/guide/) format, one directory per language tag (such as `uk` or `pt-BR`). British English, `en-GB/tgradish.ftl`, is the source every other language is checked against and falls back on; its comments note where each string appears and what it means. CLI output stays in English because scripts parse it.

To add a language, copy `en-GB/tgradish.ftl` into a directory named after its tag (for example, `crates/gui/i18n/de/tgradish.ftl`) and translate every message and attribute, starting with `language-name`, the language's name for itself as Settings lists it. Every file under `i18n/` is built into the executable; nothing else needs registering. Each file must:

- Have every message and attribute `en-GB` has. Only a regional variant of English, such as `en-US`, leaves out what it says the same, and falls back on `en-GB` for it.
- Have nothing `en-GB` lacks, and use only the arguments (`{ $name }`) the same message in `en-GB` uses.
- Have whatever its messages put in: `{ other-message }` puts in a message of the same file, and `{ -term }` a term it defines itself.
- Give counts (`{ $count -> … }`) a variant for each plural form of the language, such as `[one]`, `[few]`, `[many]` and `*[other]` in Ukrainian; use `[1]` for "exactly one" where the number itself isn't shown, as a plural form such as Ukrainian's `[one]` covers 21 too.
- Use only characters the bundled fonts draw. To draw more, add them to `crates/gui/assets/fonts/subset.py` and run it with `uv run crates/gui/assets/fonts/subset.py`.

Check every file with `cargo test -p tgradish-gui every_language_file_is_sound`, which `cargo test --workspace` runs too: it lists every problem of every file, with the English of each message missing. Keep the names of programs, formats and units as they are (tgradish, Telegram, ffmpeg, WebM, WebP, TGS, VP9, CRF, SSIM, Aseprite, `s`, `px`, `fps`, `KiB`); key names such as Ctrl may follow the language's keyboards.

`cargo test -p tgradish-gui fits_every_language`, which `cargo test --workspace` runs too, takes the GUI through its scenes in every language, at its narrowest and as it usually opens, and lists by language and message what doesn't fit: words cut short or cut off, drawn over others, or wrapped onto a second line in a narrow column or on a button where English's aren't; buttons pushed to the window's edge; and dialogs that don't fit the window at large scales. Shorten such words in a way natural to the language. With `TGRADISH_LANG=de` it checks German alone.

To see a language, run the GUI with `TGRADISH_LANG=de`, which comes before the system's language and the one chosen in Settings, or `TGRADISH_LANG=pseudo` for longer, accented English to find text that doesn't fit. `TGRADISH_LANG=de cargo xtask strings` generates `target/strings/index.html`, which shows every GUI message, its translation under it, and screenshots of where it appears in that language, alongside all CLI help pages; without `TGRADISH_LANG` it shows English only.

## Licences

tgradish is MIT-licensed. Every binary embeds its own licence and the notices for everything built into it (kept in `crates/core/licenses`). After changing dependencies, regenerate them with `cargo xtask licenses` using `cargo-about` 0.9.2 (`cargo install --locked cargo-about@0.9.2 --features cli`). Keep ffmpeg under the LGPL — never enable its GPL components.

## Releasing

1. Run `cargo xtask release prepare X.Y.Z`. This updates versions across all files, turns `[Unreleased]` in `CHANGELOG.md` into the dated release section (used as GitHub release notes), updates both lockfiles, and bumps `RUSTUP_TOOLCHAIN` in `.github/workflows/release.yml` to your active stable Rust (run `rustup update` first). `tgradish-tgs` is versioned independently: `prepare` reports whether it changed since the last release, and `--tgs X.Y.Z` bumps it. `prepare` checks that all version references match (as `cargo xtask release check` does) but does not commit; it prints the commands for the next two steps.
2. Commit, push `main`, and run the release workflow on `main` by hand (`gh workflow run release.yml --ref main`, or **Run workflow** on its Actions page). Wait for it to pass. Without a tag it does everything a tag run does except draft the release: it checks the versions, dry-runs publishing the crates, builds every target, tests all packages, and gathers the release files. It also saves the ffmpeg build caches on `main`, the only branch a tag run can restore them from. Optionally, `cargo xtask local-release` builds the same files into `target/release-artifacts` with the command the workflow runs for each target (by default the host's Linux target and Windows; Windows requires `mingw-w64` and musl `musl-gcc`), and runs `.github/smoke.sh`, as the workflow's test jobs do, on the builds this machine can run.
3. Tag that commit and push the `vX.Y.Z` tag. The release workflow checks the tag against the version, builds and tests all targets again, and creates a draft GitHub release with the changelog notes. Review the draft and publish it.
4. Run `cargo xtask release aur` to update the AUR clones in `.aur/` (gitignored) from `packaging/aur` with checksums from the published release, write each `.SRCINFO` with `makepkg --printsrcinfo`, and commit each one. Pass `--push` to push them. Requires `makepkg` and an SSH key registered on the AUR. PKGBUILDs in this repo keep `SKIP` checksums and have no `.SRCINFO`. To update only AUR packaging without a new release, bump `pkgrel` in the PKGBUILD, commit, and run `cargo xtask release aur`.
5. Run `cargo xtask release crates` to publish each crate whose current version is not yet on crates.io, in dependency order (it prompts before publishing). Pass `--dry-run` to test packaging and building without publishing. If re-packaging a version you already packaged locally, delete any matching `tgradish-*` directories under Cargo's `registry/src` first so Cargo does not reuse cached copies.

`rust-version` in `Cargo.toml` sets the MSRV (minimum supported Rust version) for the whole workspace and is checked in CI.
