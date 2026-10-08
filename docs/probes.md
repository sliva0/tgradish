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

## Results of the first round

Uploaded 2026-10-08. Accepted and shown on Android, Desktop and web:
01, 02, 06, 07, 08 and 13 as stickers, 11 as an emoji. @Stickers refused
03, 04, 05, 09, 10 and 12. So:

- tgradish's default output needs none of the optional fields (01).
- Precomps and hold keyframes work (06, 07), if the encoder wants them.
- The limit is 3 seconds, not 180 frames: 30 fps is allowed (08), but 180
  frames at 30 fps were refused (09). tgradish keeps 60 fps, which times
  frames more finely and costs nothing.
- Emoji are 512x512 like stickers (11); 100x100 was refused (12).
  tgradish already writes them that way.
- Telegram has a limit the rules don't mention, which 03 (2 MB of JSON,
  138 layers of 288 squares), 04 (2700 layers) and 05 (5100 squares in one
  layer) went over. Stickers 1.x made with 31 layers and 660 KB of JSON
  were accepted in 2022. The second round finds the limit.

## Second round: Telegram's limits

Each group raises one thing until Telegram refuses it: layers, squares
in one layer, squares shown at once and over the whole animation, the size
of the JSON, long paths. The last ones are real art encoded by tgradish
with the most layers, rectangles or JSON of the test set.

```console
P=references/pixelart
cargo run --release -p tgs-lab -- limits references/t9-probes-2 \
  $P/Spamton_overworld_glitched_laugh.gif $P/animation_susie_cake.gif \
  $P/susie_fortnite.gif $P/gf1.gif
```

Send them one at a time to @Stickers after `/newanimated` (an album of
several files isn't checked the same way). Telegram's server checks each
`.tgs` as it is uploaded: an accepted one arrives as a sticker, renamed
`AnimatedSticker.tgs`, and the bot asks for its emoji; a refused one stays
a file under its own name, and the bot answers "File type is invalid.
Please convert your image to the .TGS format. See this guide for details."
Telegram Web does no check of its own, so the app doesn't matter.

### Results

Uploaded 2026-10-08 through Telegram Web:

| What | Accepted | Refused |
| --- | --- | --- |
| layers of one square | 60 to 1500 | 2000; 2700 (round 1) |
| squares in one layer (shapes as tlottie counts) | up to 4090 (4093) | 4100 (4103), 4500; 5100 (round 1) |
| layers of 200 squares in turn: squares, JSON | 20 000, 996 KB | 24 000, 1.2 MB; 39 744, 2 MB (round 1) |
| JSON made long by a padded name | 700 KB, 1.9 MB | |
| 20 paths of 4000 points, 962 KB | | refused |
| `susie_fortnite` as tgradish encodes it: 49 layers, 7716 shapes | accepted | |

So the server has limits like a parser's: about 1 MB of JSON (names may
not count), at most 1500 to 2000 layers and 4096 shapes in a layer.
tgradish now keeps under 1 000 000 bytes, 1500 layers and 4096 shapes
(`limits::telegram`), and `inspect` reports files over them.
