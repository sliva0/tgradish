# tgradish 2.0 rewrite plan

tgradish converts videos into Telegram video stickers and custom emoji, and
can spoof the WebM duration header so a sticker can be longer than 3 seconds.

2.0 is a from-scratch Rust rewrite. Nothing is compatible with 1.x: the CLI,
config format and package are all new. The Python 1.x code lives on `master`
and in the `rewrite-attempt` branch for reference only.

## Layout

```
crates/
  core/   tgradish-core: library with all logic, no CLI code
  cli/    tgradish: command-line front-end over tgradish-core
docs/     plan and the front-end protocol
```

There is no GUI in this repo. A separate project will provide one GUI that
wraps both tgradish and the (also to be rewritten) pixelart2tgs. It can
integrate in two ways, and both are supported:

- depend on `tgradish-core` as a Rust library;
- run the `tgradish` binary and use the machine-readable protocol:
  `tgradish describe` prints a JSON description of all options (JSON Schema)
  and presets, and `--json` makes commands print events as JSON lines.
  pixelart2tgs is meant to implement the same protocol, so the GUI can stay
  tool-agnostic. See `docs/protocol.md` once it exists.

## Telegram limits

From <https://core.telegram.org/stickers> (checked 2026-10-08), shared by
stickers and emoji unless noted:

- WebM container, VP9 codec, no audio stream;
- sticker: one side exactly 512 px, the other at most 512 px;
- emoji: exactly 100x100 px;
- at most 3 seconds (this is what duration spoofing gets around);
- at most 30 fps;
- at most 256 KB.

## ffmpeg

Three ways to get ffmpeg, all supported:

1. run ffmpeg as a separate process:
   - a minimal static build made in CI and shipped next to the binary;
   - the system ffmpeg from `PATH` (always available as an option on Linux);
   - an explicitly configured path;
2. `tgradish ffmpeg download` fetches a pinned build and checks its checksum;
3. ffmpeg linked into the binary, behind a Cargo feature.

Verified facts that the design relies on (ffmpeg 9.0):

- libvpx-vp9 first-pass logs are byte-identical for different target
  bitrates, so pass 1 runs once and every bitrate attempt only runs pass 2;
- output size is close to linear in target bitrate, so a secant search
  converges in a few attempts;
- ffmpeg's native VP9 decoder drops alpha; use `-c:v libvpx-vp9` before `-i`
  when decoding stickers for preview or inspection;
- the `ssim` filter needs both inputs normalised with
  `settb=AVTB,setpts=PTS-STARTPTS`, otherwise it warns about mismatched
  timebases; scoring a 3 s 512x512 clip takes well under a second;
- `-f null -` works as the pass-1 output on every OS (1.x used `/dev/null`).

## Size fitting

`--fit` picks what is tuned to get as close to 256 KB as possible:

- `auto` (default): tries a few frame rates, fits bitrate for each, scores
  them with SSIM against the source and keeps the best;
- `bitrate`: two-pass VBR, secant search on bitrate;
- `crf`, `fps`, `length`: search over that value at constant quality;
- `off`: a single encode with the given settings.

## Milestones

2.0:

1. workspace, CI for Linux and Windows;
2. WebM parsing in core: `spoof` and `inspect`;
3. ffmpeg process backend, jobs with progress events and cancellation,
   size fitting;
4. CLI with new option names, presets in the OS config dir, `describe` and
   `--json`;
5. minimal static ffmpeg built in CI, bundled next to the binary, plus
   `tgradish ffmpeg download`;
6. linked ffmpeg backend behind a Cargo feature (done for Linux, Windows
   cross builds still to be tried in CI);
7. `tgradish watch <dir>` and clipboard input (done);
8. release archives, tag 2.0.0 (workflow written, needs a first CI run).

2.x:

- self-hosted Telegram bot (own token, allowlisted users, converts and can add
  stickers to the owner's packs), in its own crate;
- WebAssembly demo page with spoof and inspect only, built last.

## Testing

`references/` is gitignored and holds local test media (`pig.mp4`,
`uhh.mp4`). Tests that need it or ffmpeg skip themselves when either is
missing. Unit tests build their WebM fixtures in code.
