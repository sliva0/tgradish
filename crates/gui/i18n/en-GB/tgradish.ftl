### tgradish's words in British English, which every other language
### falls back on for what it lacks.
###
### A comment above a message says where it is shown, or what it means
### when its words don't. Arguments, as { $count }, are filled in by
### tgradish. Counts pick a form with [one] and *[other]; another language
### may have more forms, such as [few] and [many], and its own variants
### for them. Key names, as Ctrl, can be put the way keyboards are
### labelled in the language. Units, as s, px, fps and KiB, are left as
### they are: tgradish shows them the same in every language.
###
### { other-message } puts in another message's words, as where a hint
### names a button, so the two stay the same; { -term } does so for a
### term, which is only put in others.

# The language's name for itself, as the Settings dialog lists it.
language-name = English (UK)


## Shared by many places

close = Close
# The small button beside a setting a preset can set, back to the preset's
# value when it has been changed.
back-to-preset = Go back to the preset's value
# Beside a number tgradish picks unless asked for one: that it should be
# the number typed next to it.
custom = Custom
    .hint = Pick "{ custom }" to set it
# What a result is, in the list of files and over the settings.
webm-sticker = WebM sticker
webm-emoji = WebM emoji
tgs-sticker = TGS sticker
tgs-emoji = TGS emoji
image-sticker = Static sticker
image-emoji = Static emoji
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


## Messages along the bottom

# Each message says what happened first; under it, in smaller words where
# there is one, the file it is about, then a line of what it means, buttons
# that do something about it, and what went wrong as a program said it.
#
# The cross at the right of each message's first line, and under several,
# the link that dismisses them all.
dismiss = Dismiss
    .all = Dismiss all
# Beside a message said again while it showed, how many times it was said.
said-times = ×{ $count }
# The toggle of what went wrong, as a program said it, when that is long.
details = Details
# The settings file couldn't be read as tgradish started, as when it was
# changed by hand or by another version of tgradish: the defaults are used.
settings-unreadable = The settings file can't be read, so the defaults are in use
# A setting changed outside the Settings dialog, such as the theme, which is
# saved at once, couldn't be saved to the settings file; .unreadable as the
# file can't be read.
setting-unsaved = Your change couldn't be saved to the settings file
    .unreadable = Your change wasn't saved: the settings file can't be read
# Under settings-unreadable and setting-unsaved.unreadable, before buttons
# of fresh-settings, read-again, copy-path and show-folder.
settings-file-note = It may have been edited by hand or by another version of tgradish. Changes can't be saved until it is fixed or replaced; nothing in it is lost.
# A button that writes the settings in use in place of a settings
# file that can't be read. .saved and .kept say it did: the file shown is
# the old one, kept beside the new under the name given. .failed says it
# couldn't, why under it.
fresh-settings = Start with fresh settings
    .hint = Write the settings in use now as a new settings file; the old one is kept beside it
    .saved = Fresh settings are saved
    .kept = The old file is kept as { $name }, beside them
    .failed = Fresh settings couldn't be written
# A button that reads a file that couldn't be read again, as once it is put
# right by hand; .failed beside it for a moment when it still can't be.
read-again = Try again
    .hint = Read it again, once it is fixed
    .failed = Still can't be read
# Said once the settings file was read again and could be; the file under
# it.
settings-read = The settings file reads again, and its settings are in use
# A button that copies the whole path of the file a message is about;
# .done beside it for a moment once it has.
copy-path = Copy path
    .hint = Copy where the file is, to open it in an editor
    .done = Copied
# Said once files are removed from the list, which Undo puts back.
files-removed = { $count ->
    [one] Removed 1 file
   *[other] Removed { $count } files
}
undo-removing = Undo
    .hint = Put them back where they were, with their settings and results (Ctrl+Z)
# Said once Settings' Reset to defaults has put them back, with a way to
# undo it.
settings-reset = Settings are back to their defaults
undo-reset = Undo
    .hint = Put back the settings as they were before
# The hover of show-folder's button in a message.
folder-of = Show in folder
    .hint = Open the folder it is in
# The user's presets couldn't be read, their folder and why under it.
presets-unreadable = Your presets couldn't be read, so only the built-in ones are offered
# Files open when tgradish last closed, which aren't there any more.
files-gone = { $count ->
    [one] { $count } file is gone since last time
   *[other] { $count } files are gone since last time
}
# The list of them couldn't be read, as when a newer tgradish kept it. It
# is renamed rather than written over, to the file shown under this.
session-unreadable = The files of last time couldn't be read, so none are open; their list is set aside
# Some of the list couldn't: those are left out, and a copy of the list as
# it was is kept, as the file shown under this.
files-unreadable = { $count ->
    [one] { $count } file of last time couldn't be read and is left out; the list as it was is set aside
   *[other] { $count } files of last time couldn't be read and are left out; the list as it was is set aside
}
# The files open now couldn't be saved to be opened again next time; the
# file they are kept in and why under it.
session-unsaved = The open files couldn't be kept for next time
# Pasting found nothing on the clipboard to add; .why under it.
nothing-to-paste = Nothing to paste
    .why = The clipboard has no files, file paths or image
# Pasting failed: the clipboard couldn't be read, or .image, the image on
# it couldn't be saved as a file; what the system said under it.
paste-failed = The clipboard couldn't be read
    .image = The pasted image couldn't be saved
# The system's file dialog, or reading the file chosen in it, crashed.
pick-failed = The file dialog crashed, so nothing was chosen


## Dropping files

# In the middle of the list of files while files are dragged over the
# window.
drop-to-add = Drop to add files


## The system's file dialogs: their titles, which say what each is for,
## and the names of the kinds of files they show

# From Add files…
dialog-add-files = Add files to convert
# From A folder of frames…, in the menu beside Add files…
dialog-add-folder = Add a folder of frames
# From Images as one animation…, in the same menu
dialog-add-images = Add images as one animation
# From the microscope's button, and Inspect another…
dialog-inspect = Choose a sticker to inspect
# From Choose ffmpeg…, and Choose… beside Settings' ffmpeg program
dialog-ffmpeg = Choose ffmpeg
# From Choose… beside Settings' folder for results
dialog-results-folder = Choose where results go
# The kinds of files shown, when adding images as one animation
images-filter = Images
# The kinds of files shown, when choosing a sticker to inspect
stickers-filter = Stickers


## The list of files, on the left

settings = Settings
light-theme = Switch to the light theme
dark-theme = Switch to the dark theme
# What screen readers call the button that shows a gear, which opens a menu
# of Undo and Redo, Settings, inspecting a sticker, feedback and About.
main-menu = Settings and more
# Its items, each with its keys beside it. Undo takes back the last change
# to the settings of the file picked, or puts back the files just removed
# while a notice offers to; Redo makes a change undone again. Each is
# greyed out while there is nothing to undo or redo. .hint is said on hover.
menu-undo = Undo
    .hint = The last change to the file's settings, or the files just removed
menu-redo = Redo
    .hint = The last change undone
# Settings opens the Settings dialog.
menu-settings = Settings…
# .hint is said on hover.
inspect = Inspect a sticker…
    .hint = Check a file against Telegram's limits
# A chat with the author, in Telegram's app where it is installed, else in
# the browser.
feedback = Send feedback
    .hint = Write to the author on Telegram: in its app, where it is installed, or in the browser
about-and-licences = About and licences…
# In the menu of a file's row: shows its input in the file manager, which a
# pasted image has none of.
show-input-in-folder = Show in folder
    .pasted = A pasted image has no file of its own
# Opens the dialog of keyboard shortcuts; its keys are shown beside it.
shortcuts = Keyboard shortcuts
add-files = Add files…
# What screen readers call the arrow beside Add files…, which opens a menu
# of other ways to add files.
add-more = More ways to add files
# The menu beside Add files…
add-folder = A folder of frames…
    .hint = In name order, as one animation
add-images = Images as one animation…
    .hint = The images picked, in name order
add-pasted = From the clipboard
    .hint = Copied files, paths or an image
convert-all = { $count ->
    [0] Convert all
   *[other] Convert all ({ $count })
}
    .hint = Convert every file without an up-to-date result
remove-all = Remove all
# Over Convert all while files convert: the one at work, of all asked for.
converting-batch = Converting { $current } of { $total }
stop-all = Stop all
    .hint = Stop converting, and take the files waiting out of the queue
remove = Remove
remove-file = Remove it from the list
# A file's button that makes its result, before it has one.
convert = Convert
    .hint = Make its sticker or emoji
# The badge on a file's result that is outdated, which converts it again.
# "Outdated" is the one name for a result made with other settings than
# its file has now, wherever it is said.
row-convert-again = Outdated: convert again
# A file's state, under its name.
row-next = next
row-waiting = waiting, { $ahead } ahead
# After the size of a file's result, once its settings no longer match it.
row-stale = { $size } · outdated
row-failed = failed
row-exists = a file is in the way
row-stopped = stopped
# The large area that is the whole GUI while no files are open.
drop-here = Drop videos, GIFs, images or Aseprite files here
drop-here-note = To convert into a video, animated or static sticker or emoji
paste = Paste
# A file's name in the list when it is more than its first input.
name-pasted = pasted image
name-frames = { $name } (frames)
name-and-more = { $name } and { $more } more
# When a file's name has characters no font, tgradish's or the system's,
# can draw: each is shown by its code, as U+2FE0.
no-font = { $count ->
    [one] No font was found for { $codes }, neither among tgradish's own nor the system's, so it is shown by its code. The file itself is fine
   *[other] No font was found for { $codes }, neither among tgradish's own nor the system's, so they are shown by their codes. The file itself is fine
}


## The bar under the settings of the file picked

stop = Stop
stopping = Stopping…
# Beside Stop while a WebM is fitted to the size limit in several encodes:
# stops there and saves the best encode that fits so far.
keep-best = Keep best so far
    .hint = Stop trying more encodes, and save the best that fits so far as the result
    .nothing-fits = Nothing fits yet: there is no encode to keep
keeping-best = Keeping best…
# The button on a file waiting to be converted, which takes it out of the queue.
cancel-waiting = Cancel
next-to-convert = Next to convert
waiting-for = Waiting for { $ahead } more
# The bar's Convert button once there is a result. The hovers of the button
# either way: made with the settings as they are now, or again with the same.
convert-again = Convert again
    .changed = Outdated: make it again with the current settings
    .hint = Make it again with the same settings
-save-settings = Save current settings as preset
save-preset = { -save-settings }…
    .hint = Keep them as a preset of your own, for any file to start from
failed = Failed: { $error }
    .short = Failed — see details
in-the-way = A file not from tgradish is in the way:
replace-it = Replace it
stopped = Stopped
show-in-folder = Show in folder
# Before the name of the file a file's result goes to, whose path is
# said on hover.
saves-as = Saves as
    .hint = Where results go is set in "{ settings }"
# Before the name of a result made as the settings are now.
result-saved-as = Saved as
# Beside Convert, once there is a result: copies it as a file, to paste
# into Telegram's chat.
copy-result = Copy
    .hint = Copy the sticker as a file, to paste into a chat in Telegram
    .done = Copied
    .failed = Not copied
# Beside the name of the result to be made, where a file is there that
# converting will replace: one tgradish made at another time, or with the
# setting to replace any, another file.
replaces-earlier = Replaces an earlier result
    .any = Replaces the file there


## Reading a file to show it

reading-crashed = tgradish crashed while reading this
reading-stopped = Reading it stopped
no-frames = ffmpeg found no frames in it
no-frames-there = ffmpeg found no frames there
no-frame-there = ffmpeg found no frame there
art-no-frames = It has no frames
result-no-frames = The result has no frames


## The result's small picture over the preview

# Its label: how large it is shown, as inset-scale says it, then, on a line
# under it, its size, as "512 × 512 px".
inset-label = Result at { $scale }
    .plain = Result
    .drag = Drag it to another corner
# How large it is shown: as a share of its own pixels, or, where it is
# shown pixel for pixel larger, as so many times its size. On Zoom out too,
# how large the pixels of what the preview shows are.
inset-scale = { $percent }%
    .times = { $times }×
# .same, on hover, where the next size would show it no otherwise, as it
# shows at its own size in both, or there is none.
inset-smaller = Smaller
    .same = It would show no smaller
inset-larger = Larger
    .same = It would show no larger


## Over the preview

show-input = Input
# .outdated once the result no longer matches the settings, which .hint
# explains on hover.
show-result = Result
    .outdated = Result ⚠
    .hint = Outdated: the result was made with earlier settings
# Why the result can't be shown yet.
convert-first = Convert it first
# Why a .tgs result can't be shown: its file has shapes, such as curves
# or gradients, that tgradish's own stickers don't, as when another
# program replaced it.
tgs-unseen = This TGS sticker has shapes tgradish can't draw
# Turns on the result's small picture.
result-preview = Result preview
    .hint = Show / hide small preview beside the input
    .input-only = Shown beside the input, not the result
zoom-out = Zoom out
    .hint = Show all of it
    .all = Already fully zoomed out
# How the mouse works on the preview.
input-hint = Drag to crop · wheel to zoom · right button to pan
crop-hint = Drag the crop or its edges · double-click for all
result-hint = Wheel to zoom · right button to pan
unreadable = Couldn't read it: { $error }
reading = Reading…
# What screen readers call the line under the preview, dragged to share
# the height between the preview and the settings under it.
preview-divider = Line under the preview


## The timeline under the preview

# What screen readers call the play button, which shows ▶; .pause while it
# plays, when it shows ⏸.
play = Play
    .pause = Pause
# The play button's hover: the keys that work over the preview. A line
# can't start with [, so it is written {"["} there.
play-keys =
    Space — play or pause
    ← / → — one frame back or forward
    Shift+← / → — ten frames back or forward
    Home / End — the first or last frame used
    {"["} / ] — start or end the used part at the frame shown
    Ctrl+← / → — move the used part's start
    Alt+← / → — move the used part's end
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
# Where the pointer is over the timeline, or the end of the used part being
# dragged, counting frames from 1.
frame-number = frame { $number }
# The mark of Telegram's 3 seconds on the timeline, on hover: what becomes
# of the used part past it, as "Spoof duration" and the speed choose. The
# part cut off is hatched; one spoofed or sped up is kept whole.
limit-mark = Telegram's limit of 3 seconds: the used part past it is cut off
    .spoofed = Telegram's limit of 3 seconds: the used part past it plays too, as Telegram is told the sticker is shorter
    .sped-up = Telegram's limit of 3 seconds: a longer used part plays faster, to fit in it
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
format-animation = Animated
    .hint = An animated sticker or emoji: TGS file. This option is meant for pixel art: TGS is a vector format, so it stays crisp at any size. Best for custom emoji, which are only 100 × 100 px. Not for ordinary video or photos: use "{ format-video }" for those
format-image = Static
    .hint = A static sticker or emoji: WebP, from any picture or one frame of a video
target-sticker = Sticker
target-emoji = Custom emoji
# What Telegram asks of what is being made.
needs-webm-sticker = WebM, 512 px on the longer side, up to 256 KiB
needs-webm-emoji = WebM, 100 × 100 px, up to 64 KiB
needs-tgs = TGS, square, up to 64 KiB
needs-image-sticker = WebP, 512 px on the longer side, up to 512 KiB
needs-image-emoji = WebP, 100 × 100 px, up to 512 KiB
not-pixel-art = "{ format-animation }" is meant for pixel art, converted to vector so it stays crisp. This doesn't look like pixel art: "{ format-video }" suits it better
preset = Preset
    .hint = How hard to work on the result; your own values below go on top
    .image = A static sticker takes only the preset's picture settings, how it is squared and scaled: it is saved lossless where that fits, otherwise at the best quality which still fits
    .broken = This preset's file is broken
    .missing = There is no preset "{ $name }" any more: pick another
manage-presets = Manage…
    .hint = Rename, describe or delete your presets
# The built-in presets, from the quickest; each says what it does for
# videos (.webm) and for animations (.tgs).
preset-fast = Fast
    .webm = Fits only the bitrate, with the fast encoder: takes seconds, but blurrier
    .tgs = Takes under a second, a few percent larger
preset-balanced = Balanced
    .webm = Fits the bitrate, and tries lower frame rates where they could look better
    .tgs = Between fast and best
preset-best = Best
    .webm = Like balanced, with the slowest encoder: takes minutes for long videos
    .tgs = Takes a few seconds, the smallest file
# One of the user's own presets, with what they said it is.
preset-described = { $name }: { $description }


## Settings: the picture

picture = Picture
# For a static sticker, a section of its own beside Frame, of how the
# picture becomes the result's size: Force into square, Scaling and
# Integer scale, which are under Picture for the other formats.
picture-size = Size
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
# Under the crop's fields, and what screen readers call the cross on the
# crop's size over the preview, which does the same.
# What screen readers call the crop over the preview, which the keyboard's
# focus goes to, and what its keys do.
crop-keys = Crop: the arrows move it, Ctrl+arrows resize it; with Shift, by ten pixels
whole-picture = Whole picture
    .hint = Use all of the picture
    .exact = Use all of the picture, or as much as the fixed scale takes from its top left
middle = Middle
    .hint = Use as much of the middle as the fixed scale takes
aspect-ratio = Aspect ratio
    .hint = The width to height ratio the crop keeps while you drag it
    .square = TGS must be square, and "{ square }" is "{ square-off }", which keeps the crop's shape: picking this forces it into square again
shape-free = Free
    .hint = Any shape
shape-preserve = Preserve
    .hint = The input's own shape
shape-custom = Custom
    .hint = A shape you type: as a ratio, or as the result's size
    .pick = Pick "{ shape-custom }" to type a shape
# Beside the ratio: the size of the result, which can be typed instead;
# and what screen readers call its two fields.
ratio-result = Result
    .fixed = The result is squared or has margins, so its size doesn't follow the crop
    .width = Result width
    .height = Result height
square = Force into square
    .hint = How the result becomes square
    .tgs = How the pixel art becomes square (as all TGS must be)
square-off = Off
    .hint = The result keeps the crop's shape
    .emoji = Emoji must be square
    .tgs = TGS must be square, and so must the crop it keeps: picking this sets "{ aspect-ratio }" to 1:1
    .kept = The pixel art keeps the crop's shape, which "{ aspect-ratio }" keeps square (as all TGS must be)
square-pad = By padding
    .hint = Transparent margins fill the rest of the square
    .tgs = Transparent margins fill the rest of the square
square-crop = By cropping
    .hint = The middle fills the square and the rest is cut off: the preview dims it
    .tgs = The middle fills the square and the rest is cut off
square-stretch = By stretching
    .hint = The picture is stretched to the square
    .tgs = The pixel art is stretched to the square (its pixels stop being square)
# Why cropping or stretching into a square can't be picked.
square-exact = Fixed integer scale keeps all of the picture: pad it instead
scaling = Scaling
    .hint = How the input's pixels become the result's
scaling-auto = Automatic
    .hint = Sharp pixels for pixel art made twice as large or more, smooth otherwise
scaling-smooth = Smooth
    .hint = Neighbouring pixels blend
scaling-sharp = Sharp pixels
    .hint = Each pixel becomes a block, and blocks blend only at their edges
scaling-integer = Integer
    .hint = Each pixel becomes the same whole number of result pixels, or whole blocks of them become one; transparent margins fill the rest
integer-scale = Integer scale
    .hint = How many result pixels each input pixel becomes, or how many input pixels make one
    .only = Only with "{ scaling-integer }" scaling
scale-largest = Largest that fits
    .hint = The largest whole multiple that keeps the crop within the result
    .preset = The preset fixes a scale
scale-fixed = Fixed
    .hint = This scale; the crop can be at most what it fits into the result
smaller = Smaller
larger = Larger
scale-field = Type 3 for three times as large, or 1/3 for a third
    .pick = Pick "{ scale-fixed }" to choose a scale
scale-unused = Integer scales apply with "{ scaling-integer }" scaling only
scale-unsuited = The crop doesn't suit this scale yet
# What an integer scale, as ×3 or ×1/2, makes of the crop; then, if the result
# has transparent margins, the size of the whole result.
scale-makes = { $scale } makes the crop { $width } × { $height } px
scale-makes-padded = { $scale } makes the crop { $width } × { $height } px; transparent margins fill the rest of the { $box_width } × { $box_height }
# The scale of the messages above where stretching makes it differ each way.
scale-each-way = ×{ $across } across and ×{ $down } down
scale-shrinks = The crop is larger than the result, so it shrinks with sharp pixels: a fixed scale like ×1/2 shrinks it by whole blocks
margins = Margins
    .hint = Edge rows and columns that are transparent in all input frames
auto-crop = Crop empty margins automatically
    .hint = Cuts the pixel art to the smallest rectangle with visible pixels in any frame
    .after = Applies after the crop above
art-pixel = Art pixel size
    .hint = How many input pixels one art pixel takes. Pixels off its grid are moved onto it
art-pixel-detected = Detected
    .hint = Found from the pixel art: its smallest blocks of one colour


## Settings: time

time = Time
used-part = Used part
    .hint = Where the part of the input used starts and ends; the timeline under the preview sets it too
# Between the fields of the used part's start and end.
part-from = From
part-to = to
part-first = Its first frame
part-last = Its last frame
all-of-input = All of the input
# Under the used part, or the frame a picture is made of, in the count the
# fields don't use.
part-seconds = { $from } to { $to } of { $length }
part-frame = Frame { $number } of { $count }
part-frames = Frames { $first } to { $last } of { $count }
speed = Speed
    .hint = How many times as fast as the input the result plays
# The choices beside a number tgradish can pick itself, as the speed: what
# it picks, and how. Where the preset sets the number, in any group of
# settings, the choice is the preset's.
from-preset = As preset
    .hint = The value the preset sets
from-input = As input
    .speed = As fast as the input plays
    .rate = The input's rate at the speed, at most 60 fps
fits-3-s = Fits 3 s
    .hint = As fast as fits the used part into 3 seconds
fit-3-s = Fit 3 s
    .hint = Set the speed so the used part plays in 3 seconds
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
    .hint = { $fps } fps, as these frames don't say how fast they play
over-3-s = Spoof duration
    .hint = Telegram refuses stickers that say they last longer than 3 seconds
spoof-auto = When longer
    .hint = Stickers longer than 3 seconds get a short duration encoded into them: Telegram accepts them and plays all of it
spoof-always = Always
    .hint = Even short stickers get the short duration encoded into them
spoof-never = Never: cut at 3 s
    .hint = Doesn't spoof the metadata and cuts the video at 3 seconds
spoof-to = Spoofed duration
    .hint = The duration encoded into the metadata, which Telegram reads
    .honest = Nothing is spoofed when "{ spoof-never }" is picked
default-duration = Default
    .hint = A short duration Telegram accepts
frame = Frame
frame-at = Frame at
    .hint = The time of the frame the picture is made of; the timeline under the preview picks it too


## Settings: size and quality of a WebM

size-and-quality = Size and quality
# How the result is made to fit the size limit; .lossless is the hover of
# the ways a quality can't go with, while lossless encoding is ticked.
fitting = Getting under the limit
    .hint = What tgradish changes to land just under the file size limit
    .lossless = Lossless encoding keeps no bitrate or quality to fit: picking this unticks "{ lossless-encoding }"
fit-auto = Best looking
    .hint = Fits the bitrate, and where nothing fits or fewer frames, each with more bits, could look better, tries lower frame rates and keeps what looks most like the input
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
encoder = Encoder effort
    .hint = Faster encoding looks worse at the same size
encoder-fast = Fast
    .hint = Takes seconds, but visibly worse at the same size
encoder-balanced = Balanced
encoder-best = Best
    .hint = Slow: takes minutes for long videos
# Why a setting of fitting can't be changed.
fitting-off = "{ fit-off }" encodes once
attempts = Encodes up to
    .hint = How many times fitting may encode, at most
attempts-default = Default
    .hint = Enough to land within a few percent of the file size limit for most inputs
# After the number of encodes, which its field shows alone.
attempts-times = { $count ->
    [one] time
   *[other] times
}
# And what screen readers call the fields of its two ends.
search-within = Search within
    .hint = The range fitting searches, in the fitted value's unit
    .low = Lowest
    .high = Highest
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
    .switches = Only small for tiny or still pictures. Lossless encoding keeps no bitrate or quality to fit: ticking this switches fitting to "{ fit-fps }"
bitrate = Bitrate
    .hint = Used unless only a quality is set
    .fitted = Fitting finds the bitrate
    .constant = Encoded at a constant quality, not a bitrate
bitrate-auto = From the file size limit
    .hint = As much as the length lets fit


## Settings: size and quality of a TGS animation

tgs-encoder = Encoder effort
    .hint = How hard to look for a small encoding
tgs-fast = Fast
    .hint = Takes under a second, a few percent larger
tgs-balanced = Balanced
tgs-best = Best
    .hint = Takes a few seconds, the smallest file
too-large = If it doesn't fit
    .hint = What happens when the pixel art doesn't fit into 64 KiB as it is
never-change = Never change the pixel art; report it instead
    .set = The pixel art is never changed, as set above
give-up = Give up first
    .hint = When the pixel art has to change to fit: its motion or its detail
give-up-either = Either
    .hint = Whatever changes the pixel art least for the bytes it saves
give-up-motion = Motion
    .hint = Frames merge and drop first: full detail, choppier motion
give-up-detail = Detail
    .hint = The picture coarsens first: smooth motion, less detail
ways-to-fit = Ways to fit it
    .hint = What fitting may do to the pixel art to fit it into 64 KiB, least visible first
reduce-snap = Snap to the pixel grid
    .hint = Moves stray pixels onto the art pixel grid
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
reading-art = Reading the pixel art
sprite-sheet = Sprite sheet
    .hint = Frames laid out in a grid of equal cells, row by row
read-as-sheet = Read as a sprite sheet
# Why a file can't be read as a sprite sheet, which is one picture; and,
# .converted, a warning of a conversion that read it as it is, as a
# session kept it ticked.
sheet-refused = This file already has { $frames } frames: a sprite sheet is one picture
    .converted = Read as it is, not as a sprite sheet: it already has { $frames } frames
# The sprite sheet's columns × rows; and what screen readers call each.
sheet-cells = Cells
    .columns = Columns
    .rows = Rows
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
# A tag's frames, counted from 1 as Aseprite counts them, and how it plays.
tag-frame = Frame { $number }
tag-frames = Frames { $from } to { $to }
tag-reverse = { $frames }, in reverse
tag-ping-pong = { $frames }, there and back
tag-pong-ping = { $frames }, back and there
layers = Layers
    .hint = Which layers show, the top one first; a hidden group hides the layers in it
    .group-hidden = Its group is hidden
    .file = Go back to the layers the file shows


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
    .unfinished = A quote or a backslash is left open: the arguments stay as they last read until it is closed
    .system = Only with the system's ffmpeg, which "{ settings }" can pick


## What keeps a file from being converted as it is, and the ways out

# First over what converting will do, and on the hover and the note of the
# Convert button it greys out, before each reason as one of those below.
blocked = Can't convert yet: { $reason }
    .heading = Can't convert yet
blocked-crop = the crop is outside the picture
blocked-sheet = the sprite sheet's cells don't divide the picture
blocked-tag = the file has no tag "{ $tag }"
# The layers, each in quotes, apart by commas.
blocked-layers = { $count ->
    [one] the file has no layer { $layers }
   *[other] the file has no layers { $layers }
}
blocked-ffmpeg = ffmpeg wasn't found
# Under the crop's fields, where the crop kept, as of another file of the
# same name, doesn't fit the picture read now.
crop-outside = The crop, { $width } × { $height } from { $left }, { $top }, is outside this { $across } × { $down } picture
use-whole = Use whole picture
    .hint = Crop nothing, keeping all of the picture
# Under the sprite sheet's cells, where they don't divide its picture.
sheet-uneven = The picture, { $width } × { $height }, doesn't split into { $columns } × { $rows } equal cells
sheet-nearest = Nearest that divides: { $columns } × { $rows }
    .hint = The layout nearest this one whose cells divide the picture
# Under the tags and layers, where the file has no longer what was chosen.
tag-missing = This file has no tag "{ $tag }"
layers-missing = { $count ->
    [one] This file has no layer { $layers }
   *[other] This file has no layers { $layers }
}
layers-as-file = Show layers as in the file
    .hint = Show and hide the layers as the file does
# Under what to make, where it needs ffmpeg, which isn't there; .hint on
# hover over it.
needs-ffmpeg = This needs ffmpeg, which wasn't found
    .hint = Install ffmpeg from your system's packages, or from ffmpeg.org, or choose where it is. Files are read again once it is found
choose-ffmpeg = Choose ffmpeg…
    .hint = Pick the ffmpeg program, or a folder with ffmpeg and ffprobe
# A file's state in the list, in place of its Convert button.
row-attention = needs attention
    .hint = Something keeps it from being converted: see its settings
row-needs-ffmpeg = needs ffmpeg
    .hint = ffmpeg reads it or makes its result, and wasn't found
# Beside Convert all, of the files it leaves out.
need-attention = { $count ->
    [one] 1 needs attention
   *[other] { $count } need attention
}


## What converting will do, beside the preview, step by step

plan-video = The video will be:
plan-picture = The picture will be:
plan-animation = The animation will be:
plan-art = The pixel art will be:
# For a static sticker or emoji, which is made of one frame of what plays.
plan-video-frame = A frame of the video will be:
plan-animation-frame = A frame of the animation will be:
plan-art-frame = A frame of the pixel art will be:
plan-tag = Played as its "{ $tag }" tag
plan-tag-reverse = Played as its "{ $tag }" tag, in reverse
plan-tag-ping-pong = Played as its "{ $tag }" tag, there and back
plan-tag-pong-ping = Played as its "{ $tag }" tag, back and there
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
plan-faster = Sped up ×{ $factor }
plan-slower = Slowed down ×{ $factor }
plan-cut-3-s = Cut at 3 s of { $played }
plan-spoofed = Duration spoofed, so all { $played } plays
# Spoofing "Always", where the result is short enough not to need it.
plan-spoofed-anyway = Duration spoofed, though { $played } is short enough
plan-saved = Saved as { $name }


## Settings: the same conversion as commands, folded away under the rest

commands = Commands
copy = Copy
commands-posix = The same conversion on the command line, for a POSIX shell:
commands-powershell = The same conversion on the command line, for PowerShell:
pasted-temporary = A pasted image is kept in a temporary folder, gone when tgradish closes
ffmpeg-commands = The ffmpeg commands of the first encode; fitting changes the rate, and maybe the frame rate, between encodes:
    .builtin = What the built-in ffmpeg does for the first encode, as commands for the system's ffmpeg; fitting changes the rate, and maybe the frame rate, between encodes:
no-ffmpeg-commands = No ffmpeg commands: { $why }
    .unread = the input isn't read yet
    .art = ffmpeg reads a PNG each conversion draws of the pixel art


## The Settings dialog

# Saves a preset.
save = Save
# Under the settings once a change has been written; .kept when the file
# there couldn't be read, so it was copied aside first.
saved-to = Saved to { $path }
    .kept = Saved to { $path }; the file that couldn't be read is kept as { $kept }
not-saved = Couldn't save: { $error }
no-home = There is no home folder to keep settings in
settings-note = Changes apply and are kept as you make them
# At the left of the dialog's buttons: every setting in the dialog back to
# its default, which its message offers to undo.
reset-settings = Reset to defaults
    .hint = Every setting here back to how tgradish first had it
# Under the ffmpeg heading, when tgradish was started with --ffmpeg or
# --ffmpeg-from, or with the TGRADISH_FFMPEG environment variable set,
# which this run uses over the settings below.
ffmpeg-given = This run uses the ffmpeg given as tgradish was started; these settings apply when it is started without one
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
    .missing = There is nothing there, so the program in use stays as it was
# What the empty field of the ffmpeg program shows.
ffmpeg-found = { $path }, found on PATH
for-example = For example: { $example }
choose = Choose…
check = Check
    .hint = Check whether this ffmpeg can make stickers
# Beside Check, while the ffmpeg chosen is run to see what it can do.
checking-ffmpeg = Checking…
# What checking ffmpeg found, its version as "8.0".
ffmpeg-ready = ffmpeg { $version } with VP9: ready
ffmpeg-no-vp9 = ffmpeg { $version } can't encode VP9 with libvpx
# The heading of how converting runs on this computer, after ffmpeg's.
converting-settings = Converting
# How many threads converting uses at once, at most: beside it, threads-auto
# or Custom (custom) with a number, which shows how many this computer runs
# at once while it is automatic.
threads = Threads
    .hint = How many converting uses at once, at most; fewer leave more of the computer to the rest, and take longer
# The first choice of the threads setting.
threads-auto = Automatic
    .hint = As many as each part of converting makes use of
results = Results
save-results = Save them
next-to-input = Next to each input
in-a-folder = In a folder
    .unchosen = Until a folder is chosen, results are saved next to each input
    .relative = Write the whole path, from the top, so the folder in use can change
    .file = That is a file, not a folder, so the folder in use stays as it was
existing-files = Existing files
    .hint = Results tgradish made are always replaced when made again
replace-in-the-way = Replace any file that is in the way
interface = Interface
# The choice of language, a globe beside it; .variable under it where the
# TGRADISH_LANG environment variable chooses instead, which it then shows.
language = Language
    .hint = The language shown here; the command line stays in English
    .variable = Chosen by the { $variable } environment variable
as-the-system = As the system
# The first choice of language, then which the system's is, by its own name.
as-the-system-language = As the system: { $language }
theme = Theme
    .hint = The button beside ⚙ switches between light and dark too
theme-light = Light
theme-dark = Dark
accent = Accent colour
    .hint = What marks the chosen, the hovered and the focused
# How large everything is drawn; .keys under it says the keys that change
# it too, written as keys are elsewhere, as Ctrl++.
scale = Scale
    .hint = How large text and controls are
    .keys = { $larger } and { $smaller } change it too; { $reset } is back to 100%
scale-smaller = Smaller
scale-larger = Larger
scale-reset = Back to 100%
# The scale, as a percentage.
scale-percent = { $percent }%
# The first of the accent colours, "As the system" again.
accent-system = { as-the-system }
    .hint = The desktop's accent colour, or Breeze's blue where it has none
# The accent colours on offer, each said on hover, as Plasma offers them.
accent-pink = Pink
accent-red = Red
accent-orange = Orange
accent-yellow = Yellow
accent-green = Green
accent-teal = Teal
accent-blue = Blue
accent-purple = Purple
accent-violet = Violet
accent-grey = Grey
# The last of the accent colours, which opens a colour picker; once one is
# picked, it is said as #rrggbb.
accent-own = A colour of your own
    .chosen = A colour of your own, { $colour }
# What screen readers call the two parts of the colour picker: a square of
# the shades of one colour, and a strip of the colours of the rainbow. The
# square's value is how saturated and how bright its shade is, from 0 to
# 100; the arrow keys change them.
accent-shade = Shade
    .value = { $saturation }% saturated, { $value }% bright
accent-hue = Hue
# Beside a field of the colour picked, as #rrggbb.
accent-hex = Hex
    .wrong = Write it as # and six hexadecimal digits, as #3daee9
files = Files
reopen-files = Reopen the files of last time, with their settings
# Whether scrolling eases and what changes moves into place, a checkbox
# beside it; the system asking for less motion keeps everything still
# whatever it says.
motion = Motion
    .hint = Where the system asks for less motion, nothing moves either way
motion-on = Ease scrolling, and fade and move what changes
new-files = New files
preset-for-webm = Preset for WebM
preset-for-tgs = Preset for TGS


## The result, beside the preview

last-conversion = Last conversion
path-in-the-way = { $path } is in the way
stopped-early = Stopped before it was done
result = Result
    .stale = Result, outdated
# Under Telegram's verdict on a result it would accept, as long as the
# bar's Copy (copy-result) copies it; $keys is Copy's shortcut.
result-copy = Copy it ({ $keys }) to paste it into a chat in Telegram
# A WebM result: its size in pixels, frame rate and length.
result-webm = { $width } × { $height } px, { $fps } fps, { $length }
    .spoofed = { $width } × { $height } px, { $fps } fps, { $length }, duration spoofed
changed-to-fit = Changed to fit: how is listed below
replaced-earlier = Replaced an earlier result made by tgradish
converting = Converting
# The stages of making a TGS animation, each a bar. Fitting shows only
# once the animation is found too large: from its size then, and after
# each change made to fit, with how many there are and the size after.
stage-reading = Reading the pixel art
stage-drawing = Converting to vector
stage-fitting-steps = { $count ->
    [one] Fitting into 64 KiB: { $count } step, about { $size }
   *[other] Fitting into 64 KiB: { $count } steps, about { $size }
}
stage-fitting-from = Fitting into 64 KiB from { $size }
stage-compressing = Compressing
    .hopeless = Compressing quickly: it can't fit
    .missed = Compressed → { $size }, too large
    .missed-far = Compressing: far too large still
# Another try at fitting a TGS, once the one before came out too large: the
# estimated size it aims at, then what it came to.
retry = Try { $number }: aiming at { $aim }
retry-done = Try { $number }: aiming at { $aim } → { $size }, too large
    .far = Try { $number }: aiming at { $aim } → far too large still
# Each encode of a WebM, a bar: its frame rate and rate, then its size as
# a share of the limit, and whether that fits.
starting = Starting
encode = Encode { $number }: { $fps } fps, { $rate }
# The bar beside Stop: the encode at work, and how many there may be at most.
encode-of = Encode { $number } of up to { $most }: { $fps } fps, { $rate }
encode-done = Encode { $number }: { $fps } fps, { $rate } → { $size }, fits
    .too-large = Encode { $number }: { $fps } fps, { $rate } → { $size }, too large
rate-quality = quality { $crf }
rate-lossless = lossless
encodes-up-to = { $count ->
    [one] Up to { $count } encode, until one lands just under the file size limit
   *[other] Up to { $count } encodes, until one lands just under the file size limit
}
# How a TGS animation was made: its art's size, then how many colours and
# frames it has.
art-size = { $width } × { $height } art pixels
    .scaled = { $width } × { $height } art pixels, each { $scale } × { $scale } input pixels
art-summary = { $size }; { $colours }, { $frames }
art-tempo = Plays at ×{ $tempo } the input's speed
art-off-grid = Most of it looks like { $scale }× pixel art with some pixels off the grid; an art pixel size of { $scale } snaps them
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
    .hint = Share of the file size limit, green where it fits
column-alike = alike
    .hint = How like the input it looks (SSIM)
# A quality, short, in the table's rate column.
rate-quality-short = q { $crf }
# Under the table: why fitting stopped where it did, or tried a lower
# frame rate. Shares are of the file size limit, as the table's
# sizes; "looks … like the input" is the table's alike.
decided-in-band = Stopped: encode { $number } uses { $share } of the limit, at least the { $least } aimed for
decided-alike-enough = Stopped: encode { $number } already looks { $alike } like the input, so more bits would only make it larger
decided-alike-unchanged = Stopped: encode { $number } looks { $alike } like the input, hardly more than encode { $than } with fewer bits
decided-little-to-gain = Stopped: encode { $number } uses { $share } of the limit, and another would use only about { $predicted }
decided-too-large-at-least = Stopped: even the least of the range, in encode { $number }, uses { $share } of the limit
decided-nothing-left = Stopped: no setting is left worth trying
decided-out-of-attempts = { $count ->
    [one] Stopped: the { $count } encode allowed is made
   *[other] Stopped: all { $count } encodes allowed are made
}
decided-lowers-to-fit = Trying { $fps } fps: nothing fit at { $from } fps
decided-lowers-for-looks = Trying { $fps } fps: encode { $number } at { $from } fps looks only { $alike } like the input
decided-settled = Stopped early: kept encode { $number }
decided-lower-no-better =Stopped: { $fps } fps looks { $alike } like the input, no more than encode { $kept } at { $kept_fps } fps with { $kept_alike }
# Telegram's problems with a WebM or static result.
issue-not-webm = Its container is { $container }, not WebM
issue-no-video = It has no video track
issue-not-vp9 = Its codec is { $codec }, not VP9
issue-audio = It has an audio track
issue-size = Its size is { $width } × { $height } px, where Telegram takes { $side } × { $side } px
    .sticker = Its size is { $width } × { $height } px, where Telegram takes one side of { $side } px and the other at most that
issue-too-big = It is { $size }, over the limit of { $limit }
issue-too-long = Its metadata says it lasts { $seconds }, over the limit of { $limit }
issue-too-many-fps = Its frame rate is { $fps } fps, over the limit of { $limit } fps
issue-animated = It is animated, where static stickers are one picture


## Inspecting a sticker

inspect-another = Inspect another…
inspect-webm-sticker = A video sticker
inspect-webm-emoji = A video emoji
inspect-image-sticker = A static sticker
inspect-image-emoji = A static emoji
inspect-tgs = A vector animation, { $width } × { $height } at { $fps } fps
# A kind of sticker this version of tgradish can't show yet.
inspect-unknown = A kind of sticker this version can't show
inspect-size = Size
    .hint = Telegram refuses larger files
    .tgs = Telegram refuses larger stickers
inspect-duration = Duration
    .hint = Telegram reads the duration in the metadata; spoofing makes it short
    .spoofed = { $seconds } s in the metadata, spoofed
    .no-header = { $seconds }, none in the metadata
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
ffmpeg-not-on-path = ffmpeg and ffprobe aren't on PATH: install them, or pick another ffmpeg in "{ settings }"
# Raw ffmpeg arguments were set, with the built-in ffmpeg chosen.
ffmpeg-args-need-program = ffmpeg arguments need the system's ffmpeg, which "{ settings }" can pick; encoder options work with either
# ffmpeg or ffprobe ended with an error; its own output follows, as it is.
program-failed = { $program } failed ({ $status }):
unreadable-file = Couldn't read { $name }: { $details }
no-video = { $name } has no video in it
# A file that isn't there, as one moved or deleted since it was added.
no-such-file = { $name } isn't there any more
# A file inspected, or read as a video sticker, that is of another kind.
not-webm = { $name } isn't a WebM file
not-sticker = { $name } isn't a sticker: a WebM, WebP or .tgs file
no-frame-at = { $name } has no frame at { $seconds }
nothing-fits = Nothing fits the { $limit } limit: the smallest encode was { $smallest }. A shorter used part, a lower frame rate or a wider range to search within can help
clipboard-failed = Couldn't paste from the clipboard: { $details }
# The result would be written over one of the files converted.
output-is-input = The result would be written over { $name }, which it is made of
nothing-to-convert = There are no images to convert


## Saving and managing presets

save-preset-title = { -save-settings }
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
# What a preset's name can't be, and why one can't be renamed or deleted.
preset-name-length = A preset's name has 1 to { $most } characters, not starting with a space
preset-name-characters = A preset's name has only letters, digits, spaces, - and _
preset-name-builtin = "{ $name }" is a built-in preset's name
preset-name-taken = Another preset is named "{ $name }"
preset-extended = "{ $other }" extends it: change that preset first
your-presets = Your presets
your-presets-note = Changes are kept as you make them. "{ preset-fast }", "{ preset-balanced }" and "{ preset-best }" are built in and stay as they are
no-presets = None yet: "{ -save-settings }", in the bar under the settings
preset-broken = Its file can't be read
delete-it = Delete it
deleted = Deleted "{ $name }"
undo = Undo
    .hint = Put it back
# A message: the preset deleted couldn't be put back, why under it.
not-put-back = The preset "{ $name }" couldn't be put back


## About and licences

# The dialog's title.
about-title = About and licences
# Under the name and version; a link to the source code follows.
about-summary = Telegram stickers and emoji from videos, images or pixel art. MIT licence;
source-code = source code
# Under the summary, a link to the author on Telegram.
send-feedback = Send feedback on Telegram
# Over the licences' notices, which are shown as they are written, in
# English.
about-notices = tgradish is built from other people's work too. Their licences ask for these notices to go with every copy, so the program carries them:
# The notices' titles, each over its notice. { $name } is the name of what
# it is the licence of, as libvpx, and { $licence } a licence's name, as
# GNU Lesser General Public License 2.1: both are shown as they are.
notice-licence = { $name }: licence
notice-named-licence = { $name }: { $licence }
notice-licence-notes = { $name }: licence notes
notice-patent-grant = { $name }: patent grant
notice-sources = { $name }: what is built in, and its sources
# The licence of a font of which tgradish has a subset built in, or more
# than one: of Ubuntu Light and of Ubuntu Medium.
notice-font = { $subsets ->
    [one] Font: { $name }, a subset
   *[other] Font: { $name }, subsets
}
notice-crates = Rust crates
# Within the notice of the Rust crates, the title of each licence's part: the
# licence's name, as MIT License, then the crates it covers, as
# "anyhow, arboard"; where there are more than three, three of them and how
# many more.
notice-crates-licence = { $licence }: { $crates }
    .more = { $more ->
        [one] { $licence }: { $crates } and { $more } more
       *[other] { $licence }: { $crates } and { $more } more
    }
# In the notice of ffmpeg's sources, a link to them.
ffmpeg-sources = The exact sources of the ffmpeg built in, with this release


## The dialog of keyboard shortcuts, opened from the menu or by ? or F1

# The names of keys, as keyboards print them: shortcuts are written as
# Ctrl+Enter; Left and Right are the arrow keys. Others are written as
# their sign.
key-ctrl = Ctrl
key-cmd = Cmd
key-alt = Alt
key-shift = Shift
key-enter = Enter
key-space = Space
key-delete = Delete
key-backspace = Backspace
key-home = Home
key-end = End
key-escape = Esc
key-left = Left
key-right = Right
key-up = Up
key-down = Down
# Its groups.
shortcuts-files = Files
shortcuts-playing = The preview and its timeline
shortcuts-general = General
shortcuts-mouse = The mouse, on the preview and the timeline
# What each does. Where there are two keys, the first goes back and the
# second on, or the first to the start and the second to the end.
shortcut-add-files = Add files
shortcut-paste = Add the files or the image on the clipboard
shortcut-select = Pick the file above or below
shortcut-convert = Convert the file picked
shortcut-convert-all = Convert all: every file without an up-to-date result
shortcut-keep-best = Keep the best encode so far, while a WebM is fitted
shortcut-copy-result = Copy the result of the file picked, to paste into Telegram
shortcut-remove = Remove the file picked, or the one whose row has the focus
shortcut-undo = Undo a change to the file picked, or put back files removed
shortcut-redo = Redo a change undone
shortcut-play = Play or pause
shortcut-step = A frame back or on; with Shift, ten
shortcut-ends = The first or the last frame used
shortcut-cut-here = Start or end the part used at the frame shown
shortcut-move-start = Move the start of the part used; with Shift, by ten frames
shortcut-move-end = Move the end of the part used; with Shift, by ten frames
shortcut-settings = Open Settings
shortcut-inspect = Inspect a sticker
shortcut-scale-up = Make everything larger
shortcut-scale-down = Make everything smaller
shortcut-scale-reset = Back to 100%
shortcut-shortcuts = This list
shortcut-close = Close a dialog or a menu
# A button's hover where a shortcut does what it does too: what it says
# on hover otherwise, and the shortcut's keys.
with-keys = { $hint } ({ $keys })
# Ways of using the mouse, and what each does.
mouse-drag = Drag
    .does = Crop, or move the crop or its edges
mouse-shape = Shift+drag a corner
    .does = Keep the crop's shape, unless an aspect ratio is set
mouse-centred = Alt+drag an edge
    .does = Resize the crop about its middle
mouse-free = Ctrl+drag
    .does = Crop without snapping to the picture's edges and middle
mouse-cancel = Esc while dragging
    .does = Put the crop back as it was
mouse-crop-keys = Click crop, then arrows
    .does = Move it a pixel, ten with Shift; Ctrl+arrows resize it
mouse-double-click = Double-click
    .does = Back to the whole picture
mouse-wheel = Wheel
    .does = Zoom in or out
mouse-pan = Right or middle drag
    .does = Pan, when zoomed in
mouse-pan-across = Shift+wheel
    .does = Pan across, when zoomed in
mouse-slide = Shift+drag a bracket
    .does = Slide the timeline's used part, keeping its length
