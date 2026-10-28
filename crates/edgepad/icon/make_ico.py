"""Draw EdgePad's icon (edgepad.svg) at each size the exe carries, into
edgepad.ico, which build.rs puts in the exe (Issue #116).

    python crates/edgepad/icon/make_ico.py

Python's standard library only. The shapes are edgepad.svg's, on its 32 x
32 grid; 16px and 20px have their own, set on whole pixels, so their edges
stay sharp rather than shrunk to a blur. 256px is a PNG, the rest 32-bit
bitmaps with an alpha channel.
"""

import pathlib
import struct
import zlib

FRAME, PAPER, KEY, LATCHED = 0x2C2C2A, 0xFFFFFF, 0x378ADD, 0xEF9F27

# x, y, width, height, corner radius, colour: edgepad.svg, back to front.
SHAPES = [
    (2, 3, 28, 26, 5, FRAME),
    (12, 6, 15, 20, 2, PAPER),
    (4.5, 6, 5.5, 5.5, 1.5, KEY),
    (4.5, 13.25, 5.5, 5.5, 1.5, KEY),
    (4.5, 20.5, 5.5, 5.5, 1.5, LATCHED),
]

# The small sizes, in their own pixels: a 1px border all round the frame,
# keys and the paper 1px apart.
HINTED = {
    16: [
        (1, 2, 14, 13, 2, FRAME),
        (6, 3, 8, 11, 0.75, PAPER),
        (2, 3, 3, 3, 0.75, KEY),
        (2, 7, 3, 3, 0.75, KEY),
        (2, 11, 3, 3, 0.75, LATCHED),
    ],
    20: [
        (1, 2, 18, 16, 2.5, FRAME),
        (7, 3, 11, 14, 1, PAPER),
        (2, 3, 4, 4, 1, KEY),
        (2, 8, 4, 4, 1, KEY),
        (2, 13, 4, 4, 1, LATCHED),
    ],
}

SIZES = [16, 20, 24, 32, 40, 48, 256]


def inside(px, py, shape):
    x, y, w, h, r, _ = shape
    if not (x <= px < x + w and y <= py < y + h):
        return False
    cx = min(max(px, x + r), x + w - r)
    cy = min(max(py, y + r), y + h - r)
    return (px - cx) ** 2 + (py - cy) ** 2 <= r * r


def draw(size):
    """The icon at `size`: RGBA rows, top down, not premultiplied."""
    if size in HINTED:
        shapes, scale = HINTED[size], 1
    else:
        shapes, scale = SHAPES, size / 32
    n = 16 if size <= 48 else 4
    rows = []
    for py in range(size):
        row = []
        for px in range(size):
            r = g = b = covered = 0
            for sy in range(n):
                for sx in range(n):
                    at = ((px + (sx + 0.5) / n) / scale, (py + (sy + 0.5) / n) / scale)
                    for shape in reversed(shapes):
                        if inside(*at, shape):
                            colour = shape[5]
                            r, g, b = r + (colour >> 16), g + (colour >> 8 & 0xFF), b + (colour & 0xFF)
                            covered += 1
                            break
            if covered:
                row.append((round(r / covered), round(g / covered), round(b / covered), round(255 * covered / (n * n))))
            else:
                row.append((0, 0, 0, 0))
        rows.append(row)
    return rows


def bitmap(size, rows):
    """An icon image as a 32-bit DIB: bottom-up colours, then the mask."""
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    colours = b"".join(bytes((b, g, r, a)) for row in reversed(rows) for r, g, b, a in row)
    stride = (size + 31) // 32 * 4
    mask = b""
    for row in reversed(rows):
        bits = bytearray(stride)
        for x, (_, _, _, a) in enumerate(row):
            if a == 0:
                bits[x // 8] |= 0x80 >> (x % 8)
        mask += bytes(bits)
    return header + colours + mask


def png(size, rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + bytes(c for pixel in row for c in pixel) for row in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main():
    images = []
    for size in SIZES:
        rows = draw(size)
        images.append((size, png(size, rows) if size >= 256 else bitmap(size, rows)))
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in images:
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out += b"".join(data for _, data in images)
    path = pathlib.Path(__file__).with_name("edgepad.ico")
    path.write_bytes(out)
    print(f"{path}: {len(out)} bytes")


if __name__ == "__main__":
    main()
