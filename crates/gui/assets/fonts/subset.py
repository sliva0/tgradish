# /// script
# requires-python = ">=3.10"
# dependencies = ["fonttools==4.68.0"]
# ///
"""Makes the GUI's fonts here: small subsets of the originals.

Run from anywhere with `uv run crates/gui/assets/fonts/subset.py`. Each font
keeps the characters its file here has, and those in ADDED; to draw more of
a script, add them there and run it again. The originals are fetched from
where they are published and checked against their SHA-256.

Ubuntu's are renamed "Ubuntu Light derivative tgradish" and "Ubuntu Medium
derivative tgradish" within, as the Ubuntu Font Licence asks of a version
changed but not substantially. Each font keeps its licence beside it.
"""

import hashlib
import io
import tarfile
import urllib.request
import zipfile
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

HERE = Path(__file__).resolve().parent

# Where each original comes from, with the SHA-256 of what is fetched.
SOURCES = {
    "ubuntu": (
        "https://assets.ubuntu.com/v1/0cef8205-ubuntu-font-family-0.83.zip",
        "61a2b342526fd552f19fef438bb9211a8212de19ad96e32a1209c039f1d68ecf",
    ),
    "jetbrains-mono": (
        "https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip",
        "6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf",
    ),
    # egui's icon font, as its crate of default fonts carries it
    "egui": (
        "https://static.crates.io/crates/epaint_default_fonts/epaint_default_fonts-0.36.2.crate",
        "773fa9c96dd0dbef887e39d0ed6177f141cce4d68e6041df77570aa3702dfa13",
    ),
}

# Each font here: its original, as a file within a source, how it is cut
# down, and the name it is given within, if it is renamed.
FONTS = {
    "Ubuntu-Light-derivative-tgradish.ttf": (
        "ubuntu",
        "ubuntu-font-family-0.83/Ubuntu-L.ttf",
        {"hinting": False},
        "Ubuntu Light derivative tgradish",
    ),
    "Ubuntu-Medium-derivative-tgradish.ttf": (
        "ubuntu",
        "ubuntu-font-family-0.83/Ubuntu-M.ttf",
        {"hinting": False},
        "Ubuntu Medium derivative tgradish",
    ),
    "JetBrainsMonoNL-Regular-tgradish.ttf": (
        "jetbrains-mono",
        "fonts/ttf/JetBrainsMonoNL-Regular.ttf",
        {"hinting": False},
        None,
    ),
    "emoji-icon-font-tgradish.ttf": (
        "egui",
        "epaint_default_fonts-0.36.2/fonts/emoji-icon-font.ttf",
        {"recalc_bounds": True},
        None,
    ),
}

TEXT = [
    "Ubuntu-Light-derivative-tgradish.ttf",
    "Ubuntu-Medium-derivative-tgradish.ttf",
    "JetBrainsMonoNL-Regular-tgradish.ttf",
]

# Characters each font is to have beyond those its file here has: Uzbek's
# ʻ (U+02BB) for the text, and the globe that marks the choice of language.
ADDED = {
    **{file: {0x02BB} for file in TEXT},
    "emoji-icon-font-tgradish.ttf": {0x1F310},
}

# Characters the originals lack, drawn with the glyph of another that looks
# the same: Uzbek's oʻ and gʻ with the turned comma, as ‘.
ALIASES = {0x02BB: 0x2018}


def fetch(source):
    url, sha256 = SOURCES[source]
    with urllib.request.urlopen(url) as response:
        data = response.read()
    found = hashlib.sha256(data).hexdigest()
    if found != sha256:
        raise SystemExit(f"{url}: SHA-256 {found}, not {sha256}")
    return data


def original(data, inside):
    if data[:2] == b"PK":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            return archive.read(inside)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        return archive.extractfile(inside).read()


def rename(font, name):
    names = font["name"]
    given = {
        1: name,
        2: "Regular",
        3: f"{name} (a subset)",
        4: name,
        6: name.replace(" ", "-"),
    }
    for name_id, text in given.items():
        names.setName(text, name_id, 3, 1, 0x409)
    # each its family's regular style, Medium's no longer bold
    font["OS/2"].fsSelection = 1 << 6
    font["head"].macStyle = 0
    # changed from 0.83
    font["head"].fontRevision = 0.831


def make(file, data):
    source, inside, cut, name = FONTS[file]
    path = HERE / file
    kept = set(TTFont(path).getBestCmap()) | ADDED.get(file, set())
    options = subset.Options()
    for option, value in cut.items():
        setattr(options, option, value)
    font = subset.load_font(io.BytesIO(original(data, inside)), options)
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=kept)
    subsetter.subset(font)
    drawn = font.getBestCmap()
    for alias, of in ALIASES.items():
        if alias in kept and alias not in drawn and of in drawn:
            for table in font["cmap"].tables:
                if table.isUnicode():
                    table.cmap[alias] = drawn[of]
    if name:
        rename(font, name)
    before = path.stat().st_size
    subset.save_font(font, path, options)
    lacking = kept - set(font.getBestCmap())
    print(f"{file}: {before} -> {path.stat().st_size} bytes")
    if lacking:
        listed = " ".join(f"U+{code:04X}" for code in sorted(lacking))
        print(f"    the original lacks {listed}")


def main():
    fetched = {}
    for file, (source, *_) in FONTS.items():
        if source not in fetched:
            fetched[source] = fetch(source)
        make(file, fetched[source])


if __name__ == "__main__":
    main()
