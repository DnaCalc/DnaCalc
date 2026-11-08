"""Draws onecalc.ico, the application icon of dna-onecalc-desktop: two strands (Strand's teal and
amber) crossing on the petrol tile, in the sizes Windows asks for. Run: python make-icon.py
Needs Pillow. The colours are dna-strand's palette (PETROL_900, TEAL_400, AMBER)."""
import math
import os
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
PETROL = (0x12, 0x30, 0x3A, 255)
TEAL = (0x3D, 0xB6, 0xCF, 255)
AMBER = (0xFF, 0xB3, 0x01, 255)
RUNG = (0xED, 0xF1, 0xF2, 255)
BASE = 1024


def tile():
    image = Image.new("RGBA", (BASE, BASE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((40, 40, BASE - 40, BASE - 40), radius=210, fill=PETROL)
    left, right = 250, BASE - 250
    top, bottom = 170, BASE - 170
    steps = 400
    path_a, path_b = [], []
    for i in range(steps + 1):
        t = i / steps
        y = top + (bottom - top) * t
        swing = math.sin(t * 2 * math.pi * 1.5 + math.pi / 2)
        mid = (left + right) / 2
        half = (right - left) / 2
        path_a.append((mid + half * swing, y))
        path_b.append((mid - half * swing, y))
    # Rungs where the strands are apart, then the strands over them.
    for i in range(8, steps + 1, 33):
        (xa, ya), (xb, _) = path_a[i], path_b[i]
        if abs(xa - xb) > 220:
            draw.line((xa, ya, xb, ya), fill=RUNG, width=26)
    draw.line(path_b, fill=AMBER, width=64, joint="curve")
    draw.line(path_a, fill=TEAL, width=64, joint="curve")
    return image


def main():
    base = tile()
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [base.resize((s, s), Image.LANCZOS) for s in sizes]
    images[-1].save(
        os.path.join(HERE, "onecalc.ico"),
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=images[:-1],
    )


if __name__ == "__main__":
    main()
