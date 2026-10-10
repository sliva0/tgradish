# Fonts of the window

Subsets of three fonts, so the binary carries about 160 KB of fonts
instead of the 1.4 MB of egui's own set. Each keeps Latin (with its
extensions), Greek, Cyrillic, punctuation, currency and arrows; the icon
font keeps the symbols the window uses. Hinting is dropped: egui doesn't
use it.

| File | From | Licence |
|---|---|---|
| `Ubuntu-Light-tgradish.ttf` | Ubuntu Light, as egui ships it (`epaint_default_fonts`) | Ubuntu Font Licence 1.0, `Ubuntu-LICENCE.txt` |
| `JetBrainsMonoNL-Regular-tgradish.ttf` | JetBrains Mono NL Regular 2.304, <https://github.com/JetBrains/JetBrainsMono> | SIL OFL 1.1, `JetBrainsMono-OFL.txt` |
| `emoji-icon-font-tgradish.ttf` | emoji-icon-font, as egui ships it | MIT, `emoji-icon-font-LICENSE.txt` |

The Ubuntu Font Licence asks modified fonts to keep the original name and
add to it, so the subset of Ubuntu Light is named "Ubuntu Light tgradish".

Made with fontTools:

```console
ranges="U+0020-007E,U+00A0-024F,U+0370-03FF,U+0400-04FF,U+2000-206F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2212,U+2260-2265"
uvx --from fonttools pyftsubset Ubuntu-Light.ttf --unicodes="$ranges" --no-hinting \
  --output-file=Ubuntu-Light-tgradish.ttf
uvx --from fonttools pyftsubset JetBrainsMonoNL-Regular.ttf --unicodes="$ranges" \
  --no-hinting --desubroutinize --output-file=JetBrainsMonoNL-Regular-tgradish.ttf
uvx --from fonttools pyftsubset emoji-icon-font.ttf --text="✖⚠✔⟳⟲⚙▶⏸⏷⏵⏹" \
  --no-hinting --output-file=emoji-icon-font-tgradish.ttf
```

then, for Ubuntu Light, names 1 and 16 set to "Ubuntu tgradish", 4 to
"Ubuntu Light tgradish" and 6 to "Ubuntu-Light-tgradish" with fontTools'
`TTFont`. A symbol the window starts to use must be added to the icon
font's `--text`.
