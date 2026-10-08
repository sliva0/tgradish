# ffmpeg builds

tgradish can use ffmpeg in two ways:

- **built in**: with the `linked` Cargo feature, the ffmpeg libraries are
  linked into tgradish and used in-process. Release builds link them
  statically, so tgradish is a single file;
- **as separate programs**: `ffmpeg` and `ffprobe` executables.

Unless `--ffmpeg PATH`, `--ffmpeg-from` or `config.toml` says otherwise,
tgradish uses the built-in ffmpeg if it has one, and otherwise looks for the
executables:

1. next to the tgradish executable;
2. in the data directory, where `tgradish ffmpeg download` puts them;
3. on `PATH`, which is how Linux distributions would package tgradish.

`tgradish ffmpeg status` shows which one is used. Both ways run the same
filters and encoder settings; the tests check that they give the same
results.

## Building with ffmpeg built in

```console
# against the system's ffmpeg libraries (needs their headers and libclang)
cargo build --release --features linked

# statically, against the minimal build described below
cargo xtask ffmpeg
PKG_CONFIG_PATH=$PWD/target/ffmpeg/prefix-x86_64-unknown-linux-gnu/lib/pkgconfig \
  PKG_CONFIG_ALL_STATIC=1 cargo build --release --features linked-static
```

The static build only depends on glibc (about 15 MB).

## The minimal build

`cargo xtask ffmpeg [--target linux|windows] [--no-asm]` builds static
ffmpeg libraries, and ffmpeg and ffprobe executables, with only what
tgradish needs: common video and image
decoders, libvpx for VP9, dav1d for AV1, and the filters used for scaling,
padding and SSIM. It writes
`target/ffmpeg/ffmpeg-<version>-<target>.tar.gz` and a `.sha256` file.

- Runs on Linux. Windows builds are cross-compiled with mingw-w64.
- Needs a C toolchain, make, pkg-config, nasm, meson, ninja, curl and tar.
  `--no-asm` skips nasm, for testing only: encoding gets much slower.
- Source versions and checksums are pinned in `xtask/src/main.rs`. The
  zlib and dav1d checksums match the ones those projects publish; ffmpeg
  and libvpx were pinned when first downloaded.
- ffmpeg is configured without GPL parts, so the build is LGPL 2.1 or
  later. The archive includes the licenses of everything linked in.
- The Linux build only depends on glibc and is made on the oldest supported
  Ubuntu so it runs on older systems too.

The `ffmpeg` GitHub workflow builds both targets and tests the Windows build
by converting a video on a Windows runner.

## Publishing builds for `tgradish ffmpeg download`

1. Run the `ffmpeg` workflow and download its artifacts.
2. Attach the archives to a GitHub release named
   `ffmpeg-<ffmpeg version>-<build number>`.
3. Add each archive's version, target, download URL and SHA-256 to
   `PUBLISHED` in `crates/core/src/ffmpeg/download.rs`.

Older tgradish versions keep downloading the builds they were released
with, so published archives must never be replaced.
