"""Injected terminal content for the headless feature shots. ROWS is optional."""

import sys

mode = sys.argv[1] if len(sys.argv) > 1 else "text"
rows = int(sys.argv[2]) if len(sys.argv) > 2 else 30

if mode == "text":
    lines = [
        "\x1b[1;36mrune-term\x1b[0m — headless feature capture",
        "",
        "  \x1b[32m✓\x1b[0m the shell survives a relaunch with handover on",
        "  \x1b[32m✓\x1b[0m tabs, splits and scrollback keep their own grids",
        "  \x1b[33m→\x1b[0m drag across these rows to select them",
        "",
        "\x1b[35mCommands\x1b[0m",
        "  just shots      render every picture in docs/images",
        "  just check-all  fmt, clippy and the library tests",
        "  just run        build the keeper and open a window",
        "",
        "Select a phrase, then right-click the pane for its menu.",
    ]
elif mode == "link":
    url = "https://example.com/a/very/long/path/that/needs/a/tooltip"
    lines = [
        "\x1b[1;36mHyperlinks\x1b[0m",
        "",
        "  " + "\x1b]8;;" + url + "\x07" + url + "\x1b]8;;\x07",
        "",
        "  Hold Command over the address to reveal the whole URL.",
        "  Right-click a link to open or copy it.",
    ]
elif mode == "scroll":
    lines = [
        f"\x1b[1;{31 + i % 6}mline {i:03d}\x1b[0m  "
        + "the quick brown fox jumps over the lazy dog"
        for i in range(rows * 3)
    ]
else:
    sys.exit("features.py TEXT|LINK|SCROLL [COLS] [ROWS]")

out = "\r\n".join(lines)
sys.stdout.buffer.write(out.encode())
