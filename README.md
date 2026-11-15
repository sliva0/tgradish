# tgradish

Telegram video, static and animated stickers and emoji from videos, images or pixel art, in a GUI or on the command line:

- video stickers and emoji (WebM) from any video, GIF or picture;
- animated stickers and emoji (.tgs) from pixel art: GIF, APNG, WebP, Aseprite, a sprite sheet or a folder of frames;
- static stickers and emoji (WebP) from a picture, or a frame of a video.

Video stickers and emoji can run past Telegram's 3 second limit: tgradish spoofs the duration written in the file so Telegram accepts them.

![tgradish turning pixel art into animated stickers](.github/screenshot.png)


## Installation

From [Releases](https://github.com/sliva0/tgradish/releases):

- **Linux** (x86-64 and aarch64): a single binary, or a `.deb` or `.rpm` package:
  ``` console
  sudo apt install ./tgradish_2.0.0-1_amd64.deb
  sudo dnf install ./tgradish-2.0.0-1.x86_64.rpm
  ```
- **Windows** (x86-64): a zip with `tgradish.exe`.

Arch Linux, from the AUR: `tgradish` (built from source), `tgradish-bin` (uses the system's ffmpeg) or `tgradish-static-bin` (ffmpeg built in):
``` console
yay -S tgradish-bin
```

Nix:
``` console
nix run github:sliva0/tgradish
```

From source, with Rust 1.95 or newer: `cargo install tgradish`.

macOS is untested and not supported.

The Python version, 1.x, is on the [`1.x`](https://github.com/sliva0/tgradish/tree/1.x) branch and on PyPI. Its command line was different: see [CHANGELOG.md](CHANGELOG.md).


## Dependencies

Release builds have ffmpeg built in, except those named `system-ffmpeg`. The rest need `ffmpeg` and `ffprobe` on PATH, with libvpx; `tgradish ffmpeg` shows which is used and whether it can make stickers.

Animated stickers from pixel art, `tgradish spoof` and `tgradish inspect` work without ffmpeg.


## Usage examples

`tgradish gui [FILES]` opens the GUI, as does starting `tgradish` from a file manager or a menu. F1 lists its keyboard shortcuts.

On the command line:

``` console
tgradish convert clip.mp4                  # a video sticker, clip.sticker.webm
tgradish convert clip.mp4 --target emoji   # an emoji, clip.emoji.webm
tgradish convert walk.ase -f tgs           # an animated sticker, walk.sticker.tgs
tgradish convert photo.jpg -f webp         # a static sticker, photo.sticker.webp
tgradish watch ~/Videos -O ~/Stickers      # each new video or picture as it comes
tgradish inspect clip.sticker.webm         # what Telegram would say of it
tgradish spoof ready.webm long.webm        # a WebM made elsewhere, spoofed into long.webm
```

From an Aseprite file, only its `idle` tag, with its `sketch` layer hidden:
``` console
tgradish convert walk.ase -f tgs --tag idle --hide-layers sketch
```

`tgradish --help` lists the commands, and `tgradish <command> --help` their options. `tgradish config example` prints a config file to start from, each setting explained.


## Scripting

`--json` prints JSON Lines, and `tgradish describe` gives the schemas of its events and options. `-q` prints only the paths of the files made.

The exit status is 0 when done, 1 when something failed, 2 when the command line was wrong, 3 when everything was made but Telegram would refuse a result, and 130 when `convert` is cancelled.


## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Feedback: [@s0untitled](https://t.me/s0untitled?direct) on Telegram, or open an issue.


## Licence

[MIT License](LICENSE.txt). Builds with ffmpeg built in also carry ffmpeg under the LGPL, its sources attached to each release; `tgradish licenses` prints every notice a build carries.
