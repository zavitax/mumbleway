#!/usr/bin/env python3
"""Generates the graphics every store wants, into brand/store/.

    python tool/make_store_assets.py

Each store asks for the icon at a different size, in a different colour space,
and with a different opinion about corners — and rejects the upload at the end
of a long form rather than at the start. This produces all of them from the one
source SVG, so they cannot drift apart and none of them is a hand-resize of
another.

# Corners, which is the part that goes wrong quietly

`assets/icon/mumbleway.svg` has its own rounded corners (rx=228). That is right
for a file rendered on its own and wrong for a store that applies its own mask:
iOS and Google Play both round the icon themselves, so a pre-rounded upload is
rounded twice and the corners come out visibly wrong — a thin dark crescent
inside the mask.

So the masked stores get a **square, full-bleed** render, and only the stores
that show the icon as-is get the rounded one.

# Alpha

Apple rejects a marketing icon with an alpha channel outright. Since the square
render is full-bleed there is nothing transparent in it anyway, but the channel
itself has to go, so those are written as RGB.

# What this cannot make

Screenshots. They need the app running on each device size, against a real
server with people in the channel, and an invented one would misrepresent the
product. See brand/store/README.md for the list.
"""

import io
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import numpy as np
    from PIL import Image, ImageDraw
except ImportError:  # pragma: no cover
    sys.exit("needs Pillow and numpy: python -m pip install pillow numpy")

ROOT = Path(__file__).resolve().parent.parent
ICON_SVG = ROOT / "app" / "assets" / "icon" / "mumbleway.svg"
LOGO_SVG = ROOT / "app" / "assets" / "logo" / "mumbleway-logo-on-dark.svg"
OUT = ROOT / "brand" / "store"

# Straight out of the icon's own gradients. Nothing here is a new colour: a
# store page that is a slightly different blue from the app it is selling looks
# like a counterfeit of it.
BG_TOP = (0x1B, 0x27, 0x35)
BG_BOTTOM = (0x0A, 0x0F, 0x16)
ACCENT_LIGHT = (0x6B, 0xC1, 0xF2)
ACCENT_DARK = (0x2E, 0x86, 0xC1)


def rsvg(svg: bytes, width: int, height: int) -> Image.Image:
    """Renders SVG bytes at an exact pixel size."""
    if shutil.which("rsvg-convert") is None:
        sys.exit(
            "rsvg-convert is not on PATH. It is what the icon pipeline already "
            "uses (see app/pubspec.yaml); install librsvg."
        )
    done = subprocess.run(
        ["rsvg-convert", "-w", str(width), "-h", str(height), "-f", "png"],
        input=svg,
        capture_output=True,
        check=True,
    )
    return Image.open(io.BytesIO(done.stdout)).convert("RGBA")


def icon(size: int, *, square: bool) -> Image.Image:
    """The app icon, optionally with its rounded corners squared off."""
    svg = ICON_SVG.read_bytes()
    if square:
        # The corner radius lives in exactly one attribute, on the background
        # rect. Asserted rather than assumed: a silent no-op here produces a
        # double-rounded icon that only shows up on a device.
        if b'rx="228"' not in svg:
            sys.exit("the icon's corner radius moved; update make_store_assets.py")
        svg = svg.replace(b'rx="228"', b'rx="0"')
    return rsvg(svg, size, size)


def gradient(size: tuple[int, int], top: tuple, bottom: tuple, *, diagonal=True):
    """The icon's background gradient, at any aspect."""
    w, h = size
    ys, xs = np.mgrid[0:h, 0:w]
    if diagonal:
        # Matches the icon's own x1,y1 -> x2,y2 of (0,0) -> (0.6,1).
        t = (xs / max(w - 1, 1)) * 0.6 + (ys / max(h - 1, 1))
        t = t / t.max()
    else:
        t = ys / max(h - 1, 1)
    field = np.zeros((h, w, 3), dtype=np.float64)
    for i in range(3):
        field[:, :, i] = top[i] + (bottom[i] - top[i]) * t
    return Image.fromarray(field.round().astype(np.uint8), "RGB")


def accent_field(size: tuple[int, int]) -> Image.Image:
    """A diagonal wash between the two accent stops, to paint through a mask."""
    return gradient(size, ACCENT_LIGHT, ACCENT_DARK)


def banner(width: int, height: int) -> Image.Image:
    """A wide promotional image: the lockup, and the icon's arcs carrying right.

    Deliberately carries no words but the name. The listing is in English and
    Russian, and a graphic with a baked-in English tagline is the one part of a
    localised page that cannot follow the reader — so the tagline stays in the
    text fields, where it gets translated.
    """
    image = gradient((width, height), BG_TOP, BG_BOTTOM).convert("RGBA")

    # Fitted against both edges, not just height. The lockup is roughly 2.1:1,
    # so sizing it from height alone works until the frame goes portrait and
    # then puts a 1900 px logo on a 1440 px canvas.
    #
    # Landscape sizes are unchanged in layout but not always to the byte: this
    # rounds once at the end where the old code rounded the height first, so
    # 1920x1080 lands a single pixel wider and its antialiasing moves with it.
    logo_w = min(width * 0.68, height * 0.42 * 537 / 256)
    logo = rsvg(LOGO_SVG.read_bytes(), int(logo_w), int(logo_w * 256 / 537))

    # Left of centre in landscape, where there is room to the right for the
    # arcs to run into. A portrait frame has no such room, so it centres.
    logo_x = int(width * 0.07) if width >= height else (width - logo.width) // 2
    logo_y = (height - logo.height) // 2

    # The arcs start at the helmet inside the lockup, not at some point chosen
    # for the composition. That is the icon's whole idea -- a voice carrying
    # cleanly out of a helmet -- and an arc springing from nothing says nothing.
    #
    # Where the helmet sits within the logo, as a fraction of the lockup: the
    # generated SVG places it at translate(128.06,126.61) scale(0.126361) in a
    # 537x256 viewBox, and the shell spans 108..622 by 236..706 in icon units.
    helmet = (
        logo_x + logo.width * 0.324,
        logo_y + logo.height * 0.727,
    )

    # Struck through a mask so they can take the accent gradient rather than a
    # flat colour. They run off the frame on purpose -- a wave that stops in
    # mid-air reads as a drawn arc, and one that leaves reads as carrying on --
    # and they stay faint enough to be a field the wordmark sits on rather than
    # a second thing competing with it.
    mask = Image.new("L", (width, height), 0)
    pen = ImageDraw.Draw(mask)
    stroke = max(2, int(height / 150))
    # Spaced against the width, because that is the direction they travel. Using
    # the longer edge would space them by height in a portrait frame and leave
    # three enormous arcs in it; every landscape size here is wider than it is
    # tall, so this is the same number they were already getting.
    span = width
    for i in range(1, 11):
        radius = int(logo.height * 0.34 + span * 0.115 * i)
        alpha = int(105 * (1 - (i - 1) / 11))
        pen.arc(
            [
                helmet[0] - radius,
                helmet[1] - radius,
                helmet[0] + radius,
                helmet[1] + radius,
            ],
            start=-58,
            end=58,
            fill=alpha,
            width=stroke,
        )
    image.paste(accent_field((width, height)), (0, 0), mask)

    # Composited last, so the name stays crisp over its own wash.
    image.alpha_composite(logo, (logo_x, logo_y))
    return image.convert("RGB")


def write(image: Image.Image, path: Path, *, drop_alpha=False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if drop_alpha and image.mode != "RGB":
        image = image.convert("RGB")
    image.save(path, "PNG", optimize=True)
    kb = path.stat().st_size / 1024
    mode = image.mode
    print(f"  {path.relative_to(ROOT).as_posix():<52} {image.width}x{image.height}  {mode}  {kb:6.0f} kB")


# What each file has to be for the store it is going to. Checked after writing
# rather than trusted, because every one of these is rejected by an upload form
# at the end of a long submission, and "1024x1024, no alpha" is exactly the kind
# of thing that is true right up until somebody changes one line above.
SPECS = [
    ("app-store/icon-1024.png", 1024, 1024, False, True),
    ("mac-app-store/icon-1024.png", 1024, 1024, False, True),
    ("google-play/icon-512.png", 512, 512, None, True),
    ("google-play/feature-graphic-1024x500.png", 1024, 500, None, True),
    ("microsoft-store/store-logo-300.png", 300, 300, None, False),
    ("microsoft-store/hero-1920x1080.png", 1920, 1080, None, True),
    ("microsoft-store/poster-1440x2160.png", 1440, 2160, None, True),
    ("microsoft-store/box-art-2160x2160.png", 2160, 2160, None, True),
]


def verify() -> int:
    """Returns the number of files that would be refused."""
    failures = 0
    print("\nchecking what was written")
    for rel, want_w, want_h, want_alpha, want_square in SPECS:
        path = OUT / rel
        image = Image.open(path)
        problems = []

        if image.size != (want_w, want_h):
            problems.append(f"is {image.width}x{image.height}, wanted {want_w}x{want_h}")

        has_alpha = image.mode in ("RGBA", "LA") or "transparency" in image.info
        if want_alpha is False and has_alpha:
            problems.append("has an alpha channel, which Apple refuses")

        # Square means the corners are painted, not transparent. This is the
        # check that catches a pre-rounded icon going to a store that rounds it
        # again, which is invisible in a file listing and obvious on a phone.
        rgba = image.convert("RGBA")
        corners = [
            rgba.getpixel((0, 0)),
            rgba.getpixel((image.width - 1, 0)),
            rgba.getpixel((0, image.height - 1)),
            rgba.getpixel((image.width - 1, image.height - 1)),
        ]
        opaque_corners = all(pixel[3] == 255 for pixel in corners)
        if want_square and not opaque_corners:
            problems.append("has transparent corners; this store applies its own mask")
        if not want_square and opaque_corners:
            problems.append("has square corners; this store shows the icon as-is")

        state = "ok" if not problems else "; ".join(problems)
        failures += bool(problems)
        print(f"  {rel:<44} {state}")
    return failures


def main() -> int:
    print("Apple — square, because iOS applies its own mask; no alpha channel")
    square_1024 = icon(1024, square=True)
    write(square_1024, OUT / "app-store" / "icon-1024.png", drop_alpha=True)
    write(square_1024, OUT / "mac-app-store" / "icon-1024.png", drop_alpha=True)

    print("Google Play — square for the same reason; feature graphic is required")
    write(icon(512, square=True), OUT / "google-play" / "icon-512.png")
    write(banner(1024, 500), OUT / "google-play" / "feature-graphic-1024x500.png")

    print("Microsoft Store — shown as-is, so it keeps its own corners")
    write(icon(300, square=False), OUT / "microsoft-store" / "store-logo-300.png")
    write(banner(1920, 1080), OUT / "microsoft-store" / "hero-1920x1080.png")
    write(banner(1440, 2160), OUT / "microsoft-store" / "poster-1440x2160.png")
    write(banner(2160, 2160), OUT / "microsoft-store" / "box-art-2160x2160.png")

    print("Shared — for a README, a release page or a sticker")
    write(icon(1024, square=False), OUT / "shared" / "icon-rounded-1024.png")
    write(banner(2400, 1200), OUT / "shared" / "banner-2400x1200.png")

    failures = verify()

    print(f"\nwritten to {OUT.relative_to(ROOT).as_posix()}/")
    print("screenshots are not generated; see brand/store/README.md")
    if failures:
        print(f"{failures} file(s) would be refused by a store. Not usable as they are.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
