# tgradish-tgs

Telegram animated stickers (`.tgs`, gzipped Lottie JSON) from pixel art
animations: the art is drawn as rectangles in layers, reduced until it fits
Telegram's 64 KiB, and checked against Telegram's limits. Pure Rust with no
files, processes or ffmpeg, so it builds for `wasm32-unknown-unknown`; the
`threads` feature tries fitting's candidates in threads of their own, and
`schema` gives JSON Schemas of its options and reports.

It is the part of [tgradish](https://github.com/sliva0/tgradish) that makes
animated stickers, with a version of its own.
