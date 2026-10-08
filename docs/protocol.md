# Front-end protocol

How a GUI (or any other program) drives the `tgradish` CLI. A Rust
front-end can use the `tgradish-core` crate directly instead; the types are
the same.

The protocol is meant to be shared with pixelart2tgs, so one GUI can wrap
both tools: everything tool-specific comes from `describe`.

## Version

`tgradish describe` prints `"protocol": 1`. It changes when the description,
events or the flags below change incompatibly. Adding options, presets or
event fields is not an incompatible change, so front-ends should ignore
what they don't know.

## Describe

`tgradish describe` prints one JSON object:

```jsonc
{
  "protocol": 1,
  "tool": "tgradish",
  "version": "2.0.0",
  "output_extension": "webm",
  "default_preset": "sticker",
  "presets": [
    {
      "name": "emoji",
      "description": "Custom emoji, 100x100 px",
      "builtin": true,
      // options with everything the preset extends applied; null if broken
      "options": { "target": "emoji" },
      // why the preset is broken, null otherwise
      "error": null
    }
  ],
  // JSON Schema (2020-12) of the options object
  "options": { "type": "object", "properties": { "fit": { "...": "..." } } },
  // JSON Schema of the events printed by `convert --json`
  "events": { "oneOf": [ "..." ] }
}
```

Building a form from `options`:

- every property is optional; leaving it out means "use the default", and
  each property's `description` says what the default is;
- `enum`/`oneOf` of strings → dropdown, `boolean` → toggle, `number` or
  `integer` with `minimum`/`maximum` → slider or spin box, `string` → text
  field, `array` of strings → list editor;
- a preset fills in the values it sets.

## Convert

```
tgradish --json convert INPUT [-o OUTPUT] [-y] [--preset NAME] [--options-json JSON]
```

Options are applied in this order, later ones winning: built-in defaults,
the preset, `--options-json`, then any `--<property>` flags. Every property
of the options schema also exists as a flag with the same name, so a
front-end can pass either a JSON object or flags.

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

An `error` without `input` ends the process. With several inputs, a failed
conversion prints an `error` with its `input` and tgradish moves on to the
next one; the exit status is then 1.

Exit status: 0 on success, 1 on errors, 2 on invalid command lines, 130
when cancelled. Invalid command lines are reported by the argument parser
as text on stderr, not as JSON.

To cancel, write `cancel` and a newline to stdin, or send SIGINT on Unix.
tgradish then stops ffmpeg, deletes its temporary files and exits with 130.
Closing stdin does nothing. Killing the process works too, but leaves
temporary files behind.

## Watch

`tgradish --json watch DIR [conversion flags]` converts files as they
appear and prints the same events as `convert`, with each `started` event
naming its input. A failed conversion prints an `error` with its `input`
and watching goes on. Files already in the directory are left alone unless
`--existing` is given. Stop it like a conversion; it then exits with 0, or
130 if a conversion was running.

## Other commands

All of these accept `--json`:

- `inspect FILE...`: one JSON object per line with `file`, `target`,
  `info` (stream properties and metadata) and `issues`, or `file` and
  `error` for files that could not be read;
- `spoof FILE [-o OUT | --in-place] [--duration S]`: `output` and a
  `report` of what changed;
- `preset list`: the `presets` array from `describe`;
- `ffmpeg status`: `ffmpeg` (paths and where it came from) and
  `capabilities` (`version`, and `libvpx_vp9`: whether it can encode
  stickers; if not, the exit status is 1).

Commands other than `convert` and `inspect` print a single JSON document.
Every command prints an `error` event instead when it fails.
