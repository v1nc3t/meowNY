#!/usr/bin/env python3
"""The meowNY logo: a gold coin shaped like a cat's head, with a green M on it, drawn on a 16 x 16 pixel grid.

    python3 scripts/logo.py
        writes ui/public/favicon.svg (the app and the website use it) and three 1024 px pictures in src-tauri/icons/
    npx tauri icon src-tauri/icons/logo.png -o /tmp/meowny-icons && cp /tmp/meowny-icons/*.* src-tauri/icons/
        then makes the desktop icons from the first picture (through another folder: given src-tauri/icons itself,
        `tauri icon` also writes this picture into a local phone project in src-tauri/gen/)
The phone icons are made in CI, after the Android and iOS projects are generated (.github/workflows/android.yml and
ios.yml): Android gets the coin alone on a clear background, iOS on white, as iOS always draws a solid square.

The grid below is the logo; change a letter to change a pixel. No dependencies."""

import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The app's own colours (ui/src/styles/tokens.css): the green of income, and the gold and dark gold of the amber accents.
COLOURS = {'O': '#b45309', 'Y': '#e8a317', 'G': '#2e8b57'}

GRID = """
................
..O..........O..
..OO........OO..
..OYO.OOOO.OYO..
..OYYOYYYYOYYO..
.OYYYYYYYYYYYYO.
.OYGGYYYYYYGGYO.
.OYGGGYYYYGGGYO.
.OYGGGGYYGGGGYO.
.OYGGYGGGGYGGYO.
.OYGGYYGGYYGGYO.
.OYGGYYYYYYGGYO.
..OYYYYYYYYYYO..
...OYYYYYYYYO...
....OOOOOOOO....
................
""".strip().split('\n')

SIZE = len(GRID)
assert all(len(row) == SIZE for row in GRID), 'the grid must be square'


def runs():
    """Each colour's pixels as runs along a row: (colour, x, y, length)."""
    for y, row in enumerate(GRID):
        x = 0
        while x < SIZE:
            if row[x] == '.':
                x += 1
                continue
            start = x
            while x < SIZE and row[x] == row[start]:
                x += 1
            yield row[start], start, y, x - start


def svg():
    paths = {}
    for colour, x, y, n in runs():
        paths.setdefault(colour, []).append(f'M{x} {y}h{n}v1h-{n}z')
    body = ''.join(f'<path fill="{COLOURS[c]}" d="{"".join(d)}"/>' for c, d in paths.items())
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {SIZE} {SIZE}" shape-rendering="crispEdges">{body}</svg>\n'


def png(scale, pad=0):
    """The grid as an RGBA PNG, each grid pixel a square of `scale` pixels, with `pad` clear pixels around it."""
    side = SIZE * scale + 2 * pad
    clear = b'\0\0\0\0'
    edge = b'\0' + clear * side
    lines = [edge] * pad
    for row in GRID:
        line = bytearray(clear * pad)
        for c in row:
            line += (bytes.fromhex(COLOURS[c][1:]) + b'\xff' if c != '.' else clear) * scale
        line += clear * pad
        lines += [b'\0' + bytes(line)] * scale
    lines += [edge] * pad

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', side, side, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b''.join(lines), 9)) + chunk(b'IEND', b''))


def write(relative, data):
    path = os.path.join(ROOT, relative)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data)
    print(f'logo: wrote {relative}')


if __name__ == '__main__':
    write('ui/public/favicon.svg', svg().encode())
    # All three are 1024 x 1024. The desktop one fills the picture, so the 32, 64 and 128 pixel icons stay sharp.
    write('src-tauri/icons/logo.png', png(64))
    # iOS rounds the corners off, so the ears need room.
    write('src-tauri/icons/logo-ios.png', png(56, 64))
    # Android cuts the icon to a shape inside the middle two thirds of the picture, often a circle.
    write('src-tauri/icons/logo-android.png', png(32, 256))
