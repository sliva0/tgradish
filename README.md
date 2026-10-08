# tgradish

Converts videos and images into Telegram video stickers and custom emoji,
with the ability to bypass the 3 second limit.

> **Work in progress.** This branch is the 2.0 rewrite in Rust; there are no
> releases yet. See [docs/PLAN.md](docs/PLAN.md). The Python 1.x version is
> on `master`.

## What it does

- scales and encodes to WebM/VP9, keeping transparency;
- tunes the encoder until the file is just under Telegram's 256 KB limit;
  by default it also tries lower frame rates and keeps whichever version
  looks closest to the source;
- spoofs the duration in the file header when the video is longer than
  3 seconds, so Telegram accepts it;
- checks the result against Telegram's requirements.

## Usage

```console
# sticker from a video, written next to it as pig.sticker.webm
tgradish convert pig.mp4

# custom emoji, cut to 2 seconds starting at 1.5 s
tgradish convert pig.mp4 --preset emoji --start 1.5 --length 2

# quick result: bitrate fitting only, fast encoder
tgradish convert pig.mp4 --preset fast

# spoof an existing sticker
tgradish spoof pig.webm

# what Telegram will think of a file
tgradish inspect pig.sticker.webm
```

`tgradish convert --help` lists every option. Presets are TOML files in the
directory printed by `tgradish preset path`, see `tgradish preset show
sticker` for the format of options.

## Requirements

ffmpeg and ffprobe with libvpx (VP9). tgradish looks for them next to its
own executable, then in its data directory, then on `PATH`; `tgradish
ffmpeg status` shows which one it found. Release builds will include a
minimal ffmpeg.

## Front-ends

`tgradish describe` prints a JSON description of every option, preset and
progress event, and `--json` makes commands machine-readable. See
[docs/protocol.md](docs/protocol.md). Rust programs can use the
`tgradish-core` crate instead.

## Building

```console
cargo build --release
```

Tests that need ffmpeg or the local test media in `references/` (not in
git) skip themselves when those are missing.

## License

[MIT License](LICENSE.txt)
