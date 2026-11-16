#!/usr/bin/env python3
"""Own work (dc-8ari6). Splits a saved drawing fixture so Excel can be asked which anchors it rejects.
For shapes-paintN.xlsx it writes, into the output directory:
  N-one-KK.xlsx      only anchor KK (1-based) of the drawing part
  N-first-KK.xlsx    anchors 1..KK
  N-from-KK.xlsx     anchors KK..last
  N-full-deflate / N-full-stored   all anchors, Python deflate / stored (is it the writer's compression?)
  N-full-padded      5 anchors after a 70000-byte comment (is it the part size past 64 KiB?)
Everything else in the package (sheet, relationships, media) is copied unchanged.
Run: python3 -I make-bisect.py shapes-paint2.xlsx 2 OUTDIR   (then shapes-paint3.xlsx 3 OUTDIR)"""
import sys, re, zipfile, os

src, tag, out = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(out, exist_ok=True)
zin = zipfile.ZipFile(src)
PART = "xl/drawings/dnaDrawing.xml"
xml = zin.read(PART).decode("utf-8")
m = re.match(r"(.*?<xdr:wsDr[^>]*>)(.*)(</xdr:wsDr>\s*)$", xml, re.S)
head, body, tail = m.groups()
anchors = re.findall(r"<xdr:(?:twoCellAnchor|oneCellAnchor|absoluteAnchor)\b.*?</xdr:(?:twoCellAnchor|oneCellAnchor|absoluteAnchor)>", body, re.S)
n = len(anchors)
assert n > 0

def write(name, picked, method=zipfile.ZIP_DEFLATED):
    with zipfile.ZipFile(os.path.join(out, name), "w", method) as z:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename == PART:
                data = (head + "".join(picked) + tail).encode("utf-8")
            z.writestr(info.filename, data)

for k in range(1, n + 1):
    write(f"{tag}-one-{k:02d}.xlsx", [anchors[k - 1]])
for k in (20, 30, 36, 40, 41, 42, 43, 44, 50):
    if k <= n:
        write(f"{tag}-first-{k:02d}.xlsx", anchors[:k])
        write(f"{tag}-from-{k:02d}.xlsx", anchors[k - 1:])
write(f"{tag}-full-deflate.xlsx", anchors)
write(f"{tag}-full-stored.xlsx", anchors, zipfile.ZIP_STORED)
write(f"{tag}-full-padded.xlsx", ["<!-- " + "x" * 70000 + " -->"] + anchors[:5])
print(f"{src}: {n} anchors")
