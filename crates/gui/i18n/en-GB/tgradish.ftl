### The window's words in British English, which every other language
### falls back on for what it lacks.

# The language's name for itself, as the Settings dialog lists it.
language-name = English
###
### A comment above a message says where it is shown, or what it means
### when its words don't. Arguments, as { $count }, are filled in by the
### window. Counts pick a form with [one] and *[other]; another language
### may have more forms, such as [few] and [many], and its own variants
### for them. Key names, as Ctrl, can be put the way keyboards are
### labelled in the language. Units, as s, px, fps and KiB, are left as
### they are: the window shows them the same in every language.
###
### { other-message } puts in another message's words, as where a hint
### names a button, so the two stay the same; { -term } does so for a
### term, which is only put in others.


## Shared by many places

# A button that dismisses a message at the bottom of the window.
ok = OK
close = Close
# The small button beside a setting a preset can set, back to the preset's
# value when it has been changed.
back-to-preset = Back to the preset's value
# Beside a number the window picks unless asked for one: that it should be
# the number typed next to it.
custom = Custom
    .hint = Pick { custom } to set it
# What a result is, in the list of files and over the settings.
webm-sticker = WebM sticker
webm-emoji = WebM emoji
tgs-sticker = TGS sticker
tgs-emoji = TGS emoji
image-sticker = Image sticker
image-emoji = Image emoji
# Counts, as parts of longer messages.
count-colours = { $count ->
    [one] { $count } colour
   *[other] { $count } colours
}
count-frames = { $count ->
    [one] { $count } frame
   *[other] { $count } frames
}
count-layers = { $count ->
    [one] { $count } layer
   *[other] { $count } layers
}
count-rectangles = { $count ->
    [one] { $count } rectangle
   *[other] { $count } rectangles
}
count-tracks = { $count ->
    [one] { $count } track
   *[other] { $count } tracks
}
# How much of a limit something uses, over a bar: a result's size, or a
# sticker's length.
of-limit = { $used } of { $limit }
# Telegram's verdict on a result, under it and when inspecting a sticker.
telegram-accepts = Telegram should accept it


## The window

# The application's settings file couldn't be read on start.
settings-unreadable = The settings couldn't be read, so the defaults are used: { $error }
presets-unreadable = Your presets couldn't be read, so only the built-in ones are offered: { $error }
# Files open when the window last closed, which aren't there any more.
files-gone = { $count ->
    [one] { $count } file is gone since last time
   *[other] { $count } files are gone since last time
}
# The files open now couldn't be saved to be opened again next time.
session-unsaved = The open files couldn't be kept for next time: { $error }
# A setting changed outside the Settings dialog, such as the theme, which is
# saved at once.
setting-unsaved = The change couldn't be saved to the settings file: { $error }
# Over the window while files are dragged over it.
drop-to-add = Drop to add


## The list of files, on the left

settings = Settings
light-theme = Switch to the light theme
dark-theme = Switch to the dark theme
inspect = Inspect a sticker: check a file against Telegram's rules
add-files = Add files…
# The menu beside Add files…
add-folder = A folder of frames…
    .hint = Its images, in name order, as one animation
add-images = Images as one animation…
    .hint = The images picked, in name order
add-pasted = From the clipboard
    .hint = Copied files, paths or an image (Ctrl+V)
# The file picker's name for the images it shows.
images-filter = Images
convert-all = { $count ->
    [0] Convert all
   *[other] Convert all ({ $count })
}
    .hint = Every file without an up-to-date result
remove-all = Remove all
no-files = Files you add show up here.
remove = Remove
remove-file = Remove it from the list
# A file's button that makes its result, before it has one.
convert = Convert
    .hint = Make its sticker or emoji
row-convert-again = Settings changed: convert again
# A file's state, under its name.
row-next = next
row-waiting = waiting, { $ahead } ahead
# After the size of a file's result, once its settings no longer match it.
row-stale = { $size } · settings changed
row-failed = failed
row-exists = a file is in the way
row-stopped = stopped
# The large area shown while no files are open.
drop-here = Drop videos, GIFs, images or Aseprite files here
drop-here-note = Videos become WebM stickers and pictures image stickers; pixel art can also become a crisp TGS animation.
paste = Paste
    .hint = Ctrl+V
# A file's name in the list when it is more than its first input.
name-pasted = pasted image
name-frames = { $name } (frames)
name-and-more = { $name } and { $more } more
# When a file's name has characters no font, the window's or the system's,
# can draw: each is shown by its code, as U+2FE0.
no-font = { $count ->
    [one] No font was found for { $codes }, neither among the window's own nor the system's, so it is shown by its code. The file itself is fine.
   *[other] No font was found for { $codes }, neither among the window's own nor the system's, so they are shown by their codes. The file itself is fine.
}


## The bar under the preview of the file picked

stop = Stop
stopping = Stopping…
converting-note = Converting: progress is beside the preview
# A file waiting to be converted, which can be taken out of the queue.
dont-convert = Don't convert
next-to-convert = Next to convert
waiting-for = Waiting for { $ahead } more
convert-again = Convert again
# The keys that convert the file picked, shown on its Convert button.
convert-keys = Ctrl+Enter
convert-hint = Makes it with these settings
convert-again-hint = Makes it again with the same settings
-save-settings = Save current settings as preset
save-preset = { -save-settings }…
    .hint = { -save-settings }: keep them as a preset of your own, for any file to start from
failed = Failed: { $error }
in-the-way = A file tgradish didn't make is in the way:
replace-it = Replace it
stopped = Stopped
show-in-folder = Show in folder
# Before the folder a file's result goes to.
saves-to = Saves to { $folder }
    .hint = Where results go is set in { settings }


## Reading a file to show it

reading-crashed = tgradish crashed while reading this
reading-stopped = Reading it stopped
no-frames = ffmpeg found no frames in it
no-frames-there = ffmpeg found no frames there
no-frame-there = ffmpeg found no frame there
art-no-frames = It has no frames
result-no-frames = The result has no frames


## The result's small picture over the preview

# Its label: how large it is shown, as a share of its own pixels, then what
# it is, as "512×512, 2.40 s".
inset-label = Result at { $scale }%
    .plain = Result
    .drag = Drag it to another corner
inset-smaller = Smaller
inset-larger = Larger


## Over the preview

show-input = Input
show-result = Result
# Why the result can't be shown yet.
convert-first = Convert it first
tgs-unseen = This window can only show .tgs results it made itself; convert again to see it
# A menu of the folders the file and its result are in; "Folder" where
# there is no room for more.
show-folder = Show in folder
    .short = Folder
folder-input = Input file
    .pasted = A pasted image has no file of its own
folder-result = Result
# Turns on the result's small picture.
result-preview = Result preview
    .short = Preview
    .hint = What the result will look like, small, beside the input
zoom-out = Zoom out
    .hint = Show all of it
    .all = All of it shows
# The result no longer matches the settings; "⚠" alone where there is no
# room for the word.
outdated = Outdated
    .hint = The result was made with earlier settings
# How the mouse works on the preview.
input-hint = Drag to crop · wheel to zoom · right button to pan
crop-hint = Drag the crop or its edges · double-click for all
result-hint = Wheel to zoom · right button to pan
unreadable = Couldn't read it: { $error }
reading = Reading…


## The timeline under the preview

# The play button's hover: the keys that work over the preview. A line
# can't start with [, so it is written {"["} there.
play-keys =
    Space: play or pause
    ← →: a frame back or on, with Shift ten
    Home, End: the first or last frame used
    {"["} ]: start or end the used part at the frame shown
    Ctrl+← →, Alt+← →: move its start or its end
nothing-to-play = One frame is used: there is nothing to play
# Whether the timeline counts seconds or frames.
in-seconds = Seconds
    .hint = Times in seconds
in-frames = Frames
    .hint = Times as frame numbers, from 1
    .unknown = Where the input's frames are isn't known
part-start = Where the used part starts
    .frame = Where the used part starts: its first frame
part-end = Where the used part ends
    .frame = Where the used part ends: its last frame
# Where the pointer is over the timeline, counting frames from 1.
frame-number = frame { $number }
# Picking the frame a picture is made from.
frame-before = The frame before (←)
    .first = This is the first frame
frame-after = The frame after (→)
    .last = This is the last frame
frame-picked = The frame


## What to make, beside the preview

make = Make
format-video = Video
    .hint = A video sticker or emoji: VP9 in WebM, from any video or picture
format-animation = Animation
    .hint = An animated sticker for pixel art: each pixel becomes a shape, so it stays crisp at any size and the file stays small. Best for custom emoji, which are only 100 × 100 px. Not for ordinary video or photos: use { format-video } for those
format-image = Image
    .hint = A static sticker or emoji: one frame of a video, or a picture, as WebP
target-sticker = Sticker
target-emoji = Custom emoji
# What Telegram asks of what is being made.
needs-webm-sticker = WebM, 512 px on the longer side, up to 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, up to 64 KiB
needs-tgs = TGS, square, up to 64 KiB
needs-image-sticker = WebP, 512 px on the longer side, up to 512 KiB
needs-image-emoji = WebP, 100 × 100 px
not-pixel-art = { format-animation } is for pixel art, drawn as shapes so it stays crisp. This doesn't look like pixel art: { format-video } suits it better.
preset = Preset
    .hint = How hard to work on the result; your own values below go on top
    .image = A picture needs no preset: it is saved lossless where that fits, otherwise at the best quality that does
    .broken = This preset's file is broken
manage-presets = Manage…
    .hint = Rename, describe or delete your presets
# The built-in presets, from the quickest; each says what it does for
# videos (.webm) and for animations (.tgs).
preset-fast = Fast
    .webm = Fits only the bitrate, with the fast encoder: seconds, but blurrier
    .tgs = Under a second, a few percent larger
preset-balanced = Balanced
    .webm = Tries a few frame rates and keeps the one that looks best
    .tgs = Between fast and best
preset-best = Best
    .webm = Like balanced, with the slowest encoder: minutes for long videos
    .tgs = The smallest file, in a few seconds
# One of the user's own presets, with what they said it is.
preset-described = { $name }: { $description }


## Settings: the picture

picture = Picture
# Shown in a setting's place until the file has been read.
once-read = Shown once the input is read
crop = Crop
    .hint = The part of the input to use, in its pixels. Drag on the preview to change it
# The four fields around the small picture of the crop.
margin-top = Top margin: pixels cut off the top
margin-left = Left margin: pixels cut off the left
margin-right = Right margin: pixels cut off the right
margin-bottom = Bottom margin: pixels cut off the bottom
width = Width
height = Height
whole-picture = Whole picture
    .hint = All of the picture
    .exact = All of the picture, or as much as the exact scale takes from its top left
middle = Middle
    .hint = As much of the middle as the exact scale takes
aspect-ratio = Aspect ratio
    .hint = The width to height ratio the crop keeps while you drag it
shape-free = Free
    .hint = Any shape
shape-preserve = Preserve
    .hint = The input's own shape
shape-custom = Custom
    .hint = A shape you type: as a ratio, or as the result's size
    .pick = Pick { shape-custom } to type a shape
# Beside the ratio: the size of the result, which can be typed instead.
ratio-result = Result
    .fixed = The result is squared or has margins, so its size doesn't follow the crop
square = Force into square
    .hint = How the result becomes square: stickers needn't be, emoji must
    .tgs = How the art becomes square, as TGS stickers are
square-off = Off
    .hint = The result keeps the crop's shape
    .emoji = Emoji must be square
    .tgs = TGS stickers are always square
square-pad = By padding
    .hint = Transparent bars fill the rest of the square
    .tgs = Transparent margins fill the rest of the square
square-crop = By cropping
    .hint = The middle fills the square and the rest is cut off: the preview dims it
    .tgs = The middle fills the square and the rest is cut off
square-stretch = By stretching
    .hint = The picture is stretched to the square
    .tgs = The art is stretched to the square: its pixels stop being square
# Why cropping or stretching into a square can't be picked.
square-exact = A fixed integer scale keeps all of the picture: pad it instead
scaling = Scaling
    .hint = How the input's pixels become the result's
scaling-auto = Auto
    .hint = Sharp pixels for pixel art made twice as large or more, smooth otherwise
scaling-smooth = Smooth
    .hint = Neighbouring pixels blend, in linear light so edges don't darken
scaling-sharp = Sharp pixels
    .hint = Each pixel becomes a block; only block edges between result pixels blend
scaling-integer = Integer
    .hint = Each pixel becomes the same whole number of result pixels, or whole blocks of them become one; transparent margins fill the rest
integer-scale = Integer scale
    .hint = How many result pixels each input pixel becomes, or how many input pixels make one
    .only = Only with { scaling-integer } scaling
scale-largest = Largest that fits
    .hint = The largest whole multiple that keeps the crop within the result
scale-fixed = Fixed
    .hint = This scale; the crop can be at most what it fits into the result
smaller = Smaller
larger = Larger
scale-field = Type 3 for three times as large, or 1/3 for a third
    .pick = Pick { scale-fixed } to choose a scale
scale-unused = Whole scales apply with { scaling-integer } scaling only
scale-unsuited = The crop doesn't suit this scale yet
# What a whole scale, as ×3 or ×1/2, makes of the crop; then, if the result
# has transparent margins, the size of the whole result.
scale-makes = { $scale } makes the crop { $width } × { $height } px
scale-makes-padded = { $scale } makes the crop { $width } × { $height } px; transparent margins fill the rest of the { $box_width } × { $box_height }
scale-shrinks = The crop is larger than the result, so it shrinks with sharp pixels: a fixed scale like ×1/2 shrinks it by whole blocks
margins = Margins
    .hint = Rows and columns that are transparent in every frame
auto-crop = Auto crop empty margins
    .hint = Cuts the art to the smallest rectangle with visible pixels in any frame, so it fills the square
    .after = Applies after the crop above
art-pixel = Art pixel size
    .hint = How many input pixels one pixel of the art takes. Pixels off its grid are moved onto it
art-pixel-detected = Detected
    .hint = Found from the art: its smallest blocks of one colour


## Settings: time

time = Time
used-part = Used part
    .hint = Where the part of the input used starts and ends; the bar under the preview sets it too
# Between the fields of the used part's start and end.
part-from = From
part-to = to
part-first = Its first frame
part-last = Its last frame
all-of-input = All of the input
# Under the used part, in the count the fields don't use.
part-seconds = { $from } to { $to } of { $length }
part-frame = Frame { $number } of { $count }
part-frames = Frames { $first } to { $last } of { $count }
speed = Speed
    .hint = How many times as fast as the input the result plays
# The choices beside a number the window can pick itself, as the speed: what
# it picks, and how.
from-preset = Preset's
    .speed = The speed the preset sets
    .duration = The duration the preset sets
from-input = Input's
    .speed = As fast as the input plays
    .rate = The input's rate at the speed, at most 60 fps
fits-3-s = Fits 3 s
    .hint = As fast as fits the used part into 3 seconds
fit-3-s = Fit 3 s
    .hint = Plays the used part in 3 seconds
# How long the result plays: all of the used part, or as much of it, in the
# input's time, as fits 3 seconds.
plays = Plays { $played }
plays-cut = Plays { $kept } of the { $used }: the rest is cut off
plays-spoofed = Plays all { $played }: Telegram is told it is shorter
frame-rate = Frame rate
    .hint = Fewer frames a second leave more bytes for each; more than the input's would only repeat them
    .sequence = How fast the frames of the sequence play
    .sheet = How fast the frames of a sequence or sprite sheet play
default-rate = Default
    .hint = Ten frames a second
over-3-s = Over 3 seconds
    .hint = Telegram refuses stickers that say they last longer than 3 seconds
spoof-auto = Spoof when longer
    .hint = Stickers longer than 3 seconds get a short duration written into them: Telegram accepts them and plays all of it
spoof-always = Always spoof
    .hint = Even short stickers get the short duration
spoof-never = Cut at 3 s
    .hint = Keeps the file honest and cuts it at 3 seconds
spoof-to = Spoof to
    .hint = The duration written into the file, which Telegram reads
    .honest = Nothing is spoofed when { spoof-never } is picked
default-duration = Default
    .hint = A short duration Telegram accepts
frame = Frame
frame-at = Frame at
    .hint = The time of the frame the picture is made of; the bar under the preview picks it too


## Settings: size and quality of a WebM

size-and-quality = Size and quality
# A value a preset sets, beside a number the window picks unless asked for one.
from-preset-value = Preset's
    .hint = The value the preset sets
# How the result is made to fit the size limit; .lossless is the hover of
# the ways a quality can't go with, while lossless encoding is ticked.
fitting = Getting under the limit
    .hint = What tgradish changes to land just under the size limit
    .lossless = Lossless encoding keeps no bitrate or quality to fit: picking this unticks { lossless-encoding }
fit-auto = Best looking
    .hint = Tries a few frame rates, fits the bitrate for each and keeps the one most like the input
fit-bitrate = Bitrate
    .hint = Fits the bitrate at the chosen frame rate
fit-crf = Quality
    .hint = Fits the constant quality value (CRF)
fit-fps = Frame rate
    .hint = Fits the frame rate at constant quality
fit-length = Length
    .hint = Fits the length at constant quality, cutting the end
fit-off = Off
    .hint = Encodes once with the values below, even if it comes out too large
encoder = VP9 encoder
    .hint = Faster encoding looks worse at the same size
encoder-fast = Fast
    .hint = Seconds; visibly worse at the same size
encoder-balanced = Balanced
encoder-best = Best
    .hint = Slow: minutes for long videos
# Why a setting of fitting can't be changed.
fitting-off = { fit-off } encodes once
attempts = Encodes up to
    .hint = How many times fitting may encode, at most
attempts-default = Default
    .hint = Enough to land within a few percent of the limit for most inputs
# After the number of encodes, which its field shows alone.
attempts-times = { $count ->
    [one] time
   *[other] times
}
search-within = Search within
    .hint = The range fitting searches, in the fitted value's unit
search-auto = Automatic
    .hint = As wide as suits the way of fitting and the input
# Between the two ends of the range searched.
range-to = to
crf = Quality (CRF)
    .hint = Constant quality from 0, the best, to 63
    .lossless = Lossless encodes without a quality
    .fitted = Fitting finds the quality
    .bitrate = Fitting finds a bitrate, not a quality
crf-default = Default
    .hint = A good quality at sticker sizes
lossless = Lossless
lossless-encoding = Lossless encoding
    .hint = Only small for tiny or still pictures
    .switches = Only small for tiny or still pictures. Lossless encoding keeps no bitrate or quality to fit: ticking this switches fitting to { fit-fps }
bitrate = Bitrate
    .hint = Used unless only a quality is set
    .fitted = Fitting finds the bitrate
    .constant = Encoded at a constant quality, not a bitrate
bitrate-auto = From the size limit
    .hint = As much as the length lets fit


## Settings: size and quality of a TGS animation

tgs-encoder = TGS encoder
    .hint = How hard to look for a small encoding
tgs-fast = Fast
    .hint = Under a second; a few percent larger
tgs-balanced = Balanced
tgs-best = Best
    .hint = A few seconds; the smallest
too-large = If it doesn't fit
    .hint = What happens when the art doesn't fit into 64 KiB as it is
never-change = Never change the art; report it instead
    .set = The art is never changed, as set above
give-up = Give up first
    .hint = When the art has to change to fit: its motion or its detail
give-up-either = Either
    .hint = Whatever changes the art least for the bytes it saves
give-up-motion = Motion
    .hint = Frames merge and drop first: full detail, choppier motion
give-up-detail = Detail
    .hint = The picture coarsens first: smooth motion, less detail
ways-to-fit = Ways to fit it
    .hint = What fitting may do to the art to fit it into 64 KiB, least visible first
reduce-snap = Snap to the pixel grid
    .hint = Moves stray pixels onto the art's grid
reduce-colours = Merge similar colours
    .hint = Colours that hardly differ become one
reduce-specks = Remove specks
    .hint = Single pixels that differ from all around them
reduce-frames = Merge similar frames
    .hint = Frames that hardly differ show one picture
reduce-drop = Drop frames
    .hint = Shows every other frame, or fewer
reduce-resolution = Lower the resolution
    .hint = Fewer, larger art pixels
reading-art = Reading the art
sprite-sheet = Sprite sheet
    .hint = Frames laid out in a grid of equal cells, row by row
read-as-sheet = Read as a sprite sheet
# The sprite sheet's columns × rows.
sheet-cells = Cells
sheet-frames = Frames
sheet-all = All
    .hint = Every cell but transparent ones at the end


## Settings: an Aseprite file's tags and layers

aseprite-file = Aseprite file
unreadable-choices = It doesn't read with the tag and layers chosen
read-all = Read all of it
tag = Tag
    .hint = Picks the frames of a tag: as where it starts and ends, or read in its own order where it plays backwards or there and back
all-frames = All frames
    .hint = Every frame of the file
    .preset = The preset reads one tag
# A tag's frames, counted from 1 as Aseprite counts them, and how it plays.
tag-frame = Frame { $number }
tag-frames = Frames { $from } to { $to }
tag-reverse = { $frames }, in reverse
tag-ping-pong = { $frames }, there and back
tag-pong-ping = { $frames }, back and there
layers = Layers
    .hint = Which layers show, the top one first; a hidden group hides the layers in it
    .group-hidden = Its group is hidden
    .file = Back to the layers the file shows
    .preset = Back to the layers the preset shows


## Settings: encoder options, folded away under the rest

encoder-options = Encoder options
# Options of the libvpx-vp9 encoder, by name: name = value.
vp9-options = Encoder options by name, like tune-content = screen for flat graphics. ffmpeg -h encoder=libvpx-vp9 lists them
    .name = For example: tune-content
    .value = For example: screen
add-option = Add an option
examples = Examples:
# What each example option does.
example-tune-content = Flat graphics and screen recordings: sharper edges
example-aq-mode = Spends bits where the picture is complex
example-sharpness = Keeps more fine detail, at some cost in smoothness
example-arnr-strength = Less noise reduction across frames
example-g = A keyframe at least every 60 frames
example-qmax = Never worse than quality 50 anywhere
ffmpeg-arguments = ffmpeg arguments
    .hint = Raw arguments added before the output, split like a shell would
    .placeholder = For example: -qmin 4
    .system = Only with the system's ffmpeg, which { settings } can pick


## What converting will do, beside the preview, step by step

plan-video = The video will be:
plan-picture = The picture will be:
plan-animation = The animation will be:
plan-art = The art will be:
plan-tag = Played as its “{ $tag }” tag
plan-tag-reverse = Played as its “{ $tag }” tag, in reverse
plan-tag-ping-pong = Played as its “{ $tag }” tag, there and back
plan-tag-pong-ping = Played as its “{ $tag }” tag, back and there
plan-cropped = Cropped to { $width } × { $height }
plan-frame = Frame at { $time } taken
plan-scaled = Scaled to { $width } × { $height }
# Made wider or taller than its shape, by this factor.
plan-stretched-across = Stretched ×{ $factor } across
plan-stretched-down = Stretched ×{ $factor } down
plan-middle-cut = Middle cut to { $width } × { $height }
plan-padded = Padded to { $width } × { $height }
plan-trimmed = Trimmed to its visible pixels
plan-grid = Pixels aligned to a { $scale } × { $scale } grid
plan-square-crop = Cropped to a square
plan-square-stretch = Stretched to a square
plan-square-pad = Padded to a square
# The part used: from and to as seconds, the first without its unit.
plan-cut = Cut to { $from }–{ $to } of { $length }
plan-faster = Sped up by a factor of { $factor }
plan-slower = Slowed down by a factor of { $factor }
plan-cut-3-s = Cut at 3 s of { $played }
plan-spoofed = Duration spoofed, so all { $played } plays
plan-saved = Saved as { $name }


## Settings: the same conversion as commands, folded away under the rest

commands = Commands
copy = Copy
commands-posix = The same conversion on the command line, for a POSIX shell:
commands-powershell = The same conversion on the command line, for PowerShell:
pasted-temporary = A pasted image is kept in a temporary folder, gone when the window closes
ffmpeg-commands = The ffmpeg commands of the first encode; fitting changes the rate, and maybe the frame rate, between encodes:
    .builtin = What the built-in ffmpeg does for the first encode, as commands for the system's ffmpeg; fitting changes the rate, and maybe the frame rate, between encodes:
no-ffmpeg-commands = No ffmpeg commands: { $why }
    .unread = the input isn't read yet
    .art = ffmpeg reads a PNG each conversion draws of the pixel art


## The Settings dialog

save = Save
    .hint = Keep these settings for next time
saved-to = Saved to { $path }
not-saved = Couldn't save: { $error }
no-home = There is no home folder to keep settings in
about-and-licences = About and licences…
settings-note = Changes apply now; { save } keeps them between restarts
ffmpeg-use = Use
    .hint = Videos are read, and WebM stickers made, with ffmpeg
ffmpeg-auto = Automatic
    .hint = The ffmpeg built in if this build has one, else the system's
ffmpeg-builtin = Built in
    .hint = The ffmpeg inside tgradish
    .missing = This build of tgradish has no ffmpeg in it
ffmpeg-system = The system's
    .hint = ffmpeg and ffprobe on PATH; allows raw ffmpeg arguments
ffmpeg-program = Program
    .hint = An ffmpeg executable, or a folder with ffmpeg and ffprobe; overrides the choice above
# What the empty field of the ffmpeg program shows.
ffmpeg-found = { $path }, found on PATH
for-example = For example: { $example }
choose = Choose…
check = Check
    .hint = Whether this ffmpeg can make stickers
# What checking ffmpeg found, its version as "8.0".
ffmpeg-ready = ffmpeg { $version } with VP9: ready
ffmpeg-no-vp9 = ffmpeg { $version } can't encode VP9 with libvpx
results = Results
save-results = Save them
next-to-input = Next to each input
in-a-folder = In a folder
    .unchosen = Until a folder is chosen, results are saved next to each input
existing-files = Existing files
    .hint = Results tgradish made are always replaced when made again
replace-in-the-way = Replace any file that is in the way
window = Window
language = Language
    .hint = The window's language; the command line's stays English
as-the-system = As the system
theme = Theme
    .hint = The button beside ⚙ switches between light and dark too
theme-light = Light
theme-dark = Dark
files = Files
reopen-files = Reopen the files of last time, with their settings
scrolling = Scrolling
smooth-scrolling = Smooth: ease it over a few frames
new-files = New files
preset-for-webm = Preset for WebM
preset-for-tgs = Preset for TGS


## The result, beside the preview

last-conversion = Last conversion
path-in-the-way = { $path } is in the way
stopped-early = Stopped before it was done
result = Result
    .stale = Result, from earlier settings
# A WebM result: its size in pixels, frame rate and length.
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, duration spoofed
changed-to-fit = Changed to fit: how is listed below
replaced-earlier = Replaced an earlier result made by tgradish
converting = Converting
# The stages of making a TGS animation, each a bar.
stage-reading = Reading the art
stage-drawing = Drawing it as shapes
stage-kept-too-large = Kept as it is: about { $size }, too large
stage-kept = Kept as it is
stage-fitting-steps = { $count ->
    [one] Fitting into 64 KiB: { $count } step, about { $size }
   *[other] Fitting into 64 KiB: { $count } steps, about { $size }
}
stage-fitting-from = Fitting into 64 KiB from { $size }
stage-fits = Fits as it is
stage-fitting = Fitting into 64 KiB, if it has to
stage-compressing = Compressing
    .hopeless = Compressing quickly: it can't fit
# Each encode of a WebM, a bar: its frame rate and rate, then its size as
# a share of the limit, and whether that fits.
starting = Starting
encode = Encode { $number }: { $fps } fps, { $rate }
encode-done = Encode { $number }: { $fps } fps, { $rate } → { $size }, fits
    .too-large = Encode { $number }: { $fps } fps, { $rate } → { $size }, too large
rate-quality = quality { $crf }
rate-lossless = lossless
encodes-up-to = { $count ->
    [one] Up to { $count } encode, until one lands just under the limit
   *[other] Up to { $count } encodes, until one lands just under the limit
}
# How a TGS animation was made: its art's size, then how many colours and
# frames it has.
art-size = { $width } × { $height } art pixels
    .scaled = { $width } × { $height } art pixels, each { $scale } × { $scale } input pixels
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Plays { $tempo }× as fast as the input
art-off-grid = Most of it looks like { $scale }× art with some pixels off the grid; an art pixel size of { $scale } snaps them
# The shapes of a TGS animation, and how large its JSON is before it is
# compressed.
art-shapes = { $layers } of { $rectangles }, { $size } unpacked
art-before-fitting = About { $size } as it is; changed to fit:
# A change made to fit, then the size after it.
art-step = { $change }, about { $size } after
step-snapped = pixels snapped to a grid of { $scale }
step-colours = colours closer than { $distance }% merged
step-frames = frames that change under { $changed }% merged
step-dropped = { $share }% of frames dropped
step-specks = single stray pixels removed
step-resolution = resolution lowered to { $factor }%
# The table of a WebM's encodes; the headers' hovers say what each column is.
column-number = Each encode in turn; ✔ the one kept
column-fps = Frames a second
column-rate = Bitrate, or the quality it was encoded at
column-size = size
    .hint = Share of the size limit, green where it fits
column-alike = alike
    .hint = How like the input it looks (SSIM)
# A quality, short, in the table's rate column.
rate-quality-short = q { $crf }
# Telegram's problems with a WebM or image result.
issue-not-webm = Its container is { $container }, not WebM
issue-no-video = It has no video track
issue-not-vp9 = Its codec is { $codec }, not VP9
issue-audio = It has an audio track
issue-size = Its size is { $width } × { $height } px, where Telegram takes { $side } × { $side } px
    .sticker = Its size is { $width } × { $height } px, where Telegram takes one side of { $side } px and the other at most that
issue-too-big = It is { $size }, over the limit of { $limit }
issue-too-long = Its header says it lasts { $seconds }, over the limit of { $limit }
issue-too-many-fps = Its frame rate is { $fps } fps, over the limit of { $limit } fps
issue-animated = It is animated, where image stickers are one picture


## Inspecting a sticker

inspect-another = Inspect another…
# The file picker's name for the files it shows.
stickers-filter = Stickers
inspect-webm-sticker = A video sticker
inspect-webm-emoji = A video emoji
inspect-image-sticker = An image sticker
inspect-image-emoji = An image emoji
inspect-tgs = A vector animation, { $width } × { $height } at { $fps } fps
inspect-size = Size
    .hint = Telegram refuses larger files
    .tgs = Telegram refuses larger stickers
inspect-duration = Duration
    .hint = Telegram reads the duration in the header; spoofing makes it short
    .spoofed = { $seconds } s in the header, spoofed
    .no-header = { $seconds }, none in the header
inspect-plays = Plays
    .hint = How long the video really is
inspect-frame-rate = Frame rate
    .value = { $fps } fps of { $most }, { $frames }
inspect-picture = Picture
    .none = none
# A WebM's picture: its size and codec, and whether it has transparency.
inspect-video = { $width } × { $height } px, { $codec }
    .alpha = { $width } × { $height } px, { $codec }, with transparency
inspect-webp = { $width } × { $height } px, WebP, lossless
    .alpha = { $width } × { $height } px, WebP, lossless, with transparency
    .lossy = { $width } × { $height } px, WebP, lossy
    .lossy-alpha = { $width } × { $height } px, WebP, lossy, with transparency
inspect-audio = Audio
inspect-title = Title
inspect-made-with = Made with
    .hint = The programs that wrote the file, as it says
    .image = The program its metadata names
    .unknown = unknown
inspect-note = Note
    .hint = Text in the file's padding, which players skip
inspect-name = Name
    .hint = The animation's own name, which Telegram doesn't show
inspect-length = Length
    .hint = Animated stickers last at most 3 seconds
    .value = { $seconds } of 3.00 s, { $frames }
inspect-cost = Drawing cost
    .hint = Shapes plus 9 for each layer, as Telegram counts them
inspect-layers = Layers
inspect-shapes = Most shapes in a layer
inspect-refuses-more = Telegram refuses more
inspect-points = Most points in a fill
    .hint = Path points one fill paints; Telegram refuses more
inspect-unpacked = Unpacked
    .hint = Telegram Desktop doesn't play larger animations
inspect-features = Features
    .hint = Lottie features beyond shapes and transforms


## What went wrong, as converting or reading a file reports it

# A conversion that stopped on a fault of tgradish's own.
crashed = tgradish crashed: { $details }
ffmpeg-not-found = ffmpeg wasn't found: { $details }
# ffmpeg or ffprobe ended with an error; its own output follows, as it is.
program-failed = { $program } failed ({ $status }):
unreadable-file = Couldn't read { $name }: { $details }
no-video = { $name } has no video in it
no-frame-at = { $name } has no frame at { $seconds }
nothing-fits = Nothing fits the { $limit } limit: the smallest encode was { $smallest }. A shorter used part, a lower frame rate or a wider range to search within can help
clipboard-failed = Couldn't paste from the clipboard: { $details }


## Saving and managing presets

save-preset-title = Save settings as a preset
save-preset-note = This file's settings, for any file to start from, but for those that only suit it: sticker or emoji, crop, used part, tag, layers and sprite sheet
preset-name = Name
    .placeholder = my stickers
preset-description = Description
    .placeholder = optional
    .none = no description
preset-exists = A preset of this name is there already: saving replaces it
replace = Replace
name-it-first = Name it first
cancel = Cancel
# What a preset's name can't be.
preset-name-length = A preset's name has 1 to { $most } characters, not starting with a space
preset-name-characters = A preset's name has only letters, digits, spaces, - and _
preset-name-builtin = “{ $name }” is a built-in preset's name
preset-name-taken = Another preset is named “{ $name }”
preset-extended = “{ $other }” extends it: rename it there first
your-presets = Your presets
your-presets-note = Changes are kept as you make them. { preset-fast }, { preset-balanced } and { preset-best } are built in and stay as they are.
no-presets = None yet: { -save-settings }, under the preview.
preset-broken = Its file can't be read
delete-it = Delete it
deleted = Deleted “{ $name }”
undo = Undo
    .hint = Put it back
not-put-back = It couldn't be put back: { $error }


## About and licences

# Under the name and version; a link to the source code follows.
about-summary = Telegram stickers from videos and pixel art. MIT licence;
source-code = source code
# Over the licences' notices, which are shown as they are written, in
# English.
about-notices = tgradish is built from other people's work too. Their licences ask for these notices to go with every copy, so the program carries them:
ffmpeg-sources = The exact sources of the ffmpeg built in, with this release
