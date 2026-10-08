# Front-end protocol

How a GUI (or any other program) drives the `tgradish` CLI. A Rust
front-end can use the `tgradish-core` crate directly instead; the types are
the same.

tgradish makes two formats: WebM video stickers and emoji, and `.tgs`
animated stickers from pixel art. Each has its own options, presets and
events, all described by `describe`.

## Version

`tgradish describe` prints `"protocol": 2`. It changes when the description,
events or the flags below change incompatibly. Adding options, presets or
event fields is not an incompatible change, so front-ends should ignore
what they don't know. Protocol 2 added `.tgs`: `formats` replaced the
top-level `options`, `events`, `output_extension` and `default_preset`, and
presets got a `format`.

## Describe

`tgradish describe` prints one JSON object:

```jsonc
{
  "protocol": 2,
  "tool": "tgradish",
  "version": "2.0.0",
  // the format made when nothing asks for another
  "default_format": "webm",
  "formats": [
    {
      "format": "webm",
      "description": "Video sticker or emoji, from any video or image",
      "output_extension": "webm",
      // used when no preset is given; config.toml can change it
      "default_preset": "sticker",
      // JSON Schema (2020-12) of this format's options object
      "options": { "type": "object", "properties": { "fit": { "...": "..." } } },
      // JSON Schema of the events `convert --json` prints for it
      "events": { "oneOf": [ "..." ] }
    },
    { "format": "tgs", "output_extension": "tgs", "default_preset": "tgs-sticker", "...": "..." }
  ],
  "presets": [
    {
      "name": "emoji",
      "description": "Custom emoji, 100x100 px",
      "builtin": true,
      // the format the preset is for; null if broken
      "format": "webm",
      // options with everything the preset extends applied; null if broken
      "options": { "target": "emoji" },
      // why the preset is broken, null otherwise
      "error": null
    }
  ]
}
```

Building a form from a format's `options`:

- every property is optional; leaving it out means "use the default", and
  each property's `description` says what the default is;
- `enum`/`oneOf` of strings → dropdown, `boolean` → toggle, `number` or
  `integer` with `minimum`/`maximum` → slider or spin box, `string` → text
  field, `array` of strings → list editor;
- a preset fills in the values it sets.

## Convert

```
tgradish --json convert INPUT... [-o OUTPUT] [-y] [--format webm|tgs]
    [--preset NAME] [--options-json JSON] [--sequence]
```

The format is `--format`, or else the extension of `-o`, or else the
preset's format, or else `webm`. Options are applied in this order, later
ones winning: built-in defaults, the preset (which must be for the same
format), `--options-json` (that format's options), then any `--<property>`
flags. Every property of both options schemas exists as a flag with the
same name, so a front-end can pass either a JSON object or flags; flags
that only the other format has are an error. `--sequence` (`.tgs` only)
joins the inputs, image files or directories of them, into one sticker.

### WebM events

With `--json`, stdout carries one JSON event per line, tagged by `event`:

| event | meaning |
| --- | --- |
| `started` | conversion planned; `plan` has every resolved setting |
| `attempt_started` | an encode begins; `params` has fps, length and rate |
| `progress` | `fraction` (0 to 1) of pass `pass` of `passes` |
| `attempt_finished` | encode done: `bytes`, and whether it `fits` |
| `scored` | `ssim` of an attempt (only with `fit: auto`) |
| `warning` | `message` about an adjusted or ignored option |
| `log` | a line of ffmpeg output |
| `finished` | `output`, `bytes`, the kept `params`, `spoofed`, and `issues` Telegram would still have |
| `error` | `message`, and the `input` it is about when converting several |

### `.tgs` events

| event | meaning |
| --- | --- |
| `started` | the input is read; `report` says what normalising found and did: cells, pixel scale, colours, frames, length, crop, speed-up |
| `too_large` | losslessly the sticker would come to about `bytes`; fitting starts |
| `reduced` | fitting applied `step`: a `reduction` with its strength, the estimated `bytes` after it and the `error` so far |
| `packing` | the result is being compressed |
| `warning` | `message` about an adjusted or ignored option |
| `finished` | `output`, `bytes`, `json_bytes`, whether it is `lossy`, the `steps` fitting took, `layers`, `rectangles`, and `issues` Telegram would still have (`severity` error or warning, and a `message`) |
| `error` | as for WebM |

### Errors and exit status

An `error` without `input` ends the process. With several inputs, a failed
conversion prints an `error` with its `input` and tgradish moves on to the
next one; the exit status is then 1.

Exit status: 0 on success, 1 on errors, 2 on invalid command lines, 130
when cancelled. Invalid command lines are reported by the argument parser
as text on stderr, not as JSON.

To cancel, write `cancel` and a newline to stdin, or send SIGINT on Unix.
tgradish then stops ffmpeg, deletes its temporary files and exits with 130.
`.tgs` conversions stop between steps, within a few seconds at most.
Closing stdin does nothing. Killing the process works too, but leaves
temporary files behind.

## Watch

`tgradish --json watch DIR [conversion flags]` converts files as they
appear and prints the same events as `convert`, with each `started` event
naming its input. A failed conversion prints an `error` with its `input`
and watching goes on; failures that might be temporary, like a locked
file, are retried a few times. Directories that cannot be read are reported
as `error` events without `input`, once, and watching goes on. Files
already in the directory are left alone unless `--existing` is given. With
`--recursive` and `--output-dir`, results keep the subdirectories of their
inputs. Stop it like a conversion; it then exits with 0, or 130 if a
conversion was running.

## Other commands

All of these accept `--json`:

- `inspect FILE...`: one JSON object per line. For WebM: `file`,
  `"format": "webm"`, `target`, `info` (stream properties and metadata)
  and `issues`. For `.tgs` (by extension, or gzip data): `file`,
  `"format": "tgs"`, `stats` (canvas, fps, frames, sizes, layer and shape
  counts against Telegram's renderer, features used) and `issues`. Files
  that could not be read give `file` and `error`;
- `spoof FILE [-o OUT | --in-place] [--duration S]`: `output` and a
  `report` of what changed;
- `preset list`: the `presets` array from `describe`;
- `preset show NAME`: the preset's options with what it extends applied;
- `ffmpeg status`: `ffmpeg` (paths and where it came from) and
  `capabilities` (`version`, and `libvpx_vp9`: whether it can encode
  stickers; if not, the exit status is 1).

Commands other than `convert` and `inspect` print a single JSON document.
Every command prints an `error` event instead when it fails.
