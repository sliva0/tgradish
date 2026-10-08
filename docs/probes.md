# Telegram probes (T9)

Some of what tgradish writes is based on Telegram's documentation and
clients' source code rather than on what Telegram really does. These
probes check it: upload them, look at them in each app, and write down
what happened. The results decide the last changes before 2.0.

## Making them

The `.tgs` probes, and `CHECKLIST.md` describing them, with Ralsei as
the real art example:

```console
cargo run --release -p tgs-lab -- probes references/t9-probes \
  references/pixelart/Ralsei_battle_start.gif
```

The WebM probes, from the test videos:

```console
D=references/t9-probes
tgradish convert references/uhh.mp4 --length 3 -o $D/w1-sticker.webm
tgradish convert references/uhh.mp4 -o $D/w2-spoofed.webm
tgradish convert references/pig.mp4 -o $D/w3-long-spoofed.webm
tgradish convert references/uhh.mp4 --preset emoji --length 3 -o $D/w4-emoji.webm
tgradish convert references/uhh.mp4 --preset emoji -o $D/w5-emoji-spoofed.webm
```

| File | Pack | Tests | Expected |
| --- | --- | --- | --- |
| `w1-sticker.webm` | video stickers | a plain 3 s sticker | accepted, plays 3 s |
| `w2-spoofed.webm` | video stickers | 4.8 s with the duration spoofed | accepted, plays 4.8 s |
| `w3-long-spoofed.webm` | video stickers | 12.6 s spoofed | accepted, plays 12.6 s |
| `w4-emoji.webm` | emoji | a plain 3 s emoji, 100x100 with transparency | accepted |
| `w5-emoji-spoofed.webm` | emoji | 4.8 s spoofed emoji | accepted, plays 4.8 s |

## Uploading

Through [@Stickers](https://t.me/stickers), into packs used only for
this (delete them afterwards with `/delpack`):

- `/newpack` for stickers: the `.tgs` files marked "sticker" in
  `CHECKLIST.md`, and the `w1` to `w3` WebM files. If the bot asks for one
  kind of sticker per pack, make one pack per kind.
- `/newemojipack` for custom emoji: the `.tgs` files marked "emoji", and
  `w4` and `w5`.

Send each file, then any emoji for it. Note the bot's answer when it
refuses one, word for word.

## What to look at

In each app you have (Android, iOS, Telegram Desktop, web.telegram.org/a
and /k, macOS), open the packs and send a few of the stickers in a chat:

- does it show at all, and does it move;
- are pixels sharp, with no thin lines between neighbouring colours (the
  seams 1.x had) and no coloured fringes along edges;
- do the WebM ones play their whole length, and loop.

Fill in the columns of `references/t9-probes/CHECKLIST.md` (and the
WebM table above, in a copy), or just describe what differs from
"Expected". Screenshots help where something looks wrong.
