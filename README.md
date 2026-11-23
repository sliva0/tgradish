# tgradish

Convert videos, images, and pixel art into Telegram stickers and custom emoji using the GUI or command line:

- **Video stickers and emoji (WebM)** from any video, GIF, or image.
- **Animated stickers and emoji (`.tgs`)** from pixel art: GIF, APNG, WebP, Aseprite, sprite sheets, or folders of frames.
- **Static stickers and emoji (WebP)** from an image or a single video frame.

Video stickers and emoji can exceed Telegram's 3-second limit: tgradish spoofs the duration in the file metadata so Telegram accepts them.

![tgradish turning pixel art into animated stickers](.github/screenshot.png)

## Installation

All releases are on the [Releases](https://github.com/sliva0/tgradish/releases) page. The links below point to the latest release.

### Windows

Only `x86-64` is supported. Release builds include ffmpeg.

- Download [tgradish-windows-x86_64.zip](https://github.com/sliva0/tgradish/releases/latest/download/tgradish-windows-x86_64.zip), unpack it, and run `tgradish.exe`; or
- Download [tgradish.exe](https://github.com/sliva0/tgradish/releases/latest/download/tgradish.exe) by itself.

### Linux

Both `x86-64` and `aarch64` are supported. `$(uname -m)` selects the file for your architecture.

<details>
<summary>
<b>Debian, Ubuntu, Mint</b> and other Debian-based distributions
</summary>

Includes ffmpeg:

```shell
wget https://github.com/sliva0/tgradish/releases/latest/download/tgradish-linux-$(uname -m).deb
sudo apt install ./tgradish-linux-$(uname -m).deb
```

</details>

<details>
<summary>
<b>Fedora, openSUSE</b> and other RPM-based distributions
</summary>

Includes ffmpeg:

```shell
curl -LO https://github.com/sliva0/tgradish/releases/latest/download/tgradish-linux-$(uname -m).rpm
sudo dnf install ./tgradish-linux-$(uname -m).rpm
# or on openSUSE
sudo zypper install --allow-unsigned-rpm ./tgradish-linux-$(uname -m).rpm
```

</details>

<details>
<summary>
<b>Arch Linux</b>
</summary>

Available on the AUR:

- `tgradish` — builds from source, uses system ffmpeg
- `tgradish-bin` — prebuilt binary, uses system ffmpeg
- `tgradish-static-bin` — prebuilt binary with ffmpeg built in

```shell
paru -S tgradish-bin
# or
yay -S tgradish-bin
```

</details>

<details>
<summary>
<b>NixOS and Nix</b>
</summary>

Uses nixpkgs' ffmpeg:

```shell
nix run github:sliva0/tgradish               # run once
nix profile install github:sliva0/tgradish   # install permanently
```

</details>

<details>
<summary>
<b>Any Linux</b> with glibc 2.28 or newer
</summary>

Tarball with ffmpeg built in:

```shell
curl -L https://github.com/sliva0/tgradish/releases/latest/download/tgradish-linux-$(uname -m).tar.gz | tar -xz
install -Dm755 tgradish-*/tgradish ~/.local/bin/tgradish
```

For musl systems (such as Alpine), use `tgradish-linux-musl-$(uname -m).tar.gz`. To use your system's ffmpeg on glibc systems, use `tgradish-linux-$(uname -m)-system-ffmpeg.tar.gz`. Archives also contain a `.desktop` file, icons, man pages, and shell completions.

</details>

<details>
<summary>
<b>Cargo</b>
</summary>

Requires Rust 1.95 or newer and a C compiler; uses the system's ffmpeg:

```shell
cargo install tgradish
```

</details>

<details>
<summary>
<b>From source</b>
</summary>

Requires Rust 1.95 or newer and a C compiler; uses the system's ffmpeg:

```shell
git clone https://github.com/sliva0/tgradish
cd tgradish
cargo install --locked --path crates/cli
```

</details>

### macOS

macOS is untested and has no prebuilt releases yet. If you can help test on macOS, [get in touch](#contributing).

Building from source with Homebrew's ffmpeg may work:

```shell
brew install ffmpeg rust
cargo install tgradish
```

### 1.x

The older Python 1.x version lives on the [`1.x`](https://github.com/sliva0/tgradish/tree/1.x) branch and PyPI. Its command line differs from 2.x; see [CHANGELOG.md](CHANGELOG.md).

## Dependencies

Release builds and `tgradish-static-bin` include ffmpeg. All other builds (`system-ffmpeg` tarballs, `tgradish`, `tgradish-bin`, Nix, and Cargo installs) run `ffmpeg` and `ffprobe` from `PATH`. They require **ffmpeg 6.0 or newer** with `libvpx` for VP9 encoding (the AUR and Nix packages install this automatically). Run `tgradish ffmpeg` to check which ffmpeg tgradish uses and whether it can make video stickers.

These features work without ffmpeg:

- Animated `.tgs` stickers from pixel art (images, GIF, APNG, WebP, Aseprite, sprite sheets, or frame folders)
- `tgradish spoof`
- `tgradish inspect` on stickers (WebM, WebP, TGS), images, animations, and Aseprite files (inspecting other videos requires ffmpeg)

## Usage examples

Run `tgradish gui` to open the GUI, or launch `tgradish` from a file manager or application menu. Press `F1` in the GUI to view keyboard shortcuts.

Command-line examples:

```shell
tgradish convert clip.mp4                  # video sticker: clip.sticker.webm
tgradish convert clip.mp4 --target emoji   # video emoji: clip.emoji.webm
tgradish convert walk.ase -f tgs           # animated sticker: walk.sticker.tgs
tgradish convert photo.jpg -f webp         # static sticker: photo.sticker.webp
tgradish watch ~/Videos -O ~/Stickers      # convert new videos and images automatically
tgradish inspect clip.sticker.webm         # check file details and Telegram limits
tgradish spoof ready.webm long.webm        # spoof an existing WebM's duration into long.webm
```

Convert only the `idle` tag of an Aseprite file with the `sketch` layer hidden:

```shell
tgradish convert walk.ase -f tgs --tag idle --hide-layers sketch
```

Run `tgradish --help` for all commands, and `tgradish <command> --help` for each command's options.

Shell completions: `tgradish completions bash|zsh|fish|nushell|powershell` (packages install them already). Your own presets go in the directory `tgradish preset path` prints.

To configure defaults, run `tgradish config init` to create the config directory and write an annotated example config (it will not overwrite an existing file), or `tgradish config example` to print the example without writing it.

## Scripting

- `--json` outputs JSON Lines (one compact JSON object per line). `convert` and `watch` print one event per line, `started` first and `finished` last for each file, and failures as `error` events.
- `-q` prints only the paths of created files, one per line.

Exit statuses:

- `0` — Success. `watch` also exits with `0` when cancelled.
- `1` — Something failed. With several inputs, this wins over `3`.
- `2` — Invalid command-line arguments.
- `3` — Everything was converted or read, but Telegram would reject a result or an inspected file.
- `130` — `convert` was cancelled (Ctrl-C or `--cancel-on-stdin`).
- `141` — Standard output was closed early on Unix (`1` on other platforms), for example when piped into `head`.
- `128 + N` — Stopped by signal `N` (`SIGTERM` or `SIGHUP`) on Unix, for example `143` for `SIGTERM`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Send feedback to [@s0untitled](https://t.me/s0untitled?direct) on Telegram or open an issue on GitHub.

## Licence

[MIT License](LICENSE.txt). Builds that include ffmpeg also distribute ffmpeg under the LGPL, with source archives attached to each release. Run `tgradish licenses` to print all bundled licence notices.
