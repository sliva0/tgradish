# tgradish

Telegram video, image and animated stickers and emoji from videos,
pictures and pixel art, in a window or on the command line. Video stickers
can run past Telegram's 3 seconds by spoofing the duration they say they
last.

What changed between releases is in [CHANGELOG.md](CHANGELOG.md), with a
table for those coming from 1.x.


## Installation

Release builds for Linux (archives, `.deb` and `.rpm`) and Windows are on
the [releases page](https://github.com/sliva0/tgradish/releases); there are
packages in the AUR (`tgradish`, `tgradish-bin`, `tgradish-static-bin`)
and a Nix flake. Or build it:

``` console
cargo install tgradish
```

Packages install shell completions for bash, zsh and fish, and man pages;
elsewhere `tgradish completions SHELL` prints the script for a shell.

Video and image stickers need `ffmpeg` and `ffprobe` with libvpx, on PATH,
unless the build has ffmpeg built in (`tgradish ffmpeg` says which is
used). `.tgs` stickers from pixel art and `tgradish spoof` need neither.


## Usage

`tgradish gui [FILES]` opens the window, with the files given; it also
opens when tgradish is started from a file manager or a menu.

``` console
tgradish convert clip.mp4                 # a video sticker, clip.sticker.webm
tgradish convert clip.mp4 --target emoji  # an emoji, clip.emoji.webm
tgradish convert walk.ase -f tgs          # an animated sticker, walk.sticker.tgs
tgradish convert photo.jpg -f webp        # an image sticker, photo.sticker.webp
tgradish watch ~/Videos -O ~/Stickers     # each new video or picture as it comes
tgradish spoof clip.webm long.webm        # a WebM made to pass Telegram's 3 s
tgradish inspect clip.sticker.webm        # what Telegram would say of it
```

`tgradish --help` and each command's `--help` say the rest. Files are
named on the command line: `-` for stdin or stdout isn't supported.
`tgradish config example` prints a config file to start from, and
`tgradish config path` where it goes.

Exit status: 0 when done, 1 when something failed, 2 for a mistake in the
command line, 3 when everything was made but Telegram would refuse a
result, and 130 when `convert` was cancelled.


## For scripts and front-ends

`--json` prints JSON Lines, an object a line. `convert` and `watch` print
events as they go, ending each file with a `finished` event that names its
`input` and `output`; `webp` conversions start with a `started` event as
the others do, and `spoof` prints one `finished` event with the `issues`
Telegram would still have. `tgradish ffmpeg --json` says which ffmpeg is
used and, as `from`, what chose it. `tgradish describe` gives the options,
presets and the schemas of the events, `spoof-events` among them.


Feedback: [@s0untitled](https://t.me/s0untitled?direct) on Telegram, or
open an issue.


## License

[MIT License](LICENSE.txt)
