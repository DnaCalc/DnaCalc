#!/usr/bin/env python3
"""Writes preset-shapes.txt (dc-zjsg.2) from presetShapeDefinitions.xml, the preset geometry definitions published
with ECMA-376 Part 1 (5th edition, December 2016: OfficeOpenXML-DrawingMLGeometries.zip in
https://ecma-international.org/wp-content/uploads/ECMA-376-1_5th_edition_december_2016.zip).
The definitions are the file format's own data (a published standard, tier 1 in docs/plan/RESEARCH_SOURCES.md); the
table keeps them line for line in a compact form the worker reads (custom_geometry.rs, `preset`):
  @name                 a preset
  a NAME FORMULA        an adjust value (avLst gd)
  g NAME FORMULA        a guide (gdLst gd)
  r L T R B             the text rectangle
  p W H FILL STROKE     a path (`-` where the file says nothing)
  M x y | L x y | A wR hR stAng swAng | Q x1 y1 x y | C x1 y1 x2 y2 x y | Z   its commands
Run: python3 -I make-presets.py presetShapeDefinitions.xml preset-shapes.txt"""
import sys
import xml.etree.ElementTree as ET

A = "{http://schemas.openxmlformats.org/drawingml/2006/main}"
root = ET.parse(sys.argv[1]).getroot()
out = ["# ECMA-376 Part 1 (5th edition, 2016) preset shape geometries, from presetShapeDefinitions.xml, (c) Ecma International;",
       "# published with the standard for implementations of it. Written by make-presets.py; do not edit by hand."]

def local(e):
    return e.tag.split("}")[-1]

seen = set()
for shape in root:
    # The 2016 file lists upDownArrow twice, identically: the table keeps the first.
    if local(shape) in seen:
        continue
    seen.add(local(shape))
    out.append("@" + local(shape))
    for part in shape:
        kind = local(part)
        if kind in ("avLst", "gdLst"):
            for gd in part:
                out.append(f'{"a" if kind == "avLst" else "g"} {gd.get("name")} {" ".join(gd.get("fmla").split())}')
        elif kind == "rect":
            out.append(f'r {part.get("l")} {part.get("t")} {part.get("r")} {part.get("b")}')
        elif kind == "pathLst":
            for path in part:
                stroke = path.get("stroke", "-")
                out.append(f'p {path.get("w", "-")} {path.get("h", "-")} {path.get("fill", "-")} {stroke}')
                for c in path:
                    name = local(c)
                    pts = " ".join(f'{p.get("x")} {p.get("y")}' for p in c if local(p) == "pt")
                    if name == "moveTo":
                        out.append("M " + pts)
                    elif name == "lnTo":
                        out.append("L " + pts)
                    elif name == "arcTo":
                        out.append(f'A {c.get("wR")} {c.get("hR")} {c.get("stAng")} {c.get("swAng")}')
                    elif name == "quadBezTo":
                        out.append("Q " + pts)
                    elif name == "cubicBezTo":
                        out.append("C " + pts)
                    elif name == "close":
                        out.append("Z")
                    else:
                        raise SystemExit(f"unknown path command {name}")
open(sys.argv[2], "w", newline="\n").write("\n".join(out) + "\n")
