#!/usr/bin/env python3
"""Writes the images of pictures-paint.xlsx (own work, dc-zjsg.3) into the folder given: a 160 x 100 test card
(four coloured quarters, a white border, a diagonal and an arrow that points right, so a crop, a turn or a flip
shows) as PNG, JPEG, GIF and BMP; the same card as SVG with its PNG fallback; an EMF and a WMF with no bitmap in
them (the placeholder case) and an EMF carrying the PNG (its fallback case).
Run: python3 -I make-pictures.py pictures"""
import io, struct, sys
from PIL import Image, ImageDraw

def card(w=160, h=100):
    im = Image.new("RGB", (w, h), "white")
    d = ImageDraw.Draw(im)
    d.rectangle([0, 0, w // 2, h // 2], fill=(220, 50, 50))
    d.rectangle([w // 2, 0, w, h // 2], fill=(60, 160, 60))
    d.rectangle([0, h // 2, w // 2, h], fill=(50, 90, 200))
    d.rectangle([w // 2, h // 2, w, h], fill=(240, 200, 40))
    d.line([0, 0, w, h], fill="black", width=2)
    d.rectangle([0, 0, w - 1, h - 1], outline="white", width=3)
    d.polygon([(w // 2 - 30, h // 2 - 8), (w // 2 + 10, h // 2 - 8), (w // 2 + 10, h // 2 - 20), (w // 2 + 36, h // 2), (w // 2 + 10, h // 2 + 20), (w // 2 + 10, h // 2 + 8), (w // 2 - 30, h // 2 + 8)], fill="black")
    return im

def png_bytes(im):
    b = io.BytesIO(); im.save(b, "PNG", optimize=True); return b.getvalue()

def emf(width_mm100, height_mm100, tail=b""):
    header = bytearray(88)
    struct.pack_into("<II", header, 0, 1, 88)
    struct.pack_into("<iiii", header, 8, 0, 0, 159, 99)
    struct.pack_into("<iiii", header, 24, 0, 0, width_mm100, height_mm100)
    header[40:44] = b" EMF"
    struct.pack_into("<I", header, 44, 0x10000)
    end = struct.pack("<III", 14, 20, 0) + struct.pack("<II", 0, 20)
    return bytes(header) + tail + end

def wmf():
    body = struct.pack("<HHHIHHI", 1, 9, 0x300, 12, 0, 0, 0)
    head = struct.pack("<IHhhhhHIH", 0x9AC6CDD7, 0, 0, 0, 1440, 720, 1440, 0, 0)
    head += struct.pack("<H", 0)
    return head + body

out = sys.argv[1]
im = card()
png = png_bytes(im)
open(f"{out}/card.png", "wb").write(png)
im.save(f"{out}/card.jpg", "JPEG", quality=90)
im.convert("P", palette=Image.ADAPTIVE).save(f"{out}/card.gif", "GIF")
im.save(f"{out}/card.bmp", "BMP")
open(f"{out}/card.svg", "w").write('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100" viewBox="0 0 160 100">'
  '<rect width="80" height="50" fill="#dc3232"/><rect x="80" width="80" height="50" fill="#3ca03c"/>'
  '<rect y="50" width="80" height="50" fill="#325ac8"/><rect x="80" y="50" width="80" height="50" fill="#f0c828"/>'
  '<path d="M0 0L160 100" stroke="black" stroke-width="2"/><path d="M50 42H90V30L116 50L90 70V58H50Z"/></svg>\n')
open(f"{out}/bare.emf", "wb").write(emf(4233, 2646))
open(f"{out}/withpng.emf", "wb").write(emf(4233, 2646, png))
open(f"{out}/bare.wmf", "wb").write(wmf())
