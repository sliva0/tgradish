# tgradish-corpus

Synthetic test inputs for the crates of
[tgradish](https://github.com/sliva0/tgradish): pixel art drawn in code,
written in each format tgradish reads (GIF, WebP, APNG, Aseprite, image
sequences and sprite sheets), and video clips made with ffmpeg, so every
machine tests with the same inputs and none of them belongs to anyone
else.

It is published so that the tests of `tgradish-frames`, `tgradish-tgs`,
`tgradish-core` and the rest build from their packages too; it has no use
outside them, and its interface changes as their tests need.

MIT licensed, as tgradish is.
