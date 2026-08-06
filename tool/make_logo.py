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

# Once, at import, and never inside a function.
#
# ElementTree keeps this in module-global state, so registering it inside a
# helper means the *first* markup serialised before that helper first runs
# comes out namespace-prefixed — <ns0:linearGradient xmlns:ns0="..."> — and
# everything afterwards comes out clean. That is exactly what happened: the
# first variant generated had a prefixed gradient, the second did not, and
# flutter_svg silently declined to resolve the prefixed one. The visor and the
# sound waves rendered as nothing at all, in one file of two, depending on the
# order they were written in. Browsers resolve both forms, so it looked correct
# everywhere except the app.
ET.register_namespace("", "http://www.w3.org/2000/svg")

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


def helmet_body(on_light: bool, suffix: str) -> str:
    """The icon's drawing, lifted whole from the app icon.

    Parsed rather than copied so the logo cannot drift from the launcher icon:
    change mumbleway.svg, re-run this, and both agree.

    The tile never comes with it. A rounded dark square behind the helmet is a
    launcher device — it exists so the icon has an edge on somebody's home
    screen — and in a logo it is either a black brick sitting next to dark type
    or, on a dark app bar, an invisible square leaving the shell to float as a
    pale blob. Dropped from both variants, the helmet is a shape like the
    letters are shapes, and it takes the same ink they do.

    The accent is left alone. Blue carries against either background, and it is
    the one part of the mark that says something the silhouette cannot.
    """
    root = ET.parse(ICON).getroot()
    ns = "{http://www.w3.org/2000/svg}"
    out = []
    for child in root:
        tag = child.tag.replace(ns, "")
        if tag == "defs":
            continue
        if tag == "rect":
            continue
        markup = ET.tostring(child, encoding="unicode").replace(ns, "")
        if on_light:
            markup = markup.replace('"#F4F8FC"', '"#101822"')  # shell
            markup = markup.replace('"#DEE8F2"', '"#243447"')  # jaw shadow
        markup = markup.replace("url(#accent)", f"url(#accent-{suffix})")
        out.append(markup)
    return "\n    ".join(out)


def gradients(suffix: str) -> str:
    """The accent gradient, under a name unique to this variant.

    Unique because both files are loaded by the same app, and flutter_svg drew
    the dark variant's visor and sound waves as nothing at all while the light
    one — identical but for two flat colours — came out right. Two documents
    declaring `id="accent"` is the difference between them and a browser, which
    scopes ids per document and renders both correctly.

    The tile's gradient is not emitted: the tile went, and a definition nothing
    references is one more chance to collide.
    """
    ns = "{http://www.w3.org/2000/svg}"
    defs = ET.parse(ICON).getroot().find(f"{ns}defs")
    out = []
    for g in defs:
        if g.get("id") != "accent":
            continue
        g.set("id", f"accent-{suffix}")
        out.append(ET.tostring(g, encoding="unicode").replace(ns, ""))
    return "\n    ".join(out)


def main() -> None:
    out_dir = Path(sys.argv[1] if len(sys.argv) > 1
                   else ROOT / "app/assets/logo")

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

    # The helmet rides immediately in front of "Way" rather than sitting flush
    # against the left edge. Flush left it read as a third element of the
    # lockup, floating under the start of "Mumble" with a hole between it and
    # the word it belongs to; tucked against "Way" the two read as one object
    # travelling together, which is the whole point of the second line.
    #
    # "Way" is inside the sheared group, so its visual left edge is not `indent`
    # — the shear moves it by LEAN*(HEIGHT - y), and y differs down the glyph.
    # Measured at the helmet's own middle, which is where the eye judges the gap.
    helmet_mid = ascent + helmet_size / 2
    way_left = indent + LEAN * (HEIGHT - helmet_mid)
    helmet_x = max(0.0, way_left - helmet_size * (1 + GAP_RATIO))

    # The lean is applied about the bottom of the block, so the mark is thrown
    # forward rather than slid sideways.
    lean = (f'matrix(1 0 {-LEAN} 1 {LEAN * HEIGHT:.2f} 0)')

    def build(ink: str, on_light: bool, suffix: str) -> str:
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
    {gradients(suffix)}
  </defs>

  <!-- Helmet: hung from the first line's baseline, tucked in front of "Way". -->
  <g transform="translate({helmet_x:.2f},{ascent:.2f}) scale({helmet_size / 1024:.6f})">
    {helmet_body(on_light, suffix)}
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
    ):  # helmet and letters share the ink; only the accent stays put
        out = out_dir / f"mumbleway-logo-{suffix}.svg"
        out.write_text(build(ink, on_light, suffix), encoding="utf-8")
        print(f"{out}  {total_w:.0f}x{HEIGHT:.0f}  (aspect {total_w / HEIGHT:.2f}:1)")


if __name__ == "__main__":
    main()
