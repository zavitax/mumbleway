"""Build a multi-size Windows .ico from PNG sources.

flutter_launcher_icons emits a single 256x256 PNG-compressed entry. Windows can
scale that, but the small sizes it actually asks for -- 16px in the title bar
and Explorer's details view, 32px on the taskbar -- come out soft, and some
shell paths skip a PNG-only icon altogether and fall back to the generic
executable glyph.

So: real BMP entries for everything up to 64px, where Windows wants them, and
PNG for the two large sizes, where the format expects it.

Pure standard library on purpose. Pillow is not installed and this runs on a
developer machine, so it decodes PNG itself: zlib inflate plus the five
per-scanline filters from the spec.
"""

import struct
import sys
import zlib


def decode_png(path):
    """Returns (width, height, RGBA bytes)."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")

    pos, idat, width, height, depth, colour = 8, bytearray(), 0, 0, 0, 0
    palette, trns = b"", b""
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
        elif kind == b"PLTE":
            palette = body
        elif kind == b"tRNS":
            trns = body
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
        pos += 12 + length

    if depth != 8:
        raise ValueError(f"{path}: only 8-bit channels supported, got {depth}")

    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[colour]
    raw = zlib.decompress(bytes(idat))
    stride = width * channels

    # Undo the per-scanline filters. Each row is prefixed with its filter type
    # and predicts from the pixel to the left (a), above (b), and up-left (c).
    out = bytearray()
    prev = bytearray(stride)
    pos = 0
    for _ in range(height):
        ftype = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            x = line[i]
            if ftype == 0:
                v = x
            elif ftype == 1:
                v = x + a
            elif ftype == 2:
                v = x + b
            elif ftype == 3:
                v = x + (a + b) // 2
            elif ftype == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                v = x + (a if pa <= pb and pa <= pc else (b if pb <= pc else c))
            else:
                raise ValueError(f"{path}: bad filter {ftype}")
            line[i] = v & 0xFF
        out += line
        prev = line

    # Normalise everything to RGBA.
    rgba = bytearray(width * height * 4)
    for i in range(width * height):
        px = out[i * channels : (i + 1) * channels]
        if colour == 6:
            rgba[i * 4 : i * 4 + 4] = px
        elif colour == 2:
            rgba[i * 4 : i * 4 + 3] = px
            rgba[i * 4 + 3] = 255
        elif colour == 0:
            rgba[i * 4 : i * 4 + 3] = bytes([px[0]] * 3)
            rgba[i * 4 + 3] = 255
        elif colour == 4:
            rgba[i * 4 : i * 4 + 3] = bytes([px[0]] * 3)
            rgba[i * 4 + 3] = px[1]
        elif colour == 3:
            idx = px[0]
            rgba[i * 4 : i * 4 + 3] = palette[idx * 3 : idx * 3 + 3]
            rgba[i * 4 + 3] = trns[idx] if idx < len(trns) else 255
    return width, height, bytes(rgba)


def bmp_entry(width, height, rgba):
    """A DIB with doubled height: colour rows, then the legacy AND mask."""
    header = struct.pack(
        "<IiiHHIIiiII", 40, width, height * 2, 1, 32, 0, 0, 0, 0, 0, 0
    )
    pixels = bytearray()
    for y in range(height - 1, -1, -1):  # DIB rows run bottom-up
        row = rgba[y * width * 4 : (y + 1) * width * 4]
        for x in range(width):
            r, g, b, a = row[x * 4 : x * 4 + 4]
            pixels += bytes((b, g, r, a))

    # Fully transparent pixels are masked out as well as being alpha 0, which
    # is what pre-alpha shell paths read.
    mask_stride = ((width + 31) // 32) * 4
    mask = bytearray()
    for y in range(height - 1, -1, -1):
        bits = bytearray(mask_stride)
        for x in range(width):
            if rgba[(y * width + x) * 4 + 3] == 0:
                bits[x // 8] |= 0x80 >> (x % 8)
        mask += bits
    return header + bytes(pixels) + bytes(mask)


def main(out_path, sources):
    images = []
    for size, path in sources:
        width, height, rgba = decode_png(path)
        if (width, height) != (size, size):
            raise ValueError(f"{path} is {width}x{height}, expected {size}")
        if size >= 128:
            images.append((size, open(path, "rb").read()))  # PNG-compressed
        else:
            images.append((size, bmp_entry(width, height, rgba)))

    offset = 6 + 16 * len(images)
    directory, blobs = bytearray(), bytearray()
    for size, blob in images:
        directory += struct.pack(
            "<BBBBHHII",
            0 if size == 256 else size,
            0 if size == 256 else size,
            0,
            0,
            1,
            32,
            len(blob),
            offset,
        )
        blobs += blob
        offset += len(blob)

    with open(out_path, "wb") as f:
        f.write(struct.pack("<HHH", 0, 1, len(images)))
        f.write(directory)
        f.write(blobs)
    print(f"wrote {out_path}: {len(images)} images, {offset} bytes")


if __name__ == "__main__":
    tmp = sys.argv[1]
    out = sys.argv[2]
    main(out, [(s, f"{tmp}/{s}.png") for s in (16, 24, 32, 48, 64, 128, 256)])
