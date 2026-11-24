### tgradish's words in American English: only those that differ from
### British English, en-GB/tgradish.ftl, which says the rest and whose
### comments say what each message means. American spelling (color,
### license, gray), checking a box rather than ticking it, the serial comma
### in lists of three or more, and commas inside closing quotes.
###
### A message here is used in place of en-GB's whole, value and attributes
### apart: one whose value is as en-GB's but an attribute differs gives only
### that attribute. What one here puts in, as { square-off }, is put in from
### this file alone, so it is given here even where it says what en-GB does.

language-name = English (US)


## Shared by many places

count-colours = { $count ->
    [one] { $count } color
   *[other] { $count } colors
}


## Messages along the bottom

nothing-to-paste =
    .why = The clipboard has no files, file paths, or image


## The list of files, on the left

about-and-licences = About and licenses…
add-pasted =
    .hint = Copied files, paths, or an image
drop-here = Drop videos, GIFs, images, or Aseprite files here
drop-here-note = To convert into a video, animated, or static sticker or emoji


## What to make, beside the preview

preset =
    .image = A static sticker takes only the preset's picture settings, how it is squared and scaled: it is saved lossless where that fits, otherwise at the best quality that still fits
    .missing = There is no preset "{ $name }" anymore: pick another
manage-presets =
    .hint = Rename, describe, or delete your presets


## Settings: the picture

aspect-ratio =
    .square = TGS must be square, and "{ square }" is "{ square-off }," which keeps the crop's shape: picking this forces it into square again
square = Force into square
square-off = Off
scaling-smooth =
    .hint = Neighboring pixels blend
art-pixel-detected =
    .hint = Found from the pixel art: its smallest blocks of one color


## Settings: size and quality of a WebM

fitting =
    .lossless = Lossless encoding keeps no bitrate or quality to fit: picking this unchecks "{ lossless-encoding }"
fit-fps = Frame rate
lossless-encoding = Lossless encoding
    .switches = Only small for tiny or still pictures. Lossless encoding keeps no bitrate or quality to fit: checking this switches fitting to "{ fit-fps }"


## Settings: size and quality of a TGS animation

reduce-colours = Merge similar colors
    .hint = Colors that hardly differ become one


## Settings: an Aseprite file's tags and layers

tag =
    .hint = Picks the frames of a tag: as where it starts and ends, or read in its own order where it plays backward or there and back


## What keeps a file from being converted as it is, and the ways out

quoted-names =
    .last = "{ $first }," and "{ $second }"
    .more = "{ $name }," { $rest }


## The Settings dialog

accent = Accent color
    .hint = What marks the chosen, the hovered, and the focused
accent-system =
    .hint = The desktop's accent color, or Breeze's blue where it has none
accent-grey = Gray
accent-own = A color of your own
    .chosen = A color of your own, { $colour }


## The result, beside the preview

step-colours = colors closer than { $distance }% merged


## What went wrong, as converting or reading a file reports it

no-such-file = { $name } isn't there anymore
not-sticker = { $name } isn't a sticker: a WebM, WebP, or .tgs file
nothing-fits = Nothing fits the { $limit } limit: the smallest encode was { $smallest }. A shorter used part, a lower frame rate, or a wider range to search within can help


## Saving and managing presets

-save-settings = Save current settings as preset
save-preset-note = This file's settings, for any file to start from, but for those that only suit it: sticker or emoji, crop, used part, tag, layers, and sprite sheet
preset-name-characters = A preset's name has only letters, digits, spaces, -, and _
your-presets-note = Changes are kept as you make them. "{ preset-fast }," "{ preset-balanced }," and "{ preset-best }" are built in and stay as they are
preset-fast = Fast
preset-balanced = Balanced
preset-best = Best
no-presets = None yet: "{ -save-settings }," in the bar under the settings


## About and licenses

about-title = About and licenses
about-summary = Telegram stickers and emoji from videos, images, or pixel art. MIT license;
about-notices = tgradish is built from other people's work too. Their licenses ask for these notices to go with every copy, so the program carries them:
notice-licence = { $name }: license
notice-licence-notes = { $name }: license notes
notice-crates-licence =
    .more = { $licence }: { $crates }, and { $more } more
