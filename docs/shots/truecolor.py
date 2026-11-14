"""24-bit hue planes on stdout. Columns and rows are the pane's cell count."""

import sys


def hsv(h, s, v):
    i = int(h * 6.0) % 6
    f = h * 6.0 - int(h * 6.0)
    p = v * (1.0 - s)
    q = v * (1.0 - f * s)
    t = v * (1.0 - (1.0 - f) * s)
    r, g, b = (
        (v, t, p),
        (q, v, p),
        (p, v, t),
        (p, q, v),
        (t, p, v),
        (v, p, q),
    )[i]
    return round(r * 255), round(g * 255), round(b * 255)


def along(y, n):
    if n <= 1:
        return 0.0
    return y / (n - 1)


def plane(cols, height, sat, val):
    rows = []
    for y in range(height):
        s = sat(y, height)
        v = val(y, height)
        cells = []
        for x in range(cols):
            r, g, b = hsv(x / cols, s, v)
            cells.append(f"\x1b[48;2;{r};{g};{b}m ")
        rows.append("".join(cells) + "\x1b[0m")
    return rows


def main():
    cols = int(sys.argv[1]) if len(sys.argv) > 1 else 80
    rows = int(sys.argv[2]) if len(sys.argv) > 2 else 24
    if cols < 1 or rows < 4:
        sys.exit("truecolor.py COLS ROWS, with at least 4 rows")
    body = rows - 2
    top = body // 2
    bottom = body - top
    lines = ["hue × saturation"]
    lines += plane(cols, top, lambda y, n: 1.0 - along(y, n), lambda _y, _n: 1.0)
    lines += ["hue × value"]
    lines += plane(cols, bottom, lambda _y, _n: 1.0, lambda y, n: 1.0 - along(y, n))
    out = bytearray(b"\x1b[?25l\x1b]0;truecolor\x07")
    out += "\r\n".join(lines).encode()
    sys.stdout.buffer.write(out)


if __name__ == "__main__":
    main()
