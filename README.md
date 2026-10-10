# tgradish

Converts videos and images into Telegram video stickers and custom emoji,
with the ability to bypass the 3 second limit, and pixel art into animated
(`.tgs`) stickers.

> **Work in progress.** This branch is the 2.0 rewrite in Rust; there are no
> releases yet. See [docs/PLAN.md](docs/PLAN.md) for the roadmap; a
> Telegram web app comes after 2.0. The Python 1.x version is on `master`, and
> pixelart2tgs 1.x is now part of tgradish.

## What it does

- scales and encodes to WebM/VP9, keeping transparency;
- tunes the encoder until the file is just under Telegram's size limit
  (256 KB for stickers, 64 KB for emoji);
  by default it also tries lower frame rates and keeps whichever version
  looks closest to the source;
- spoofs the duration in the file header when the video is longer than
  3 seconds, so Telegram accepts it;
- scales in linear light, so edges don't darken, and keeps small pixel
  art crisp when it grows (`--scaling`);
- checks the result against Telegram's requirements.

For `.tgs` animated stickers, from GIF, APNG, WebP, JPEG, BMP or Aseprite
files, sprite sheets or image sequences of pixel art ([docs/tgs.md](docs/tgs.md)):

- finds the art's own pixel grid and draws it pixel-exact, without the
  seams 1.x had, at about half 1.x's size;
- when it still doesn't fit in 64 KB, makes the least visible changes that
  fit (merging close colours or near-identical frames, dropping frames,
  ...) and says which.

## Usage

Started from a file manager or the Start menu, tgradish opens a window:
drop or paste files, crop and trim each in a preview, choose a sticker or
an emoji and how hard to work on it, convert, look at the result, change
something and convert again. `tgradish gui` opens it from a terminal.

On the command line:

```console
# sticker from a video, written next to it as pig.sticker.webm
tgradish convert pig.mp4

# custom emoji, cut to 2 seconds starting at 1.5 s
tgradish convert pig.mp4 --target emoji --start 1.5 --length 2

# a sticker from part of a screen recording: 640x360 from (100, 50)
tgradish convert recording.mkv --crop 640x360+100+50

# 50x50 pixel art as a WebM sticker, every pixel 10x10, centred
tgradish convert sprite.png --scaling pixel-perfect

# quick result: bitrate fitting only, fast encoder
tgradish convert pig.mp4 --preset fast

# whatever is on the clipboard: copied files, a copied path or an image
tgradish convert --clipboard

# convert everything that lands in a folder, results go elsewhere
tgradish watch ~/Downloads/stickers --output-dir ~/stickers

# spoof an existing sticker
tgradish spoof pig.webm

# what Telegram will think of a file
tgradish inspect pig.sticker.webm

# animated sticker from pixel art, written as dance.sticker.tgs
tgradish convert dance.gif --format tgs

# from an Aseprite tag, or from numbered frames at 8 fps
tgradish convert walk.aseprite -o walk.tgs --tag run
tgradish convert frames/ --sequence -o walk.tgs --fps 8

# too large to fit as it is: keep the detail, drop frames first
tgradish convert machine.gif --format tgs --compromise motion
```

`tgradish convert --help` lists every option. Presets say how hard to work
(`fast`, `balanced`, `best`) for both formats; your own are TOML files in
the directory printed by `tgradish preset path`, see `tgradish preset show
balanced` for the format of options.

### Marks in the stickers

Stickers say they were made with tgradish in their metadata (WebM) or name
(`.tgs`); `--watermark=false` leaves that out. Every sticker also carries a
hidden mark of 8 bytes that no option removes: the tgradish version, whether
the command line or the window made it, and 24 bits of a hash of your user
name. The hash tells stickers by one person from others' without naming
them, though a common name can be guessed by hashing it. In WebM files the
mark is the video track's UID; in `.tgs` files it is the order of
rectangles that are drawn the same in any order, which costs about 0.6% of
the size. `tgradish inspect` shows it. The window also uses it to replace
results tgradish made before rather than numbering new ones.

## Installing

Release builds for Windows and Linux (x86-64 and ARM) are single files with
ffmpeg built in; download one from the releases page and run it. Nothing
else is needed. The Linux builds run on distributions from 2019 on.

Linux builds marked `system-ffmpeg`, and builds from source without the
`linked` feature, use the system's ffmpeg and ffprobe (6.0 or newer, with
libvpx for VP9) from `PATH`, or the ones `--ffmpeg PATH` points at;
`tgradish ffmpeg status` shows which one is used. See
[docs/ffmpeg.md](docs/ffmpeg.md).

On Arch Linux, from the AUR: `tgradish` (built from source) or
`tgradish-bin`. With Nix: `nix run github:sliva0/tgradish`, or the flake's
package in a NixOS configuration.

## Coming from 1.x

tgradish 1.x was a Python package (`pip install tgradish`); 2.0 is a single
program, installed as above. The package on PyPI stays at 1.x. Commands
changed a little:

| 1.x | 2.0 |
| --- | --- |
| `tgradish convert -i pig.mp4` | `tgradish convert pig.mp4` |
| `tgradish spoof pig.webm spoofed.webm` | `tgradish spoof pig.webm -o spoofed.webm` |

pixelart2tgs is now `tgradish convert art.gif --format tgs`.

## Front-ends

`tgradish describe` prints a JSON description of every option, preset and
progress event, and `--json` makes commands machine-readable. See
[docs/protocol.md](docs/protocol.md). Rust programs can use the
`tgradish-core` crate instead.

## Building

```console
# uses ffmpeg executables
cargo build --release

# without the window
cargo build --release --no-default-features

# with ffmpeg built in, against the system's ffmpeg libraries
cargo build --release --features linked
```

Tests that need ffmpeg or the local test media in `references/` (not in
git) skip themselves when those are missing.

## License

[MIT License](LICENSE.txt)
