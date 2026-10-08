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
results. Differences:

- `extra-args` only works with ffmpeg as a separate program;
- the built-in ffmpeg only reads regular files, not FIFOs or devices,
  because reads that block in the OS could not be cancelled;
- display matrices with rotations other than quarter turns are ignored by
  the built-in ffmpeg, the command line would rotate by the exact angle.

Like the ffmpeg command line, `--start` seeks to the nearest keyframe
before the start. In files without an index (MPEG-TS, raw H.264) that have
few keyframes, the seek can land where nothing decodes, and the conversion
fails with no frames; convert the file to MP4 or MKV first.

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

## License obligations

ffmpeg is LGPL, so anything that ships it, as executables or linked into
tgradish, must come with its exact sources. `cargo xtask ffmpeg-sources`
packs the pinned source archives into
`target/ffmpeg/ffmpeg-<version>-sources.tar`; publish it with every
release. Since tgradish itself is open source, linking statically is fine:
anyone can rebuild it against a modified ffmpeg as described above.

## Releases

Pushing a `v*` tag runs the `release` workflow. It builds the minimal ffmpeg
for Linux and Windows, links it into tgradish statically, smoke-tests both
(the Windows build on Windows), and drafts a GitHub release with:

- `tgradish-<version>-<target>` archives with the single-file binary;
- `ffmpeg-<version>-<target>.tar.gz`, the executables for
  `tgradish ffmpeg download`;
- `ffmpeg-<version>-sources.tar`;
- `.sha256` files.

`cargo xtask package --target linux|windows` assembles an archive locally
after building with `--features linked-static --target <triple>`.

## Publishing builds for `tgradish ffmpeg download`

1. Run the `ffmpeg` workflow and download its artifacts.
2. Attach the archives to a GitHub release named
   `ffmpeg-<ffmpeg version>-<build number>`.
3. Add each archive's version, target, download URL and SHA-256 to
   `PUBLISHED` in `crates/core/src/ffmpeg/download.rs`.

Older tgradish versions keep downloading the builds they were released
with, so published archives must never be replaced.
