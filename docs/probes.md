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

So the server has limits like a parser's: at most 1500 to 2000 layers and
4096 shapes in a layer, and something near 1 MB of rectangles. Round 3
found that this last one counts shapes and layers, not bytes, and that the
paths were refused for their empty tangents, not their length.

## Third round: what the limits count

The second round left open whether the limit near 1 MB counts bytes or
something else, and why paths were refused with less JSON. Each probe here
is made so the candidates (bytes, numbers, arrays, objects, shapes,
layers, points) disagree about it, and later probes narrowed what was left.
Also: the packed size, exactly.

```console
X=references/pixelart/1x-uploaded
cargo run --release -p tgs-lab -- sizes references/t9-probes-3 \
  $X/banana.tgs $X/Kris_battle.tgs
```

Uploaded the same way as round 2. Not every file was uploaded: the
ladders stopped once the answer was clear.

### Results

Uploaded 2026-10-10 through Telegram Web:

| Probe | Shapes | Layers | Result |
| --- | ---: | ---: | --- |
| `packed-65536`: 01 packed to exactly 64 KiB by a long name | | | accepted |
| `packed-65537` | | | refused |
| `boards-in-turn-101` (1 MB of JSON) | 20 503 | 101 | accepted |
| `boards-in-turn-100-long-sizes`: the accepted round 2 probe, sizes written as `1.00001` (1.24 MB) | 20 300 | 100 | accepted |
| `boards-in-turn-110`, `-115` (1.15 MB) | 22 330, 23 345 | 110, 115 | accepted |
| `boards-in-turn-116`, `-117`, `-118` | 23 548 and up | 116 and up | refused |
| `groups-5000`: one square per group, 1000 groups to a layer | 20 000 | 5 | accepted |
| `groups-5800` | 23 200 | 6 | accepted |
| `groups-6500` (790 KB) | 26 000 | 7 | refused |
| `layers-1000-and-boards-50`, `-55`: layers of one square and of 200 | 14 150, 15 165 | 1050, 1055 | accepted |
| `layers-1000-and-boards-60` | 16 180 | 1060 | refused |
| `layers-1400-and-boards-74`, `layers-1300-and-boards-83`, `layers-1000-and-boards-95` | 20 622 to 23 285 | 1095 to 1474 | refused |
| `layers-1750`: one square each | 7000 | 1750 | refused |
| `path-4`, `paths-1`, `paths-8`, `short-paths-170`: paths with empty tangents, as 1.x and round 2 wrote them | | | refused, even 4 points |
| `path-standard`: `"c":true`, `[0,0]` tangents, no repeated point | | | accepted |
| `path-empty-tangents`: the same with `[]` tangents | | | refused |
| `path-no-c`, `path-repeated-point`: the same without `c`, or with the first point repeated | | | accepted |
| `path-4000-standard`: one path of 4000 points | | | accepted |
| `paths-standard-3`, `-5`, `-10`: 12 000 points and more under one fill | | | refused |
| `paths-2-and-boards-40`: 8000 points under one fill, over 8120 shapes in 42 layers | | | accepted |
| `path-4000-and-boards-60`: 4000 points over 12 180 shapes in 62 layers | | | accepted |
| `path-4000-in-3-groups`: 12 000 points under three fills in one layer | | | accepted |
| `path-4000-in-3-layers`: the same in three layers, shown together | | | accepted |
| `path-4000-in-10-layers`: 40 000 points under ten fills | | | accepted |
| `1x-banana-zero-tangents`: 1.x's `banana.tgs` with `[0,0]` tangents, keeping merge paths, strokes and a fractional `op` | | | accepted (the original is refused) |
| `1x-kris-battle-zero-tangents`: 1.x's `Kris_battle.tgs` the same way, 18 376 points in 30 layers | | | accepted (the original is refused) |

So:

- The `.tgs` may be exactly 64 KiB (65 536 bytes).
- Bytes of JSON don't count. The limit is on the whole animation's
  shapes, counted like tlottie (each group, rectangle, fill and transform
  is one), where a layer weighs like 8.2 to 8.9 shapes; the total must
  stay between 24 300 and 24 580 or below. One linear rule fits every
  upload of rounds 2 and 3. tgradish counts a layer as 9 and allows
  24 000 (`limits::telegram::cost`).
- Layers are capped on their own too: 1750 one-square layers cost less
  than that and were refused; 1500 were accepted.
- Paths need numbers for their tangents. pixelart2tgs 1.x wrote `[]`, so
  Telegram refuses its stickers now; with `[0,0]` they pass.
- The path points one fill paints (the paths before it in its group) have
  a cap of their own, between 8000 and 12 000. Spread over fills there can
  be many more, and they add less than 0.6 shapes a point to the size
  limit, maybe nothing. So outlines, one shape however many points, could
  fit more under that limit than rectangles, though they pack larger.

## Fourth round: WebM

The first round's WebM probes, and some more around the limits, uploaded
through `/newvideo` and `/newemojipack` (video emoji). @Stickers checks
WebM itself, with its own messages; the file keeps its name.

```console
D=references/t9-probes-3
ffmpeg -f lavfi -i testsrc2=size=512x512:rate=60:duration=3 -c:v libvpx-vp9 \
  -pix_fmt yuva420p -b:v 450k -an $D/w-fps-60.webm
ffmpeg -stream_loop 4 -i references/pig.mp4 -c copy pig-63s.mp4
tgradish convert pig-63s.mp4 -o $D/w-long-63s-spoofed.webm
ffmpeg -t 3 -i references/uhh.mp4 -vf scale=100:100 -c:v libvpx-vp9 \
  -pix_fmt yuva420p -b:v 120k -an $D/e-100-3s.webm
ffmpeg -i references/uhh.mp4 -vf scale=100:100 -c:v libvpx-vp9 \
  -pix_fmt yuva420p -b:v 80k -an e-100-long.webm
tgradish spoof e-100-long.webm -o $D/e-100-spoofed.webm
ffmpeg -t 3 -i references/uhh.mp4 -vf scale=512:512 -c:v libvpx-vp9 \
  -pix_fmt yuva420p -b:v 140k -an $D/e-512-3s.webm
```

The `pad` files are `w1-sticker.webm` or `e-100-3s.webm` grown to an exact
size by an EBML Void element at the end of the Segment, whose size is
raised to match.

Uploaded 2026-10-10 through Telegram Web:

| File | Pack | Result |
| --- | --- | --- |
| `w1-sticker.webm`: 3 s | video stickers | accepted |
| `w2-spoofed.webm`: 4.8 s spoofed | video stickers | accepted |
| `w-long-63s-spoofed.webm`: 63 s spoofed | video stickers | accepted |
| `w-pad-262144.webm`: exactly 256 KiB | video stickers | accepted |
| `w-pad-262145.webm` | video stickers | "File is too big. Video stickers may not exceed 256 KB." |
| `w-fps-60.webm`: 60 fps | video stickers | accepted |
| `w5-emoji-spoofed.webm`: 246 KB | video emoji | "File is too big. Video emoji may not exceed 64 KB." |
| `e-100-pad-65536.webm`: exactly 64 KiB | video emoji | accepted |
| `e-100-pad-65537.webm` | video emoji | "File is too big. Video emoji may not exceed 64 KB." |
| `e-100-spoofed.webm`: 4.8 s spoofed | video emoji | accepted |
| `e-512-3s.webm`: 512x512 | video emoji | "Video dimensions are invalid. Please check that the video is a square of exactly 100x100 pixels." |

So:

- Spoofing works for stickers and emoji, also for a minute.
- Stickers may be exactly 256 KiB, emoji exactly 64 KiB. tgradish made
  emoji up to 256 KiB, which Telegram refuses; it now fits them to 64 KiB.
- 60 fps is accepted, though the rules say 30. tgradish allows up to 60
  when asked and still picks at most 30 on its own, since that leaves more
  bytes per frame. Whether every app plays 60 fps stickers smoothly is
  still to be seen.
- Emoji must be exactly 100x100, as tgradish makes them.
