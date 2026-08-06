#!/usr/bin/env python3
"""Generates the horizontal MumbleWay logo as a self-contained SVG.

The same lockup the app draws in `lib/widgets/wordmark.dart`: the name stacked
in two lines with "Way" hanging off the right-hand end, the helmet tucked into
the empty quarter under "Mumble" that the indent leaves behind, the whole name
leaning forward.

The constants below are copied from that file and must be kept in step with it.
This script is the logo as an *asset* — readme, store listing, sticker on a top
box — written to brand/, which is outside the app bundle so it is never shipped
inside the app. The widget is the same logo as part of the interface.

The wordmark is converted to outlines rather than left as `<text>`. A logo that
depends on a font being installed renders as Helvetica on the one machine that
matters, and this one has a variable weight axis (600) that no system copy of
Exo 2 would have anyway.

The proportions are close to the widget's but not identical — around 2.10:1
here against 1.97:1 there. Both follow the same rules; they measure the text
differently. Flutter shapes it, with kerning and whatever else the shaper
applies, while this script sums raw glyph advances. Matching exactly would mean
reimplementing a shaper, and the two are never seen side by side. If they ever
are, the widget is the one to trust.

Usage:
    python tool/make_logo.py
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "app/assets/fonts/Exo2-Variable.ttf"
ICON = ROOT / "app/assets/icon/mumbleway.svg"

# --- kept in step with lib/widgets/wordmark.dart -----------------------------
FIRST, SECOND = "Mumble", "Way"
WEIGHT = 600
LEADING = 0.82      # second line's baseline offset, as a fraction of font size
OVERHANG = 0.10     # how far "Way" reaches past the end of "Mumble"
LEAN = 0.12         # forward shear, tan(angle)
GAP_RATIO = 0.24    # helmet-to-"Way" gap, as a fraction of the helmet
TRACKING = -0.03    # letter spacing, as a fraction of font size
# -----------------------------------------------------------------------------

HEIGHT = 256.0      # the SVG's own units; it scales to anything


def outline(font: TTFont, text: str, scale: float) -> tuple[str, float]:
    """SVG path data for `text` on a y-down baseline at the origin, and its
    advance width."""
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    paths: list[str] = []
    x = 0.0
    tracking = TRACKING * font["head"].unitsPerEm

    for char in text:
        name = cmap.get(ord(char))
        if name is None:
            raise SystemExit(f"{char!r} is not in the font")
        pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.2f}")
        glyphs[name].draw(pen)
        data = pen.getCommands()
        if data:
            # y-up in font space, y-down in SVG.
            paths.append(f'<path transform="translate({x * scale:.2f},0) '
                         f'scale({scale:.5f},{-scale:.5f})" d="{data}"/>')
        x += glyphs[name].width + tracking

    return "\n      ".join(paths), x * scale


def helmet_body(on_light: bool) -> str:
    """The icon's drawing, lifted whole from the app icon.

    Parsed rather than copied so the logo cannot drift from the launcher icon:
    change mumbleway.svg, re-run this, and both agree.

    On a light background the helmet is inverted. The app icon is a pale helmet
    on a dark tile, which is right on a phone's home screen and wrong on a white
    page — the tile becomes a heavy black brick sitting next to dark type. So
    the tile goes and the shell takes the ink colour, leaving a dark helmet with
    a blue visor. The accent is left alone: it carries against both.
    """
    ET.register_namespace("", "http://www.w3.org/2000/svg")
    root = ET.parse(ICON).getroot()
    ns = "{http://www.w3.org/2000/svg}"
    out = []
    for child in root:
        tag = child.tag.replace(ns, "")
        if tag == "defs":
            continue
        # The dark tile only makes sense behind a pale helmet.
        if on_light and tag == "rect":
            continue
        markup = ET.tostring(child, encoding="unicode").replace(ns, "")
        if on_light:
            markup = markup.replace('"#F4F8FC"', '"#101822"')  # shell
            markup = markup.replace('"#DEE8F2"', '"#243447"')  # jaw shadow
        out.append(markup)
    return "\n    ".join(out)


def gradients() -> str:
    tree = ET.parse(ICON)
    ns = "{http://www.w3.org/2000/svg}"
    defs = tree.getroot().find(f"{ns}defs")
    return "\n    ".join(
        ET.tostring(g, encoding="unicode").replace(ns, "") for g in defs
    )


def main() -> None:
    out_dir = Path(sys.argv[1] if len(sys.argv) > 1
                   else ROOT / "brand")

    font = instantiateVariableFont(TTFont(FONT), {"wght": WEIGHT})
    upem = font["head"].unitsPerEm

    # Ascender to descender, as a multiple of the font size. Taken from the
    # font rather than assumed to be 1: assuming it put the descender of the
    # "y" outside the viewBox, and "Way" rendered as "Wav".
    asc = font["hhea"].ascender / upem
    desc = -font["hhea"].descender / upem

    # Sized so the whole two-line block, descender included, is exactly HEIGHT.
    font_size = HEIGHT / (asc + desc + LEADING)
    scale = font_size / upem
    ascent = asc * font_size

    first_path, first_w = outline(font, FIRST, scale)
    second_path, second_w = outline(font, SECOND, scale)

    # The helmet hangs from the first line's baseline down to the foot of the
    # mark, so it sits in the empty quarter the indent leaves under "Mumble"
    # rather than in front of the whole lockup — where it cost its own width.
    # Starting it any higher puts its tile through the bottom of "Mumble".
    helmet_size = HEIGHT - ascent

    # "Way" starts wherever both demands are met: far enough right to overhang
    # "Mumble", and far enough right to leave the helmet a square of its own.
    indent = max(
        helmet_size * (1 + GAP_RATIO),
        first_w * (1 + OVERHANG) - second_w,
    )
    total_w = max(first_w, indent + second_w) + HEIGHT * LEAN

    # The lean is applied about the bottom of the block, so the mark is thrown
    # forward rather than slid sideways.
    lean = (f'matrix(1 0 {-LEAN} 1 {LEAN * HEIGHT:.2f} 0)')

    def build(ink: str, on_light: bool) -> str:
        return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {total_w:.0f} {HEIGHT:.0f}" width="{total_w:.0f}" height="{HEIGHT:.0f}">
  <!--
    MumbleWay horizontal logo. GENERATED — do not edit by hand.
      python tool/make_logo.py

    The name breaks across two lines and the second starts well to the right,
    finishing past where the first ended. The eye crosses the mark diagonally,
    the way the road arrives, and "Way" is left hanging off the end rather than
    tucked into the block — travelling rather than labelled. The forward lean
    says it again, quietly.

    Narrower than "MumbleWay" set in one run, which is the point: an app bar on
    a phone has no width to spare and plenty of unused height.

    The wordmark is outlined, so this needs no fonts installed. Exo 2 at weight
    600, SIL Open Font License 1.1 — see app/assets/fonts/Exo2-OFL.txt.

    The layout follows app/lib/widgets/wordmark.dart, which draws the same
    lockup live in the interface. Change one, change the other. The two are not
    pixel-identical: Flutter shapes the text and this script sums advances, so
    the proportions differ by a few percent.
  -->
  <defs>
    {gradients()}
  </defs>

  <!-- Helmet, hung from the first line's baseline. -->
  <g transform="translate(0,{ascent:.2f}) scale({helmet_size / 1024:.6f})">
    {helmet_body(on_light)}
  </g>

  <!-- The name. -->
  <g fill="{ink}">
    <g transform="{lean}">
      <g transform="translate(0,{ascent:.2f})">
      {first_path}
      </g>
      <g transform="translate({indent:.2f},{ascent + font_size * LEADING:.2f})">
      {second_path}
      </g>
    </g>
  </g>
</svg>
"""

    # Two files rather than one. The wordmark is a single flat colour, and a
    # near-white one disappears on a white README exactly as a near-black one
    # would in the app. The helmet carries its own dark tile and reads against
    # either, so only the name changes.
    out_dir.mkdir(parents=True, exist_ok=True)
    for suffix, ink, on_light in (
        ("on-dark", "#F4F8FC", False),
        ("on-light", "#101822", True),
    ):
        out = out_dir / f"mumbleway-logo-{suffix}.svg"
        out.write_text(build(ink, on_light), encoding="utf-8")
        print(f"{out}  {total_w:.0f}x{HEIGHT:.0f}  (aspect {total_w / HEIGHT:.2f}:1)")


if __name__ == "__main__":
    main()
