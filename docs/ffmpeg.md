# ffmpeg builds

tgradish can use ffmpeg in two ways:

- **built in**: with the `linked` Cargo feature, the ffmpeg libraries are
  linked into tgradish and used in-process. Release builds link them
  statically, so tgradish is a single file;
- **the system ffmpeg**: `ffmpeg` and `ffprobe` executables from `PATH`,
  or wherever `--ffmpeg PATH` or `config.toml` points. This is for builds
  without the `linked` feature, like distribution packages.

By default (`--ffmpeg-from auto`) tgradish uses the built-in ffmpeg if it
has one, and the system one otherwise. `--ffmpeg PATH` always uses that
executable. `tgradish ffmpeg status` shows which one is used.

Both ways run the same filters and encoder settings; the tests check that
they give the same results. Differences:

- `--extra-args` (raw ffmpeg arguments) only works with the system ffmpeg;
  `--encoder-options` covers encoder settings with both;
- `--encoder-options` with a name libvpx-vp9 doesn't have is an error with
  the built-in ffmpeg; the ffmpeg command line only errors for names no
  part of ffmpeg knows, and warns about the rest;
- the built-in ffmpeg only reads regular files, not FIFOs or devices,
  because reads that block in the OS could not be cancelled;
- display matrices with rotations other than quarter turns are ignored by
  the built-in ffmpeg, the command line would rotate by the exact angle.

Like the ffmpeg command line, `--start` seeks to the nearest keyframe
before the start. In files without an index (MPEG-TS, raw H.264) that have
few keyframes, the seek can land where nothing decodes, and the conversion
fails with no frames; convert the file to MP4 or MKV first.

## Building from source

```console
# uses the system ffmpeg (6.0 or newer, with libvpx)
cargo build --release

# ffmpeg built in, linked against the system's ffmpeg libraries
# (needs their headers and libclang)
cargo build --release --features linked

# ffmpeg built in statically, like the releases, against the minimal
# build described below
cargo xtask ffmpeg
PKG_CONFIG_PATH=$PWD/target/ffmpeg/prefix-x86_64-unknown-linux-gnu/lib/pkgconfig \
  PKG_CONFIG_ALL_STATIC=1 cargo build --release --features linked-static
```

On Windows, the simplest is a plain `cargo build --release` and an ffmpeg
from <https://ffmpeg.org/download.html> (for example through `winget
install ffmpeg`) on `PATH`, or passed with `--ffmpeg`. A static build like
the releases is made on Linux by cross-compiling, see
`.github/workflows/release.yml`.

## The minimal build

`cargo xtask ffmpeg [--target linux|windows] [--no-asm]` builds static
ffmpeg libraries with only what tgradish needs: common video and image
decoders, libvpx for VP9, dav1d for AV1, and the filters used for scaling,
padding and SSIM. They go into `target/ffmpeg/prefix-<target>`, and the
licenses of everything in them into `target/ffmpeg/licenses-<target>`.

- Runs on Linux. Windows builds are cross-compiled with mingw-w64.
- Needs a C toolchain, make, pkg-config, nasm, meson, ninja, curl and tar.
  `--no-asm` skips nasm, for testing only: encoding gets much slower.
- Source versions and checksums are pinned in `xtask/src/main.rs`. The
  zlib and dav1d checksums match the ones those projects publish; ffmpeg
  and libvpx were pinned when first downloaded.
- Configure fails the build if it leaves out any component tgradish asks
  for, which it otherwise does silently for misspelled names.
- ffmpeg is configured without GPL parts, so the build is LGPL 2.1 or
  later.

## License obligations

ffmpeg is LGPL, so anything that ships it, linked into tgradish, must come
with its exact sources. `cargo xtask ffmpeg-sources` packs the pinned
source archives into `target/ffmpeg/ffmpeg-<version>-sources.tar`; it is
published with every release. Since tgradish itself is open source,
linking statically is fine: anyone can rebuild it against a modified ffmpeg
as described above.

## Releases

Pushing a `v*` tag runs the `release` workflow. It builds the minimal ffmpeg
for Linux and Windows, links it into tgradish statically, checks that the
Windows executable only needs DLLs that come with Windows, smoke-tests both
(the Windows build on Windows), and drafts a GitHub release with:

- `tgradish-<version>-<target>` archives with the single-file binary and
  the licenses;
- `ffmpeg-<version>-sources.tar`;
- `.sha256` files.

Pull requests that change the ffmpeg build run the same workflow without
publishing anything.

`cargo xtask package --target linux|windows` assembles an archive locally
after building with `--features linked-static --target <triple>`.
