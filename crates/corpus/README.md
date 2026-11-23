# tgradish-corpus

Synthetic test media for [tgradish](https://github.com/sliva0/tgradish):
pixel art generated in code in each format tgradish reads, plus video
clips generated with ffmpeg. This keeps tests reproducible across
machines without bundling third-party media.

This crate is not published: tgradish's crates use it only in tests (and
tgs-lab as a tool), so their packages leave it out.
