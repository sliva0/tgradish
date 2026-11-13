# Art of the README's screenshot

The pictures `readme_screenshot` in `src/screenshots.rs` loads into the
window for the README. Not part of the crate's package.

All are from **Sunny Land 2D Pixel Art Pack** by **Luis Zuno (ansimuz)**,
released under **CC0** (public domain), as the pack's own
`public-license.txt` says too: "Public domain and free to use on whatever
you want, personal or commercial. Credit is not required but appreciated."
The pack's music, by Pascal Belisle, is under other terms and is not used.

- Source: <https://opengameart.org/content/sunny-land-2d-pixel-art-pack>,
  whose "License(s)" says CC0; the author's own page,
  <https://ansimuz.itch.io/sunny-land-pixel-game-art>, gives it as Creative
  Commons Zero v1.0 Universal
- File: `sunny-land-files.zip`, sha256
  `17b8b3d9313c747e189b4bf84a4f6e1187b098841a01cb285fdb15be44266ec0`
- Fetched: 2026-11-13

| File | From the pack | Changed |
| --- | --- | --- |
| `frog-idle.png` | `PNG/spritesheets/frog-idle.png` | none |
| `frog-jump.png` | `PNG/spritesheets/frog-jump.png` | none |
| `fox-idle.png` | `PNG/spritesheets/player-idle.png` | renamed |
| `fox-run.png` | `PNG/spritesheets/payer-run.png` | renamed |
| `sparkle.gif` | `GIF/sprites/FX/item-feedback.gif` | renamed |
| `eagle.gif` | `GIF/sprites/eagle/attack.gif` | renamed |
| `cherry.gif` | `GIF/sprites/items/cherry.gif` | none |
| `house.png` | `PNG/environment/props/house.png` | none |
| `seaside.png` | `PNG/environment/environment-preview.png` | cropped to 756x216 from 700,0; metadata stripped |

The screenshot makes Aseprite files of the sheets, cut into frames, 100 ms
a frame as the pack's GIFs have them: `frog.aseprite`, 35x32, tagged
"idle", the calm first frame repeated ten times and then the croak, as the
pack's GIF holds it for a second, and "jump"; and `fox.aseprite`, 33x32,
tagged "idle" and "run". It makes `seaside.mp4` of `seaside.png`, a
384x216 view panning along it for three seconds at twice its size.
