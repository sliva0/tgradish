# tgradish-frames

RGBA animations with frame durations, and pure-Rust decoders for them: GIF,
PNG and APNG, WebP, JPEG, BMP, Aseprite files, sprite sheets and image
sequences. It works on bytes alone, with no files, processes or threads, so
it builds for `wasm32-unknown-unknown`, and what decoding may allocate is
bounded, so hostile files fail rather than exhaust memory.

```rust
use tgradish_frames::{DecodeOptions, Limits, decode};

let animation = decode(&bytes, &DecodeOptions::default(), &Limits::default())?;
println!("{}x{}, {} frames", animation.width(), animation.height(), animation.frames().len());
```

Aseprite files can be read for their tags and layers first, with
`aseprite::Sprite`; `DecodeOptions` picks among them.

It is the part of [tgradish](https://github.com/sliva0/tgradish) that reads
pixel art, with a version of its own.
