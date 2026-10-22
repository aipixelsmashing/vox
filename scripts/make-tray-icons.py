#!/usr/bin/env python3
"""Draw the five tray icons as macOS template images (docs/UI-KIT.md, "Icon states").

A template image is read by macOS for its alpha channel only: opaque pixels are drawn in
the menu bar's own colour, transparent ones are not drawn. So every icon here is black on a
transparent background, and state is carried by silhouette alone. Each state is written at
1x (22x22) and 2x (44x44); src-tauri/src/tray.rs embeds the 2x files, which the menu bar
scales to its 18 pt row.

Run from the repository root; needs Pillow (`pip install pillow`):

    python3 scripts/make-tray-icons.py
"""

from pathlib import Path

from PIL import Image, ImageDraw

CANVAS = 22  # 1x canvas in points; the menu bar scales the whole canvas to 18 pt
SS = 16  # supersampling factor for anti-aliasing
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"


def canvas():
    img = Image.new("L", (CANVAS * SS, CANVAS * SS), 0)
    return img, ImageDraw.Draw(img)


def p(v):
    """Points on the 1x canvas to supersampled pixels."""
    return v * SS


def mark(draw, filled, cx=11.0, top=3.0, scale=1.0, ink=255):
    """The microphone: capsule head, cradle, stem, base. Centred on cx, head top at `top`."""
    s = scale
    head_w, head_h = 6.0 * s, 10.0 * s
    stroke = 1.5 * s
    x0, x1 = cx - head_w / 2, cx + head_w / 2
    y0, y1 = top, top + head_h
    if filled:
        draw.rounded_rectangle((p(x0), p(y0), p(x1), p(y1)), radius=p(head_w / 2), fill=ink)
    else:
        draw.rounded_rectangle(
            (p(x0), p(y0), p(x1), p(y1)), radius=p(head_w / 2), outline=ink, width=int(p(stroke))
        )
    # Cradle: the lower half of a circle wider than the head.
    cr = head_w / 2 + 2.5 * s
    cy = y1 - head_w / 2
    draw.arc(
        (p(cx - cr), p(cy - cr), p(cx + cr), p(cy + cr)),
        start=0,
        end=180,
        fill=ink,
        width=int(p(stroke)),
    )
    # Stem and base.
    stem_top = cy + cr
    base_y = stem_top + 2.5 * s
    draw.line((p(cx), p(stem_top), p(cx), p(base_y)), fill=ink, width=int(p(stroke)))
    draw.line(
        (p(cx - head_w / 2), p(base_y), p(cx + head_w / 2), p(base_y)),
        fill=ink,
        width=int(p(stroke)),
    )


def ring(draw, start=0, end=360, cx=11.0, cy=11.0, r=10.0, stroke=1.5):
    draw.arc(
        (p(cx - r), p(cy - r), p(cx + r), p(cy + r)), start=start, end=end, fill=255, width=int(p(stroke))
    )


def dot(draw, cx, cy, r, knockout):
    """A filled dot with a transparent gap around it so it reads as separate from the mark."""
    k = r + knockout
    draw.ellipse((p(cx - k), p(cy - k), p(cx + k), p(cy + k)), fill=0)
    draw.ellipse((p(cx - r), p(cy - r), p(cx + r), p(cy + r)), fill=255)


def idle():
    img, d = canvas()
    mark(d, filled=False)
    return img


def recording():
    # Filled mark inside a closed ring: the level ring, at rest.
    img, d = canvas()
    ring(d)
    mark(d, filled=True, top=5.0, scale=0.68)
    return img


def transcribing():
    # Filled mark inside an open arc: the rotating tick, one frame of it.
    img, d = canvas()
    ring(d, start=-60, end=210)
    mark(d, filled=True, top=5.0, scale=0.68)
    return img


def attention():
    # Outline mark with a corner badge, top right.
    img, d = canvas()
    mark(d, filled=False)
    dot(d, cx=17.5, cy=4.5, r=3.5, knockout=1.25)
    return img


def always_listening():
    # Outline mark with a persistent dot, bottom right: smaller and lower than the badge.
    img, d = canvas()
    mark(d, filled=False)
    dot(d, cx=18.0, cy=17.5, r=2.25, knockout=1.0)
    return img


def write(name, mask):
    for scale, suffix in ((1, ""), (2, "@2x")):
        size = CANVAS * scale
        # Lanczos leaves a faint halo of near-zero alpha around every edge; a template
        # image should be clean, so anything that faint is dropped.
        alpha = mask.resize((size, size), Image.LANCZOS).point(lambda v: 0 if v < 10 else v)
        out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        out.putalpha(alpha)
        path = OUT / f"tray-{name}{suffix}.png"
        out.save(path, optimize=True)
        print(path.relative_to(OUT.parent.parent), f"{size}x{size}")


if __name__ == "__main__":
    for name, fn in (
        ("idle", idle),
        ("recording", recording),
        ("transcribing", transcribing),
        ("attention", attention),
        ("always-listening", always_listening),
    ):
        write(name, fn())
