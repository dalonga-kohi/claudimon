#!/usr/bin/env python3
"""Render a `claudimon --snapshot` cell dump (*.cells) to a PNG using a real
console font, so README screenshots look like the game in a terminal.

Usage: render_cells.py <in.cells> <out.png> [scale]

Fonts: Linux console PSF fonts from the `kbd` package (Terminus first, then the
default VGA font for symbols like ♥ ♪ ●), plus a few hand-drawn fallbacks.
Standard library only.
"""
import gzip
import struct
import sys
import zlib

FONT_DIRS = ["/usr/share/kbd/consolefonts", "/usr/share/consolefonts", "/lib/kbd/consolefonts"]
FONT_NAMES = ["Lat2-Terminus16.psfu.gz", "default8x16.psfu.gz", "lat2-16.psfu.gz"]


def load_psf(path):
    """Return ({char: [row bitmasks]}, width, height) for a PSF1/PSF2 font."""
    data = gzip.open(path).read() if path.endswith(".gz") else open(path, "rb").read()
    glyphs, table = [], {}
    if data[:2] == b"\x36\x04":  # PSF1
        mode, height = data[2], data[3]
        count = 512 if mode & 1 else 256
        width, size, off = 8, height, 4
        glyphs = [data[off + i * size: off + (i + 1) * size] for i in range(count)]
        pos = off + count * size
        if mode & 6:
            g = 0
            while g < count and pos + 1 < len(data):
                (u,) = struct.unpack_from("<H", data, pos)
                pos += 2
                if u == 0xFFFF:
                    g += 1
                elif u != 0xFFFE:
                    table.setdefault(chr(u), g)
    elif data[:4] == b"\x72\xb5\x4a\x86":  # PSF2
        _, hsize, flags, count, size, height, width = struct.unpack_from("<7I", data, 4)
        glyphs = [data[hsize + i * size: hsize + (i + 1) * size] for i in range(count)]
        pos = hsize + count * size
        if flags & 1:
            g, buf = 0, b""
            while g < count and pos < len(data):
                b = data[pos]
                pos += 1
                if b in (0xFF, 0xFE):
                    if buf:
                        for ch in buf.decode("utf-8", "ignore"):
                            table.setdefault(ch, g)
                    buf = b""
                    if b == 0xFF:
                        g += 1
                else:
                    buf += bytes([b])
    else:
        raise ValueError("not a PSF font: " + path)
    bpr = (width + 7) // 8
    font = {}
    for ch, g in table.items():
        rows = []
        raw = glyphs[g]
        for r in range(height):
            v = int.from_bytes(raw[r * bpr:(r + 1) * bpr], "big")
            rows.append(v >> (bpr * 8 - width))
        font[ch] = rows
    return font, width, height


# Hand-drawn 8x16 fallbacks for symbols the console fonts lack.
def glyph(*rows):
    rows = list(rows) + ["........"] * (16 - len(rows))
    return [int(r.replace(".", "0").replace("#", "1"), 2) for r in rows]


FALLBACK = {
    "✻": glyph("........", "........", "...#....", "#..#..#.", ".#.#.#..", "..###...", "#######.", "..###...", ".#.#.#..", "#..#..#.", "...#...."),
    "✦": glyph("........", "........", "...#....", "...#....", "..###...", "#######.", "..###...", "...#....", "...#...."),
    "★": glyph("........", "........", "...#....", "...#....", "..###...", "#######.", ".#####..", "..###...", ".##.##..", "##...##."),
    "✿": glyph("........", "........", "..#.#...", ".##.##..", "#.###.#.", ".##.##..", "#.###.#.", ".##.##..", "..#.#..."),
    "✉": glyph("........", "........", "........", "#######.", "##...##.", "#.#.#.#.", "#..#..#.", "#.....#.", "#######."),
    "❄": glyph("........", "........", "...#....", "#..#..#.", ".#.#.#..", "..###...", "#######.", "..###...", ".#.#.#..", "#..#..#.", "...#...."),
    "☂": glyph("........", "........", "...#....", ".#####..", "#######.", "#######.", "...#....", "...#....", "...#....", "#..#....", ".##....."),
    "☁": glyph("........", "........", "........", "........", "..##....", ".####.#.", "#######.", "#######.", ".#####.."),
    "☀": glyph("........", "........", "#..#..#.", ".#...#..", "..###...", "#.###.#.", "..###...", ".#...#..", "#..#..#."),
    "☾": glyph("........", "........", "..###...", ".##.....", "##......", "##......", "##......", ".##.....", "..###..."),
    "✓": glyph("........", "........", "........", "......#.", ".....##.", "#...##..", "##.##...", ".###....", "..#....."),
    "✹": glyph("........", "........", "#..#..#.", ".#.#.#..", "..###...", "#######.", "..###...", ".#.#.#..", "#..#..#."),
    "♛": glyph("........", "........", "#.#.#.#.", "#######.", "#######.", ".#####..", ".#####..", "#######."),
    "⊘": glyph("........", "........", "..###...", ".#..##..", "#..#..#.", "#.#...#.", ".##..#..", "..###..."),
    "↯": glyph("........", "........", "....##..", "...##...", "..##....", ".######.", "....##..", "...##...", "..##...."),
    "⚱": glyph("........", "........", ".#####..", "..###...", ".#####..", "#######.", "#######.", ".#####..", "..###..."),
    "♨": glyph("........", "........", ".#.#.#..", "#.#.#...", ".#.#.#..", "........", "#.....#.", ".#####.."),
    "⛰": glyph("........", "........", "........", "...#....", "..###...", ".#####..", "#######."),
    "↔": glyph("........", "........", "........", "..#..#..", ".#....#.", "########", ".#....#.", "..#..#.."),
    "♡": glyph("........", "........", ".##.##..", "#..#..#.", "#.....#.", ".#...#..", "..#.#...", "...#...."),
    "λ": glyph("........", "........", ".##.....", "...#....", "...#....", "..#.#...", ".#...#..", "#.....#."),
    "▬": glyph("........", "........", "........", "........", "........", "########", "########"),
}


# Close-enough substitutes the fonts do have.
ALIASES = {"╭": "┌", "╮": "┐", "╰": "└", "╯": "┘"}


def load_fonts():
    import os
    fonts = []
    for d in FONT_DIRS:
        for n in FONT_NAMES:
            p = os.path.join(d, n)
            if os.path.exists(p):
                try:
                    fonts.append(load_psf(p))
                except Exception as e:  # pragma: no cover
                    print("skipping font", p, e, file=sys.stderr)
    if not fonts:
        sys.exit("no console fonts found (install the `kbd` package)")
    return fonts


def write_png(path, w, h, rows):
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + bytes(r) for r in rows)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)


def main():
    src, dst = sys.argv[1], sys.argv[2]
    scale = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    fonts = load_fonts()
    fw, fh = 8, 16
    lines = open(src).read().split("\n")
    cols, nrows = map(int, lines[0].split())
    W, H = cols * fw, nrows * fh
    img = [bytearray(W * 3) for _ in range(H)]
    missing = set()

    def find(ch):
        ch = ALIASES.get(ch, ch)
        for f, w, h in fonts:
            if ch in f and w == fw and h == fh:
                return f[ch]
        if ch in FALLBACK:
            return FALLBACK[ch]
        missing.add(ch)
        return None

    for i in range(cols * nrows):
        r, c = divmod(i, cols)
        v = list(map(int, lines[1 + i].split()))
        top, bot = v[0:3], v[3:6]
        x0, y0 = c * fw, r * fh
        if len(v) > 6:
            ch, fg, bg = chr(v[6]), v[7:10], v[10:13]
            g = find(ch) if ch != " " else None
            for y in range(fh):
                row = img[y0 + y]
                bits = g[y] if g else 0
                for x in range(fw):
                    col = fg if bits & (1 << (fw - 1 - x)) else bg
                    row[(x0 + x) * 3:(x0 + x) * 3 + 3] = bytes(col)
        else:
            for y in range(fh):
                col = bytes(top if y < fh // 2 else bot) * fw
                img[y0 + y][x0 * 3:(x0 + fw) * 3] = col
    if missing:
        print("glyphs missing:", "".join(sorted(missing)), file=sys.stderr)
    if scale > 1:
        img = [bytearray(b"".join(bytes(row[x * 3:x * 3 + 3]) * scale for x in range(W))) for row in img for _ in range(scale)]
        W, H = W * scale, H * scale
    write_png(dst, W, H, img)


if __name__ == "__main__":
    main()
