#!/usr/bin/env python3
"""Build the CJK subset font for the `teaching` binary (ZH/JA/KO PDFs).

genpdf embeds the full font file; the full Arial Unicode is 23 MB (and gets
embedded four times, once per style slot), so the ZH/JA PDFs would be huge.
This script collects every character actually used in texts_zh.rs/texts_ja.rs/texts_ko.rs,
subsets Arial Unicode down to those (plus ASCII and the punctuation main.rs
adds itself) and writes the result outside the repo (an Apple system font
must not be committed). Re-run after changing the ZH/JA/KO texts, then:

    CJK_FONT=~/.config/pegelstand/fonts/ArialUnicode-CJK-subset.ttf \
        cargo run --release --bin teaching -- ZH JA KO

Needs fonttools (e.g. `uv run --with fonttools python teaching/make_cjk_subset.py`
or a venv with `pip install fonttools`).
"""
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SRC = ROOT / "src" / "bin" / "teaching"
OUT = pathlib.Path.home() / ".config" / "pegelstand" / "fonts" / "ArialUnicode-CJK-subset.ttf"
FULL = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf"
# ASCII + punctuation/symbols emitted by main.rs itself (Ü ü · — – ‘ ’ “ ” … ° ² →)
EXTRA = "U+0020-007E,U+00B0,U+00B2,U+00B7,U+00DC,U+00FC,U+2013,U+2014,U+2018,U+2019,U+201C,U+201D,U+2026,U+2192"

chars = set()
for name in ("texts_zh.rs", "texts_ja.rs", "texts_ko.rs"):
    chars |= set((SRC / name).read_text(encoding="utf-8"))
chars = {c for c in chars if ord(c) >= 0x20}

OUT.parent.mkdir(parents=True, exist_ok=True)
charfile = OUT.parent / "cjk_chars.txt"
charfile.write_text("".join(sorted(chars)), encoding="utf-8")

subprocess.run(
    [sys.executable, "-m", "fontTools.subset", FULL,
     f"--text-file={charfile}", f"--unicodes={EXTRA}", f"--output-file={OUT}"],
    check=True,
)
print(f"wrote {OUT} ({OUT.stat().st_size:,} bytes, {len(chars)} chars from texts)")
