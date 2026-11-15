# tgradish-tgs

A library that turns pixel art animations into Telegram animated stickers
(`.tgs`, gzipped Lottie JSON): the art is read on its own pixel grid, drawn
as rectangles in layers, reduced until it fits Telegram's 64 KiB when that
is allowed, and checked against Telegram's limits. It also checks any
`.tgs` against those limits, and draws what a sticker it made shows, from
the art or from the file, for previews.

```rust
use tgradish_tgs::frames::{DecodeOptions, Limits, decode};
use tgradish_tgs::{Options, make};

let animation = decode(&bytes, &DecodeOptions::default(), &Limits::default())?;
let sticker = make(animation, &Options::default(), &mut |_progress| {}, &|| false)?;
std::fs::write("art.tgs", &sticker.tgs)?;
```

Pure Rust with no files, processes or ffmpeg, so it builds for
`wasm32-unknown-unknown`; the `threads` feature tries fitting's candidates
in threads of their own, and `schema` gives JSON Schemas of its options
and reports, written in kebab-case. The `internals` feature opens up the
encoder for tgradish's own research tools, and is exempt from semver.

It is the part of [tgradish](https://github.com/sliva0/tgradish) that makes
animated stickers, with a version of its own.
