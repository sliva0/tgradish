# tgradish-tgs

A pure-Rust library that converts pixel art animations into Telegram
animated stickers (`.tgs`, gzipped Lottie JSON) and validates any `.tgs`
file against Telegram's limits.

It decodes GIF, PNG/APNG, WebP, JPEG, BMP, Aseprite files, sprite sheets
and image sequences on their native pixel grid, encodes them as layered
vector rectangles, optionally reduces them to fit within Telegram's 64 KiB
limit, checks the result against Telegram's limits and renders previews of
stickers it made (from the source art or from the `.tgs` file).

```rust
use tgradish_tgs::frames::{DecodeOptions, Limits, decode};
use tgradish_tgs::{Options, make};

let animation = decode(&bytes, &DecodeOptions::default(), &Limits::default())?;
let sticker = make(animation, &Options::default(), &mut |_progress| {}, &|| false)?;
std::fs::write("art.tgs", &sticker.tgs)?;
```

Decoding memory is bounded by `frames::Limits` so malformed or hostile
inputs fail safely instead of exhausting memory. For Aseprite files, use
`frames::aseprite::Sprite` to inspect tags and layers, and
`frames::DecodeOptions` to select which ones to render.

## Features and portability

Because it uses pure Rust with no filesystem access, subprocesses or
ffmpeg, `tgradish-tgs` compiles to `wasm32-unknown-unknown`.

- `threads` — evaluates size-fitting candidates across multiple threads.
- `internals` — exposes internal encoder APIs for tgradish's research tools
  (unstable, exempt from semver).

Part of [tgradish](https://github.com/sliva0/tgradish), versioned
independently.
