# tgradish-tgs

Telegram animated stickers (`.tgs`, gzipped Lottie JSON) from pixel art
animations: the art is read on its own pixel grid, drawn as rectangles in
layers, reduced until it fits Telegram's 64 KiB when that is allowed, and
checked against Telegram's limits.

```rust
use tgradish_tgs::frames::{DecodeOptions, Limits, decode};
use tgradish_tgs::{Options, make};

let animation = decode(&bytes, &DecodeOptions::default(), &Limits::default())?;
let sticker = make(&animation, &Options::default(), &mut |_progress| {}, &|| false)?;
std::fs::write("art.tgs", &sticker.tgs)?;
```

Pure Rust with no files, processes or ffmpeg, so it builds for
`wasm32-unknown-unknown`; the `threads` feature tries fitting's candidates
in threads of their own, and `schema` gives JSON Schemas of its options
and reports, written in kebab-case. The `internals` feature opens up the
encoder for tgradish's own research tools, and is exempt from semver.

It is the part of [tgradish](https://github.com/sliva0/tgradish) that makes
animated stickers, with a version of its own.
