#!/usr/bin/env python3
"""Convert an ANSI-coloured terminal capture (tmux capture-pane -e) to HTML.

Usage: ansi2html.py capture.ans > capture.html
Handles 24-bit, 256 and 16 colour SGR codes plus bold/underline/reverse.
"""
import html
import re
import sys

BASIC = ["#000000", "#aa0000", "#00aa00", "#aa5500", "#0000aa", "#aa00aa", "#00aaaa", "#aaaaaa",
         "#555555", "#ff5555", "#55ff55", "#ffff55", "#5555ff", "#ff55ff", "#55ffff", "#ffffff"]


def xterm(n):
    if n < 16:
        return BASIC[n]
    if n < 232:
        n -= 16
        lv = [0, 95, 135, 175, 215, 255]
        return "#%02x%02x%02x" % (lv[n // 36], lv[(n // 6) % 6], lv[n % 6])
    g = 8 + (n - 232) * 10
    return "#%02x%02x%02x" % (g, g, g)


def parse(params, st):
    p = [int(x) if x else 0 for x in params.split(";")] if params else [0]
    i = 0
    while i < len(p):
        c = p[i]
        if c == 0:
            st.update(fg=None, bg=None, bold=False, ul=False, rev=False)
        elif c == 1:
            st["bold"] = True
        elif c == 4:
            st["ul"] = True
        elif c == 7:
            st["rev"] = True
        elif c in (22,):
            st["bold"] = False
        elif c == 24:
            st["ul"] = False
        elif c == 27:
            st["rev"] = False
        elif 30 <= c <= 37:
            st["fg"] = BASIC[c - 30]
        elif 90 <= c <= 97:
            st["fg"] = BASIC[c - 90 + 8]
        elif 40 <= c <= 47:
            st["bg"] = BASIC[c - 40]
        elif 100 <= c <= 107:
            st["bg"] = BASIC[c - 100 + 8]
        elif c == 39:
            st["fg"] = None
        elif c == 49:
            st["bg"] = None
        elif c in (38, 48) and i + 1 < len(p):
            key = "fg" if c == 38 else "bg"
            if p[i + 1] == 2 and i + 4 < len(p):
                st[key] = "#%02x%02x%02x" % (p[i + 2], p[i + 3], p[i + 4])
                i += 4
            elif p[i + 1] == 5 and i + 2 < len(p):
                st[key] = xterm(p[i + 2])
                i += 2
        i += 1


def main():
    text = open(sys.argv[1], encoding="utf-8").read()
    st = dict(fg=None, bg=None, bold=False, ul=False, rev=False)
    out = []
    for line in text.split("\n"):
        parts = re.split(r"\x1b\[([0-9;]*)m", line)
        row = []
        for idx, part in enumerate(parts):
            if idx % 2 == 1:
                parse(part, st)
                continue
            if not part:
                continue
            fg, bg = st["fg"] or "#c8c0b0", st["bg"] or "#0b0a09"
            if st["rev"]:
                fg, bg = bg, fg
            style = f"color:{fg};background:{bg}"
            if st["bold"]:
                style += ";font-weight:bold"
            if st["ul"]:
                style += ";text-decoration:underline"
            row.append(f'<span style="{style}">{html.escape(part)}</span>')
        out.append("".join(row))
    print('<!doctype html><meta charset="utf-8"><body style="margin:0;background:#0b0a09">'
          '<pre style="font:15px/1.15 \'DejaVu Sans Mono\',monospace;margin:12px">' + "\n".join(out) + "</pre></body>")


if __name__ == "__main__":
    main()
