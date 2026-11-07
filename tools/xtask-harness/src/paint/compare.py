"""Report cell painting parity from original rasters; pixel differences are a coarse guard."""
import base64
import copy
import json
import hashlib
import sys
from pathlib import Path
from PIL import Image, ImageChops, ImageStat


def changed(a, b, tolerance=0):
    diff = ImageChops.difference(a, b)
    bands = diff.split()
    maximum = ImageChops.lighter(ImageChops.lighter(bands[0], bands[1]), bands[2])
    hist = maximum.histogram()
    count = sum(hist[tolerance + 1:])
    return {"pixels": a.width * a.height, "changed": count,
            "percent": 100 * count / (a.width * a.height),
            "mean_channel_error": sum(ImageStat.Stat(diff).mean) / 3}


def crop(image, rect):
    x, y, w, h = (round(rect[k]) for k in ("x", "y", "w", "h"))
    if w <= 0 or h <= 0 or x < 0 or y < 0 or x + w > image.width or y + h > image.height:
        return None
    return image.crop((x, y, x + w, y + h))


def dark_ink(image):
    """Diagnostic bounds only, excluding the outer gridline. Pixel metrics use every pixel."""
    if image.width < 3 or image.height < 3:
        return None
    mask = image.convert("L").point(lambda p: 255 if p < 96 else 0)
    box = mask.crop((1, 1, image.width - 1, image.height - 1)).getbbox()
    return [n + 1 for n in box] if box else None


def calibrate(reference, image):
    """Find physical grid edges in the captured raster; integral-point API values are estimates.

    Projection averages are only used to locate lines. All comparisons retain original pixels.
    Blank columns to the right of the requested range expose row edges without text or fills.
    Blank rows below it expose column edges; a tall projection is used if the range is clipped.
    """
    ref = copy.deepcopy(reference)
    cells = {(c["row"], c["col"]): c for c in ref["cells"]}
    row_count = max(r for r, _ in cells)
    col_count = max(c for _, c in cells)
    factor = ref["dpi"] / 72 * ref["zoom"] / 100
    x0, y0 = ref["origin"]["x"], ref["origin"]["y"]
    bounds = ref["sheet_bounds"]
    right, bottom = bounds["x"] + bounds["w"], bounds["y"] + bounds["h"]
    data_right = x0 + sum(cells[1, c]["points"]["w"] * factor for c in range(1, col_count + 1))
    data_bottom = y0 + sum(cells[r, 1]["points"]["h"] * factor for r in range(1, row_count + 1))
    row_strip = (min(round(data_right) + 16, right - 100), max(bounds["y"], y0 - 3), right - 4, bottom - 4)

    def candidates(rect, horizontal):
        mask = image.crop(rect).convert("L").point(lambda p: 255 if p < 245 else 0)
        projection = mask.resize((1, mask.height) if horizontal else (mask.width, 1), Image.Resampling.BOX)
        values = projection.tobytes()
        groups = []
        for i, value in enumerate(values):
            if value >= 191:
                if groups and i == groups[-1][1] + 1:
                    groups[-1][1] = i
                else:
                    groups.append([i, i])
        offset = rect[1 if horizontal else 0]
        return [offset + (a + b) // 2 + 1 for a, b in groups], [(offset + a, offset + b) for a, b in groups]

    if x0 < bounds["x"] or y0 < bounds["y"]:
        # Window coordinates refer to the scrolling pane when frozen. Locate the retained
        # header strips in this same screenshot; never borrow another workbook's origin.
        _, bands = candidates((min(bounds["x"] + round(data_right - x0) + 16, right - 100), bounds["y"], right - 4, bottom - 4), True)
        header = next(((a, b) for a, b in bands if a <= bounds["y"] + 2 and b - a >= 4), None)
        if header is None:
            raise ValueError(f"Cannot locate frozen header: {ref['prefix']}")
        y0 = header[1] + 1
        _, bands = candidates((bounds["x"], min(y0 + round(data_bottom - ref["origin"]["y"]) + 8, bottom - 80), right - 4, bottom - 4), False)
        header = next(((a, b) for a, b in bands if a <= bounds["x"] + 2 and b - a >= 4), None)
        if header is None:
            raise ValueError(f"Cannot locate frozen row headers: {ref['prefix']}")
        x0 = header[1] + 1
        data_right = x0 + sum(cells[1, c]["points"]["w"] * factor for c in range(1, col_count + 1))
        data_bottom = y0 + sum(cells[r, 1]["points"]["h"] * factor for r in range(1, row_count + 1))
        row_strip = (min(round(data_right) + 16, right - 100), max(bounds["y"], y0 - 3), right - 4, bottom - 4)

    ys, horizontal_bands = candidates(row_strip, True)
    # EXCEL7 includes the sheet tabs/scroll bar. Their solid bottom band is not sheet pixels.
    for start, end in horizontal_bands:
        if end >= bottom - 6 and end - start >= 4:
            bottom = start
            ys = [y for y in ys if y < bottom]
            break

    def edges(count, origin, values, sizes, limit):
        found = {}
        estimate = origin
        for i in range(1, count + 2):
            if i == 1:
                if not any(abs(p - origin) <= 1 for p in values):
                    raise ValueError(f"Cannot verify the captured A1 origin: {ref['prefix']}")
                found[i] = origin
                continue
            if sizes[i - 2] == 0:
                found[i] = found[i - 1]
                continue
            if i > 1:
                estimate = found[i - 1] + sizes[i - 2] * factor
            if estimate > limit - 2:
                break
            possible = [p for p in values if abs(p - estimate) <= max(1.5, ref["dpi"] / 96)
                        and (i == 1 or p > found[i - 1])]
            if not possible:
                if estimate > limit - 48 * ref["dpi"] / 96:
                    break
                raise ValueError(f"Cannot calibrate grid edge {i} at {estimate}: {ref['prefix']}")
            found[i] = min(possible, key=lambda p: abs(p - estimate))
        return found

    ye = edges(row_count, y0, ys, [cells[r, 1]["points"]["h"] for r in range(1, row_count + 1)], bottom)
    strips = []
    if data_bottom + 24 < bottom:
        strips.append((max(bounds["x"], x0 - 3), round(data_bottom) + 8, right - 4, bottom - 4))
    for row in range(1, row_count + 1):
        if row + 1 in ye and ye[row + 1] - ye[row] >= 8 and all(not cells[row, c]["text"] for c in range(1, col_count + 1)):
            top, end = ye[row], ye[row + 1]
            strips.append((max(bounds["x"], x0 - 3), top + max(2, (end - top) // 4), right - 4, end - max(2, (end - top) // 4)))
    strips.append((max(bounds["x"], x0 - 3), y0 + 4, right - 4, bottom - 4))
    xe = None
    last_error = None
    for col_strip in strips:
        xs, _ = candidates(col_strip, False)
        try:
            xe = edges(col_count, x0, xs, [cells[1, c]["points"]["w"] for c in range(1, col_count + 1)], right)
            break
        except ValueError as error:
            last_error = error
    if xe is None:
        raise last_error
    for (row, col), cell in cells.items():
        cell["raster_calibrated"] = row + 1 in ye and col + 1 in xe
        if cell["raster_calibrated"]:
            cell.update(x=xe[col], y=ye[row], w=xe[col + 1] - xe[col], h=ye[row + 1] - ye[row])
    ref["origin"] = {"x": xe[1], "y": ye[1]}
    ref["sheet_bounds"]["h"] = bottom - bounds["y"]
    ref["coordinate_method"] = "captured raster gridline projections; point geometry predicts adjacent edges"
    ref["raster_axes"] = {"x": xe, "y": ye, "column_strip": col_strip, "row_strip": row_strip}
    return ref


"""Cell observations from original rasters, independent of antialiasing identity."""
from collections import Counter
import numpy as np


METRICS = ("geometry", "display_text", "text_ink_box", "baseline", "alignment",
           "clipping_overflow", "line_breaks", "line_position", "line_thickness",
           "line_joins", "line_dashes", "fill_colour_class", "fill_pattern",
           "text_colour_class", "selection")


def workbook_cells(path, sheet_name):
    """Expected styles and occupied neighbours come from the hashed input, not the painter."""
    import zipfile
    import xml.etree.ElementTree as ET
    ns = {"s": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
    def local(tag):
        return tag.rsplit("}", 1)[-1]
    def coord(address):
        import re
        letters, row = re.fullmatch(r"\$?([A-Z]+)\$?(\d+)", address).groups()
        col = 0
        for letter in letters:
            col = col * 26 + ord(letter) - 64
        return int(row), col
    def rgb(node, default=None):
        if node is not None and "rgb" in node.attrib:
            return "#" + node.attrib["rgb"][-6:]
        return default
    with zipfile.ZipFile(path) as z:
        book = ET.fromstring(z.read("xl/workbook.xml"))
        rels = {r.attrib["Id"]: r.attrib["Target"] for r in ET.fromstring(z.read("xl/_rels/workbook.xml.rels"))}
        sheet = next(s for s in book.find("s:sheets", ns) if s.attrib["name"] == sheet_name)
        target = rels[sheet.attrib["{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"]]
        import posixpath
        sheet_path = target.lstrip("/") if target.startswith("/") else posixpath.normpath("xl/" + target)
        sheet = ET.fromstring(z.read(sheet_path))
        styles = ET.fromstring(z.read("xl/styles.xml"))
    fonts, fills, borders, xfs = [list(styles.find("s:" + name, ns)) for name in ("fonts", "fills", "borders", "cellXfs")]
    parsed = []
    for xf in xfs:
        font = fonts[int(xf.get("fontId", "0"))]
        alignment = xf.find("s:alignment", ns)
        align = alignment.attrib if alignment is not None else {}
        fill = fills[int(xf.get("fillId", "0"))]
        pattern = fill.find("s:patternFill", ns)
        border = borders[int(xf.get("borderId", "0"))]
        edges = []
        for side in ("left", "right", "top", "bottom"):
            edge = border.find("s:" + side, ns)
            edges.append({"style": edge.get("style"), "color": rgb(edge.find("s:color", ns), "#000000")} if edge is not None and edge.get("style") else None)
        parsed.append({"font_color": rgb(font.find("s:color", ns), "#000000"),
                       "pixels": float(font.find("s:sz", ns).get("val")) * 96 / 72,
                       "family": font.find("s:name", ns).get("val"),
                       "underline": font.find("s:u", ns) is not None, "strike": font.find("s:strike", ns) is not None,
                       "rotation": int(align.get("textRotation", "0")), "wrap": align.get("wrapText") == "1",
                       "horizontal": align.get("horizontal"), "vertical": align.get("vertical"),
                       "pattern": pattern.get("patternType") if pattern is not None else "gradient",
                       "fill": rgb(pattern.find("s:fgColor", ns)) if pattern is not None else None,
                       "borders": edges})
    cells = {}
    for node in sheet.findall("s:sheetData/s:row/s:c", ns):
        at = coord(node.get("r"))
        cells[at] = {"style": parsed[int(node.get("s", "0"))], "occupied": node.find("s:v", ns) is not None or node.find("s:is", ns) is not None or node.find("s:f", ns) is not None}
    selections = []
    for selection in sheet.findall("s:sheetViews/s:sheetView/s:selection", ns):
        for area in selection.get("sqref", "A1").split():
            ends = area.split(":")
            selections.append((*coord(ends[0]), *coord(ends[-1])))
    return {"cells": cells, "default_style": parsed[0], "selections": selections,
            "merges": [(*coord(n.get("ref").split(":")[0]), *coord(n.get("ref").split(":")[-1])) for n in sheet.findall("s:mergeCells/s:mergeCell", ns)]}


def check(ok=None, **observations):
    return {"status": "unmeasured" if ok is None else "pass" if ok else "fail", **observations}


def colour_class(rgb):
    r, g, b = [int(n) for n in rgb]
    hi, lo = max(r, g, b), min(r, g, b)
    if hi < 96:
        return "black"
    if hi - lo < 32:
        return "black" if hi < 96 else "white" if lo > 224 else "grey"
    if r > g * 1.25 and r > b * 1.25:
        return "red"
    if g > r * 1.2 and g > b * 1.2:
        return "green"
    if b > r * 1.2 and b > g * 1.15:
        return "blue"
    if min(r, g) > b * 1.3:
        return "yellow"
    if min(g, b) > r * 1.3:
        return "cyan"
    if min(r, b) > g * 1.3:
        return "magenta"
    return "mixed"


def colour_mask(pixels, rgb, tolerance=72):
    # Interior ink, not antialiasing fringes. Distance also distinguishes dark fills.
    delta = np.abs(pixels.astype(np.int16) - np.array(rgb, dtype=np.int16))
    mask = np.max(delta, axis=2) <= tolerance
    # A grey antialiased letter is not a blue border, even when RGB distances overlap.
    hi, lo = max(rgb), min(rgb)
    if hi - lo >= 32:
        high, low = rgb.index(hi), rgb.index(lo)
        mask &= pixels[:, :, high].astype(np.int16) - pixels[:, :, low].astype(np.int16) >= 24
    return mask


def rgb_value(value):
    value = (value or "#000000").lstrip("#")
    return tuple(int(value[i:i + 2], 16) for i in (0, 2, 4))


def runs(values, complete=False):
    groups = []
    for index, value in enumerate(values):
        if value:
            if groups and groups[-1][1] == index:
                groups[-1][1] = index + 1
            else:
                groups.append([index, index + 1])
    # Only runs touching the sampled strip's limits are censored. A first/last
    # gap between two ink runs is complete and must remain in the sample.
    return [r for r in groups if r[0] > 0 and r[1] < len(values)] if complete else groups


def text_observation(image, foreground, pixels=15):
    mask = colour_mask(np.asarray(image), foreground)
    h, w = mask.shape
    # Remove straight border bands only near the perimeter. Do not crop away overhang.
    for y in range(h):
        if (y < 4 or y >= h - 4) and np.count_nonzero(mask[y]) >= max(3, .7 * w):
            mask[y] = False
    for x in range(w):
        if (x < 4 or x >= w - 4) and np.count_nonzero(mask[:, x]) >= max(3, .7 * h):
            mask[:, x] = False
    ys, xs = np.nonzero(mask)
    box = [int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1] if len(xs) else None
    bands = []
    for start, end in runs(np.any(mask, axis=1)):
        if bands and start - bands[-1][1] <= max(1, round(pixels * .15)):
            bands[-1][1] = end
        else:
            bands.append([start, end])
    # A full blank raster row separates visible lines. Dots may make extra bands; report them.
    return {"box": box, "line_bands": bands, "ink_pixels": int(mask.sum())}, mask


def border_observation(image, rect, side, colour, radius=5):
    x, y, w, h = [round(rect[k]) for k in ("x", "y", "w", "h")]
    vertical = side in (0, 1)
    edge = (x if side == 0 else x + w) if vertical else (y if side == 2 else y + h)
    # Avoid joins while measuring the pen. Joins are measured separately.
    start, end = ((y + 5, y + h - 5) if vertical else (x + 5, x + w - 5))
    box = (edge - radius, start, edge + radius + 1, end) if vertical else (start, edge - radius, end, edge + radius + 1)
    if end <= start or min(box) < 0 or box[2] > image.width or box[3] > image.height:
        return None
    # Solid pen colour is separate from the much wider ClearType fringe tolerance
    # used for text. Nearby black letters otherwise become extra blue pen bands.
    mask = colour_mask(np.asarray(image.crop(box)), colour, tolerance=16)
    if vertical:
        mask = mask.T
    # Axis 0 is the normal distance to the edge, axis 1 runs along it.
    coverage = mask.mean(axis=1)
    occupied = np.flatnonzero(coverage >= .12)
    if not len(occupied):
        return {"bands": [], "dash_runs": [], "present": False}
    bands = [[a - radius, b - radius] for a, b in runs(coverage >= .12)]
    along = np.any(mask[occupied], axis=0)
    dash_runs = [b - a for a, b in runs(along, complete=True)]
    gap_runs = [b - a for a, b in runs(~along, complete=True)]
    return {"bands": bands, "dash_runs": dash_runs, "gap_runs": gap_runs,
            "solid": bool(along.all()),
            "present": True, "centre": float(np.average(np.arange(len(coverage)) - radius, weights=np.where(coverage >= .12, coverage, 0))),
            "thickness": sum(b - a for a, b in bands)}


def register_baseline(a, b, band_a, band_b, radius):
    """Estimate vertical displacement from ink-row density, independently of descender fringes."""
    rows_a, rows_b = a.sum(axis=1).astype(float), b.sum(axis=1).astype(float)
    rows_a[:band_a[0]] = 0; rows_a[band_a[1]:] = 0
    rows_b[:band_b[0]] = 0; rows_b[band_b[1]:] = 0
    rows_a = np.pad(rows_a,(radius,radius))
    rows_b = np.pad(rows_b,(radius,radius))
    norm = np.linalg.norm(rows_a) * np.linalg.norm(rows_b)
    if not norm:
        return {"measured":False,"reason":"no foreground row profile"}
    candidates = sorted(((lag,float(np.dot(rows_a,np.roll(rows_b,-lag))/norm)) for lag in range(-radius,radius+1)),key=lambda p:-p[1])
    best, second = candidates[:2]
    reliable = best[1] >= .85 and best[1] - second[1] >= .03
    return {"measured":reliable,"delta_px":best[0],"similarity":best[1],"next_similarity":second[1],
            "minimum_similarity":.85,"minimum_separation":.03,"search_radius_px":radius,
            "reason":"" if reliable else "vertical profile does not identify a unique displacement"}


def overflow_samples(excel_image, dna_image, r, rect, row, col, refcells, cols, source, cell):
    """The source's empty neighbours bound overflow; the planned paint rectangle is not trusted."""
    entry = source["cells"].get((row,col), {"style": source["default_style"]})
    style = entry["style"]
    if not cell or cell["shown"]["kind"] != "text" or style.get("wrap") or style.get("rotation"):
        return None
    horizontal = style.get("horizontal") or "left"
    if horizontal not in ("general", "left", "right", "center"):
        return None
    def blocked(c):
        if c not in cols or (row,c) not in refcells or not refcells[row,c]["raster_calibrated"]:
            return True
        e = source["cells"].get((row,c))
        if e and (e["occupied"] or e["style"]["pattern"] != "none"):
            return True
        return any(top <= row <= bottom and left <= c <= right for top,left,bottom,right in source["merges"])
    left = right = col
    if horizontal in ("right", "center"):
        while left > 1 and not blocked(left - 1):
            left -= 1
    if horizontal in ("left", "general", "center"):
        while not blocked(right + 1):
            right += 1
    er = refcells[row,left]
    end = refcells[row,right]
    width = round(end["x"] + end["w"] - er["x"])
    offset = round(r["x"] - er["x"])
    height = min(round(r["h"]), round(rect["h"]))
    ea = excel_image.crop((round(er["x"]),round(r["y"]),round(er["x"])+width,round(r["y"])+height))
    db = dna_image.crop((round(rect["x"])-offset,round(rect["y"]),round(rect["x"])-offset+width,round(rect["y"])+height))
    return ea, db, offset


def parity(a, b, delta, reference_text, cell, excel_rect, dna_rect, excel_image, dna_image, source=None, scale=1, text_samples=None):
    checks = {name: check(reason="not applicable to this cell") for name in METRICS}
    checks["geometry"] = check(all(abs(v) < .01 for v in delta.values()), delta_px=delta)
    shown = cell["shown"] if cell else None
    style = shown["style"] if shown else {}
    expected = source["cells"].get((excel_rect["row"], excel_rect["col"]), {"style": source["default_style"]})["style"] if source else style
    text = shown["text"] if shown else ""
    checks["display_text"] = check(reference_text == text, excel=reference_text, dna=text,
                                   provenance=shown.get("provenance") if shown else None)
    conditional = (shown or {}).get("conditional") or {}
    foreground = rgb_value(conditional.get("font_color") or (shown or {}).get("color") or expected.get("font_color"))
    brush = style.get("brush") or {}
    pattern = expected.get("pattern")
    ambiguous = (pattern not in (None, "none", "solid", "gradient")) or expected.get("rotation", 0) != 0
    pixels = expected.get("pixels", 15) * scale
    text_a, text_b, text_offset = text_samples if text_samples else (a,b,0)
    ta, ma = text_observation(text_a, foreground, pixels)
    tb, mb = text_observation(text_b, foreground, pixels)
    for observation in (ta,tb):
        if observation["box"]:
            observation["box"][0] -= text_offset
            observation["box"][2] -= text_offset
    if not expected.get("wrap") and "\n" not in reference_text and not expected.get("rotation"):
        # Cropping a digit through its hollow centre can leave disconnected top/bottom ink.
        # Those bands are one line, unless the total height establishes actual wrapping.
        for observation in (ta, tb):
            box = observation["box"]
            if box and box[3] - box[1] <= pixels * 1.3:
                observation["line_bands"] = [[box[1], box[3]]]
    checks["text_ink_box"] = check(reason="blank cell has no source text")
    if reference_text or text or ta["box"] or tb["box"]:
        aa, bb = ta["box"], tb["box"]
        eligible = not ambiguous
        reason = "pattern or rotated text cannot be separated reliably" if ambiguous else ""
        present = bool(aa and bb)
        diffs = [bb[i] - aa[i] for i in range(4)] if present else None
        checks["text_ink_box"] = check(None if not eligible else present and all(abs(v) <= 1 for v in diffs),
                                       excel=ta, dna=tb, delta_px=diffs, tolerance_px=1, reason=reason)
        same = reference_text == text
        ba, bd = ta["line_bands"], tb["line_bands"]
        checks["line_breaks"] = check(None if not eligible else len(ba) == len(bd) and present,
                                      excel_line_bands=ba, dna_line_bands=bd,
                                      method="visible ink bands; line contents are not OCR verified")
        if eligible and same and reference_text and present and len(ba) == len(bd) and not expected.get("underline") and not expected.get("strike"):
            full = [(e,d) for e,d in zip(ba,bd) if e[0] > 0 and d[0] > 0 and e[1] < text_a.height and d[1] < text_b.height]
            registrations = [register_baseline(ma,mb,e,d,min(32,max(4,round(pixels)))) for e,d in full]
            baselines = [r["delta_px"] for r in registrations if r["measured"]]
            checks["baseline"] = check(None if not baselines else all(n == 0 for n in baselines), delta_px=baselines, tolerance_px=0,
                                       measured_lines=len(baselines), clipped_lines_not_measured=len(ba)-len(full),
                                       registrations=registrations, ink_bottom_delta_px=[d[1]-e[1] for e,d in full],
                                       method="relative vertical registration of ink-row profiles for identical text; estimated displacement, not an absolute Excel font baseline")
            h = style.get("horizontal") or ("right" if shown["kind"] == "number" else "center" if shown["kind"] in ("logical", "error") else "left")
            offset = diffs[2] if h == "right" else (diffs[0] + diffs[2]) / 2 if h in ("center", "centerContinuous") else diffs[0]
            checks["alignment"] = check(abs(offset) <= 1, horizontal=h, anchor_delta_px=offset, tolerance_px=1)
        else:
            checks["baseline"] = check(reason=reason or "different text, missing ink or unequal visible line counts")
        if eligible and present:
            # Compare occupied boundary strips and visible endpoints. Different strings fail independently.
            extent = round(excel_rect["w"])
            contacts = lambda box: [box[0] < 0, box[2] > extent, box[1] <= 0, box[3] >= a.height]
            ca, cb = contacts(aa), contacts(bb)
            checks["clipping_overflow"] = check(same and ca == cb and abs((aa[2]-aa[0])-(bb[2]-bb[0])) <= 2,
                                                excel_overflow_or_vertical_clip=ca, dna_overflow_or_vertical_clip=cb,
                                                excel_envelope=aa,dna_envelope=bb,width_tolerance_px=2,
                                                method="visible envelope over the source's empty neighbours, bounded independently of the render plan; no OCR identity claim")
            def ink_class(image, mask):
                p = np.asarray(image)[mask]
                return colour_class(np.median(p, axis=0)) if len(p) else None
            c1, c2 = ink_class(text_a, ma), ink_class(text_b, mb)
            checks["text_colour_class"] = check(c1 == c2 and c1 is not None, excel=c1, dna=c2)
        elif eligible:
            checks["text_colour_class"] = check(False, reason="source foreground ink is missing in at least one raster")
    # Modal interior colour measures background class without counting glyph antialiasing.
    def fill(im):
        p = np.asarray(im)[3:-3, 3:-3].reshape(-1, 3)
        if not len(p):
            return None
        quantized = (p // 16).astype(np.uint16)
        codes = quantized[:, 0] * 256 + quantized[:, 1] * 16 + quantized[:, 2]
        mode = int(np.bincount(codes).argmax())
        selected = p[codes == mode]
        return colour_class(np.median(selected, axis=0))
    fa, fb = fill(a), fill(b)
    checks["fill_colour_class"] = check(None if fa is None or fb is None else fa == fb, excel=fa, dna=fb,
                                        reason="no unobstructed interior" if fa is None or fb is None else "")
    borders = expected.get("borders", [])
    edges = []
    for side, edge in enumerate(borders):
        if not edge:
            continue
        colour = rgb_value(edge.get("color"))
        ea = border_observation(excel_image, excel_rect, side, colour)
        eb = border_observation(dna_image, dna_rect, side, colour)
        edges.append({"side": ("left", "right", "top", "bottom")[side], "style": edge["style"], "excel": ea, "dna": eb})
    if edges:
        visible = [e for e in edges if e["excel"] is not None and e["dna"] is not None and (e["excel"]["present"] or e["dna"]["present"])]
        usable = bool(visible)
        exists = usable and all(e["excel"]["present"] and e["dna"]["present"] for e in visible)
        checks["line_position"] = check(None if not usable else exists and all(abs(e["dna"]["centre"] - e["excel"]["centre"]) <= .5 for e in visible), edges=edges, measured_edges=len(visible), tolerance_px=.5)
        checks["line_thickness"] = check(None if not usable else exists and all(e["dna"]["thickness"] == e["excel"]["thickness"] for e in visible), edges=edges, measured_edges=len(visible), tolerance_px=0)
        dashed = [e for e in visible if "dash" in e["style"].lower() or e["style"] == "dotted"]
        if dashed and exists:
            comparisons=[]
            for edge in dashed:
                for key in ("dash_runs","gap_runs"):
                    a_runs,b_runs=edge["excel"].get(key,[]),edge["dna"].get(key,[])
                    if key=="gap_runs" and ((a_runs and edge["dna"].get("solid")) or (b_runs and edge["excel"].get("solid"))):
                        comparisons.append(False)
                    else:
                        comparisons.append(None if not a_runs or not b_runs else abs(float(np.median(a_runs))-float(np.median(b_runs)))<=1)
            outcome=False if False in comparisons else None if None in comparisons else True
            checks["line_dashes"] = check(outcome, edges=dashed, tolerance_px=1,
                reason="a complete dash or gap cannot be measured in the strip" if outcome is None else "",
                method="median lengths of complete ink and gap runs; phase and glyph identity are not compared")
        def corner(im, rect, right, bottom, colour):
            x = round(rect["x"] + (rect["w"] if right else 0))
            y = round(rect["y"] + (rect["h"] if bottom else 0))
            mask = colour_mask(np.asarray(im.crop((x - 4, y - 4, x + 5, y + 5))), colour, tolerance=16)
            yy, xx = np.nonzero(mask)
            return {"box": [int(xx.min()), int(yy.min()), int(xx.max()) + 1, int(yy.max()) + 1] if len(xx) else None,
                    "quadrants": [bool(mask[ys, xs].any()) for ys, xs in ((slice(0,4),slice(0,4)),(slice(0,4),slice(4,9)),(slice(4,9),slice(0,4)),(slice(4,9),slice(4,9)))]}
        joins = []
        for right, bottom, sides in [(False, False, (0,2)),(True,False,(1,2)),(False,True,(0,3)),(True,True,(1,3))]:
            if all(borders[i] for i in sides):
                colour = rgb_value(borders[sides[0]].get("color"))
                joins.append({"corner": [right,bottom], "excel": corner(excel_image,excel_rect,right,bottom,colour), "dna": corner(dna_image,dna_rect,right,bottom,colour)})
        if joins:
            visible_joins = [j for j in joins if j["excel"]["box"] or j["dna"]["box"]]
            checks["line_joins"] = check(None if not visible_joins else all(j["excel"]["box"] and j["dna"]["box"] and j["excel"]["quadrants"] == j["dna"]["quadrants"] for j in visible_joins), corners=joins, measured_corners=len(visible_joins),
                                          method="ink presence in four quadrants around each original corner")
    if pattern and pattern not in ("none", "solid", "gradient"):
        colour = rgb_value(expected.get("fill"))
        def texture(im):
            # The upper interior strip stays clear of the authored bottom-aligned text.
            # Text holes must not destroy a real period or make two unknown periods a pass.
            height=max(8,min(16,im.height//2-3))
            mask = colour_mask(np.asarray(im)[3:3+height,3:-3], colour)
            def period(axis):
                for n in range(1, min(17, mask.shape[axis] // 2 + 1)):
                    left = mask[:-n] if axis == 0 else mask[:,:-n]
                    right = mask[n:] if axis == 0 else mask[:,n:]
                    if np.mean(left != right) <= .03:
                        return n
                return None
            return {"x_period": period(1), "y_period": period(0), "density": float(mask.mean()) if mask.size else None}
        ea, eb = texture(a), texture(b)
        measurable=ea["x_period"] is not None and ea["y_period"] is not None and ea["density"] is not None
        checks["fill_pattern"] = check(None if not measurable else eb["density"] is not None and ea["x_period"] == eb["x_period"] and ea["y_period"] == eb["y_period"] and abs(ea["density"] - eb["density"]) <= .04,
                                         excel=ea, dna=eb, density_tolerance=.04, pattern=pattern,
                                         reason="reference period cannot be isolated from ink" if not measurable else "")
    if source:
        r, c = excel_rect["row"], excel_rect["col"]
        sides = set()
        for top, left, bottom, right in source["selections"]:
            if top <= r <= bottom and left <= c <= right:
                sides.update(i for i, yes in enumerate((c == left,c == right,r == top,r == bottom)) if yes)
        if sides:
            selection = []
            for side in sorted(sides):
                ea = border_observation(excel_image,excel_rect,side,(32,127,64))
                eb = border_observation(dna_image,dna_rect,side,(32,127,64))
                selection.append({"side": side, "excel": ea, "dna": eb})
            measured = all(e["excel"] is not None and e["dna"] is not None and e["excel"]["present"] for e in selection)
            checks["selection"] = check(None if not measured else all(e["excel"]["present"] and e["dna"]["present"] and abs(e["excel"]["centre"] - e["dna"]["centre"]) <= .5 for e in selection), edges=selection,
                                         method="green selection edge positions; no desktop focus changes",
                                         reason="reference selection edges are not visible or lie outside the raster; focus parity is unmeasured" if not measured else "")
    return checks


def parity_summary(cells):
    result = {}
    for name in METRICS:
        counts = Counter(c["parity"][name]["status"] for c in cells)
        measured = counts["pass"] + counts["fail"]
        result[name] = {"status": "fail" if counts["fail"] else "pass" if measured else "unmeasured",
                        "measured": measured, "passed": counts["pass"], "failed": counts["fail"], "unmeasured": counts["unmeasured"]}
    return result


def cell_result(observations, source, row, col, reference_text, cell):
    style=source["cells"].get((row,col),{"style":source["default_style"]})["style"]
    required=["geometry","display_text","fill_colour_class"]
    if reference_text or (cell and cell["shown"]["text"]) or "excel" in observations["text_ink_box"]:
        required += ["text_ink_box","baseline","alignment","clipping_overflow","line_breaks","text_colour_class"]
    edges=style.get("borders",[])
    if any(edges):
        required += ["line_position","line_thickness"]
        if any(edges[a] and edges[b] for a,b in ((0,2),(1,2),(0,3),(1,3))):required += ["line_joins"]
        if any(e and ("dash" in e["style"].lower() or e["style"]=="dotted") for e in edges):required += ["line_dashes"]
    if style.get("pattern") not in (None,"none","solid","gradient"):required += ["fill_pattern"]
    if any(top<=row<=bottom and left<=col<=right and (row in (top,bottom) or col in (left,right)) for top,left,bottom,right in source["selections"]):required += ["selection"]
    failed=[name for name in required if observations[name]["status"]=="fail"]
    unmeasured=[name for name in required if observations[name]["status"]=="unmeasured"]
    if any(top<=row<=bottom and left<=col<=right for top,left,bottom,right in source["merges"]):
        unmeasured.append("merged_text_extent")
    return {"status":"fail" if failed else "unmeasured" if unmeasured else "pass",
            "required_checks":required,"failed_checks":failed,"unmeasured_required_checks":unmeasured}


def cell_summary(cells):
    counts=Counter(c["cell_result"]["status"] for c in cells)
    return {"total":len(cells),"measured":counts["pass"]+counts["fail"],"passed":counts["pass"],
            "failed":counts["fail"],"unmeasured":counts["unmeasured"],
            "status":"fail" if counts["fail"] else "unmeasured" if counts["unmeasured"] else "pass"}


def write_report(out, data):
    encoded = json.dumps(data).replace("<", "\\u003c").replace("&", "\\u0026")
    page = r'''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Excel and DNA Calc cell parity</title>
<style>
body{font:14px system-ui;margin:24px;color:#18262c;background:white}h1{font-size:24px}label{display:inline-block;margin:8px 16px 8px 0}select,input{font:inherit;padding:5px}p{max-width:105ch;line-height:1.5}#images{display:flex;gap:16px;overflow:auto}figure{margin:0}figcaption{font-weight:600;margin:8px 0}img{display:block;max-width:none;image-rendering:pixelated;border:1px solid #ddd}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}th,td{padding:7px;border-bottom:1px solid #ddd;text-align:left;vertical-align:top}th{position:sticky;top:0;background:#eef2f3}td.text{white-space:pre-wrap;max-width:28em;overflow-wrap:anywhere}caption{text-align:left;margin:16px 0}pre{white-space:pre-wrap}.fail{color:#a52520}.pass{color:#17683f}.unmeasured{color:#665b42}summary{cursor:pointer;padding:8px 0}a{color:#005c77}
</style>
<h1>Excel and DNA Calc cell parity</h1>
<p>Cell geometry, text ink bounds, relative baselines, alignment, visible clipping and overflow, line breaks, borders, colour classes, fills and selection. Text antialiasing identity is not a goal. Each check names its measurement, tolerance and limits. Unmeasured checks never count as passes.</p>
<p id="desktop"></p>
<p><a href="comparison.json">Complete measurements and source hashes</a></p>
<label>Capture <select id="capture"></select></label>
<label>Summary <select id="scope"><option value="capture">This capture</option><option value="all">All captures</option></select></label>
<p id="summary"></p>
<table><thead><tr><th>Check</th><th>Result</th><th>Measured</th><th>Pass</th><th>Fail</th><th>Unmeasured / not applicable</th></tr></thead><tbody id="metrics"></tbody></table>
<label>Images <select id="view"><option value="pair">Excel and DNA Calc</option><option value="overlay">Overlay</option><option value="difference">Coarse pixel difference</option></select></label>
<div id="images"></div>
<label>Find cell or text <input id="find" type="search"></label>
<label>Check <select id="metric"><option value="">Any check</option></select></label>
<label><input id="failed" type="checkbox" checked> Show failed checks only</label>
<label>Sort <select id="sort"><option value="failures">Largest cell failures first</option><option value="cell">Cell address</option></select></label>
<table><caption id="caption"></caption><thead><tr><th>Cell</th><th>Excel / DNA text</th><th>Result and measurements</th></tr></thead><tbody id="cells"></tbody></table>
<details><summary>Coarse pixel regression observations</summary><p id="pixels"></p><p>Original pixels are retained. Pixel difference rates do not determine cell results or sorting.</p></details>
<details><summary>Cells outside the measured area</summary><pre id="excluded"></pre></details>
<p>Baseline displacement is estimated by vertically registering ink-row profiles for identical text and equal visible line counts. Weak or ambiguous registrations and clipped lines remain unmeasured; absolute Excel font baselines are not exposed. Ink-bottom deltas remain as diagnostics, because a descender fringe alone can change them. Patterned and rotated text are left unmeasured where foreground ink cannot be isolated. Ink line counts do not identify each line's characters. Clipping and overflow compare visible envelopes, not OCR glyph identity. Borders are measured in original images across the cell boundary. Authoring merged cells is never part of this tool.</p>
<script type="application/json" id="observations">__PAINT_DATA__</script>
<script>
const data=JSON.parse(document.getElementById('observations').textContent),get=id=>document.getElementById(id);
const address=c=>{let n=c.col,s='';while(n){n--;s=String.fromCharCode(65+n%26)+s;n=Math.floor(n/26)}return s+c.row};
const label=s=>s.replaceAll('_',' '),failed=c=>Object.values(c.parity).filter(v=>v.status==='fail').length;
function row(target,values,cls=''){const tr=document.createElement('tr');tr.className=cls;for(const v of values){const td=document.createElement('td');td.textContent=v;tr.append(td)}target.append(tr);return tr}
get('desktop').textContent='Desktop: '+data.desktop.status+'. '+(data.desktop.reason||data.desktop.host||'');
data.results.forEach((r,i)=>{const o=document.createElement('option');o.value=i;o.textContent=r.prefix;get('capture').append(o)});
for(const k of Object.keys(data.parity_summary)){const o=document.createElement('option');o.value=k;o.textContent=label(k);get('metric').append(o)}
let pictureKey='';
function draw(){
 const r=data.results[Number(get('capture').value)];if(!r)return;
 const summaries=get('scope').value==='all'?data.parity_summary:r.parity_summary;
 get('metrics').replaceChildren();for(const[k,v]of Object.entries(summaries))row(get('metrics'),[label(k),v.status,v.measured,v.passed,v.failed,v.unmeasured],v.status);
 const all=get('scope').value==='all',cs=all?data.cell_summary:r.cell_summary,outside=all?data.results.reduce((n,r)=>n+r.not_compared.length,0):r.not_compared.length;get('summary').textContent=`Cell result: ${cs.status}. ${cs.measured} measured: ${cs.passed} pass all applicable checks, ${cs.failed} fail measured checks; ${cs.unmeasured} lack a required measurement. ${outside} outside the measured area. One cell may fail several checks. Scope: ${all?'all captures':r.prefix}.`;
 const key=get('capture').value+':'+get('view').value;
 if(key!==pictureKey){pictureKey=key;get('images').replaceChildren();const views=get('view').value==='pair'?[['reference-sheet','Excel'],['dna-sheet','DNA Calc']]:[[get('view').value,get('view').value]];for(const[suffix,name]of views){const f=document.createElement('figure'),c=document.createElement('figcaption'),img=document.createElement('img');c.textContent=name;img.alt=name+' original sheet crop';img.src=encodeURIComponent(r.prefix)+'-'+suffix+'.png';f.append(c,img);get('images').append(f)}}
 const q=get('find').value.toLowerCase(),metric=get('metric').value;
 const cells=r.cells.filter(c=>(!q||address(c).toLowerCase().includes(q)||c.excel_text.toLowerCase().includes(q)||(c.dna_text||'').toLowerCase().includes(q))&&(!get('failed').checked||(metric?c.parity[metric].status==='fail':failed(c)>0)));
 cells.sort(get('sort').value==='failures'?(a,b)=>failed(b)-failed(a)||a.row-b.row||a.col-b.col:(a,b)=>a.row-b.row||a.col-b.col);
 get('cells').replaceChildren();for(const c of cells){const tr=row(get('cells'),[address(c),JSON.stringify(c.excel_text)+' / '+JSON.stringify(c.dna_text??''),'']);tr.children[1].className='text';const target=tr.children[2],status=document.createElement('strong');status.className=c.cell_result.status;status.textContent=c.cell_result.status;target.append(status);for(const[k,v]of Object.entries(c.parity)){if(metric&&metric!==k)continue;if(get('failed').checked&&v.status!=='fail')continue;const d=document.createElement('details'),s=document.createElement('summary'),p=document.createElement('pre');s.textContent=label(k)+': '+v.status;s.className=v.status;p.textContent=JSON.stringify(v,null,2);d.append(s,p);target.append(d)}}
 get('caption').textContent=`${cells.length} cells displayed. Checks are sorted by the number of failures, independently of pixel differences.`;
 get('excluded').textContent=JSON.stringify(r.not_compared,null,2);
 get('pixels').textContent=`Exact changed pixels: ${r.global_exact.changed}/${r.global_exact.pixels} (${r.global_exact.percent.toFixed(2)}%). Above 16 channel levels: ${r.global_over16.percent.toFixed(2)}%. Translation: ${r.translation_px.x}, ${r.translation_px.y} px.`;
}
for(const id of['capture','scope','view','find','metric','sort','failed'])get(id).addEventListener('input',draw);draw();
</script></html>'''
    (out / "comparison.html").write_text(page.replace("__PAINT_DATA__", encoded), encoding="utf-8")


def self_test():
    """Known damaged rasters must fail the same functions used by the real report."""
    import unittest
    from PIL import ImageDraw
    class Controls(unittest.TestCase):
        def setUp(self):
            self.a = Image.new("RGB", (128,60), "white")
            ImageDraw.Draw(self.a).text((10,20), "Ag 0123", fill="black")
            self.rect = dict(x=0,y=0,w=128,h=60,row=1,col=1)
            self.style = {"font_color":"#000000","pixels":14,"rotation":0,"wrap":False,"borders":[None]*4}
            self.cell = {"shown":{"text":"Ag 0123","kind":"text","style":self.style,"provenance":"literal"}}
        def measure(self,b, **kwargs):
            return parity(self.a,b,dict(x=0,y=0,w=0,h=0),"Ag 0123",self.cell,self.rect,self.rect,self.a,b,**kwargs)
        def test_identical_and_antialias_fringe(self):
            b=self.a.copy();ImageDraw.Draw(b).point((3,3),fill=(160,160,160))
            for key in ("geometry","display_text","text_ink_box","baseline","alignment","clipping_overflow","line_breaks","fill_colour_class","text_colour_class"):
                self.assertEqual(self.measure(b)[key]["status"],"pass",key)
            self.assertGreater(changed(self.a,b)["changed"],0)
        def test_shifted_baseline(self):
            b=Image.new("RGB",self.a.size,"white");b.paste(self.a,(0,3))
            result=self.measure(b)
            self.assertEqual(result["baseline"]["status"],"fail")
            self.assertEqual(result["baseline"]["delta_px"],[3])
            self.assertEqual(result["text_ink_box"]["status"],"fail")
        def test_descender_fringe_does_not_move_the_estimated_baseline(self):
            b=self.a.copy();observation,_=text_observation(self.a,(0,0,0))
            box=observation["box"];ImageDraw.Draw(b).point((box[2]-1,box[3]+2),fill="black")
            result=self.measure(b)
            self.assertEqual(result["baseline"]["status"],"pass")
            self.assertEqual(result["baseline"]["delta_px"],[0])
            self.assertEqual(result["baseline"]["ink_bottom_delta_px"],[3])
            self.assertEqual(result["text_ink_box"]["status"],"fail")
        def test_erased_ink_and_wrong_colour(self):
            for colour in ("white","blue"):
                b=Image.new("RGB",self.a.size,"white");ImageDraw.Draw(b).text((10,20),"Ag 0123",fill=colour)
                result=self.measure(b)
                self.assertEqual(result["text_ink_box"]["status"],"fail")
                self.assertEqual(result["text_colour_class"]["status"],"fail")
                self.assertNotEqual(result["baseline"]["status"],"pass")
        def test_unexpected_ink_in_a_blank_cell(self):
            self.cell["shown"]["text"]=""
            blank=Image.new("RGB",self.a.size,"white")
            result=parity(blank,self.a,dict(x=0,y=0,w=0,h=0),"",self.cell,self.rect,self.rect,blank,self.a)
            self.assertEqual(result["text_ink_box"]["status"],"fail")
            self.assertEqual(result["baseline"]["status"],"unmeasured")
        def test_different_display_text_is_not_a_baseline_pass(self):
            self.cell["shown"]["text"]="Ag 1234"
            result=self.measure(self.a)
            self.assertEqual(result["display_text"]["status"],"fail")
            self.assertEqual(result["baseline"]["status"],"unmeasured")
        def test_missing_overflow(self):
            b=self.a.copy();ImageDraw.Draw(b).rectangle((32,0,127,59),fill="white")
            self.rect["w"]=32
            result=self.measure(b,text_samples=(self.a,b,0))
            self.assertEqual(result["clipping_overflow"]["status"],"fail")
            self.assertTrue(result["clipping_overflow"]["excel_overflow_or_vertical_clip"][1])
            self.assertFalse(result["clipping_overflow"]["dna_overflow_or_vertical_clip"][1])
        def test_wrap_and_fill_damage(self):
            self.style["wrap"]=True
            ImageDraw.Draw(self.a).text((10,36),"second line",fill="black")
            b=Image.new("RGB",self.a.size,"yellow");ImageDraw.Draw(b).text((10,20),"Ag 0123",fill="black")
            result=self.measure(b)
            self.assertEqual(result["line_breaks"]["status"],"fail")
            self.assertEqual(result["fill_colour_class"]["status"],"fail")
        def test_border_position_and_thickness(self):
            rect=dict(x=20,y=20,w=80,h=35)
            def image(shift=0,width=1):
                im=Image.new("RGB",(128,80),"white");d=ImageDraw.Draw(im)
                d.line((19+shift,20,19+shift,54),fill="#154c94",width=width)
                return im
            original=border_observation(image(),rect,0,(21,76,148))
            moved=border_observation(image(3),rect,0,(21,76,148))
            thick=border_observation(image(0,3),rect,0,(21,76,148))
            self.assertEqual(original["thickness"],1)
            self.assertEqual(moved["centre"]-original["centre"],3)
            self.assertEqual(thick["thickness"],3)
            self.assertFalse(border_observation(Image.new("RGB",(128,80),"#808080"),rect,0,(21,76,148))["present"])
        def test_border_fringe_is_not_an_extra_pen_band(self):
            rect=dict(x=20,y=20,w=80,h=35)
            image=Image.new("RGB",(128,80),"white");d=ImageDraw.Draw(image)
            d.line((19,20,19,54),fill="#154c94")
            # A measured ClearType blue fringe beside black text, not border ink.
            d.line((24,35,24,44),fill=(0,58,144))
            seen=border_observation(image,rect,0,(21,76,148))
            self.assertEqual(seen["bands"],[[-1,0]])
            self.assertEqual(seen["centre"],-1)
        def test_complete_dash_and_gap_runs_keep_first_and_last_interior_runs(self):
            self.assertEqual(runs([True,True,False,False,True,False,False,True],complete=True),[[4,5]])
            self.assertEqual(runs([False,False,True,True,False,True,True,False],complete=True),[[2,4],[5,7]])
            self.assertEqual(runs([True,False,False,True,False,False,True],complete=True),[[3,4]])
            self.assertEqual(runs([False,True,True,False,True,True,False],complete=True),[[1,3],[4,6]])
        def test_pattern_period(self):
            self.style["pattern"]="lightGrid";self.style["fill"]="#154c94"
            source={"cells":{(1,1):{"style":self.style}},"default_style":self.style,"selections":[]}
            def image(period):
                im=Image.new("RGB",self.a.size,"#fff0cc");d=ImageDraw.Draw(im)
                for x in range(0,128,period):d.line((x,0,x,59),fill="#154c94")
                for y in range(0,60,period):d.line((0,y,127,y),fill="#154c94")
                return im
            self.a=image(4)
            self.assertEqual(self.measure(image(4),source=source)["fill_pattern"]["status"],"pass")
            result=self.measure(image(8),source=source)["fill_pattern"]
            self.assertEqual(result["status"],"fail")
            self.assertEqual(result["excel"]["x_period"],4)
            self.assertEqual(result["dna"]["x_period"],8)
        def test_summary_keeps_unmeasured_out_of_passes(self):
            result=parity_summary([{"parity":self.measure(self.a)}])
            self.assertEqual(result["geometry"]["measured"],1)
            self.assertEqual(result["line_position"]["measured"],0)
            self.assertEqual(result["line_position"]["passed"],0)
        def test_required_unmeasured_check_prevents_a_cell_pass(self):
            source={"cells":{},"default_style":self.style,"selections":[],"merges":[]}
            measured=self.measure(self.a)
            measured["baseline"]=check(reason="unknown baseline")
            result=cell_result(measured,source,1,1,"Ag 0123",self.cell)
            self.assertEqual(result["status"],"unmeasured")
            self.assertIn("baseline",result["unmeasured_required_checks"])
            summary=cell_summary([{"cell_result":result}])
            self.assertEqual(summary["passed"],0)
            self.assertEqual(summary["measured"],0)
        def test_two_unknown_pattern_periods_do_not_pass(self):
            self.style["pattern"]="lightGrid";self.style["fill"]="#154c94"
            source={"cells":{(1,1):{"style":self.style}},"default_style":self.style,"selections":[]}
            d=ImageDraw.Draw(self.a)
            for y in range(60):
                for x in range(128):
                    if (x*x+17*y*y+13*x*y)%31<10:d.point((x,y),fill="#154c94")
            self.assertEqual(self.measure(self.a,source=source)["fill_pattern"]["status"],"unmeasured")
        def test_border_join_and_dashes(self):
            self.rect=dict(x=20,y=20,w=80,h=35,row=1,col=1)
            self.cell["shown"]["text"]=""
            self.style["borders"]=[{"style":"thin","color":"#154c94"}]*4
            self.a=Image.new("RGB",(128,80),"white")
            ImageDraw.Draw(self.a).rectangle((19,19,99,54),outline="#154c94")
            def measure(b):
                return parity(self.a.crop((20,20,100,55)),b.crop((20,20,100,55)),dict(x=0,y=0,w=0,h=0),"",self.cell,self.rect,self.rect,self.a,b)
            self.assertEqual(measure(self.a)["line_joins"]["status"],"pass")
            b=self.a.copy();ImageDraw.Draw(b).rectangle((17,17,23,23),fill="white")
            self.assertEqual(measure(b)["line_joins"]["status"],"fail")
            self.style["borders"]=[{"style":"dashed","color":"#154c94"},None,None,None]
            self.a=Image.new("RGB",(128,80),"white");d=ImageDraw.Draw(self.a)
            for y in range(20,55,5):d.line((19,y,19,y+2),fill="#154c94")
            b=self.a.copy();ImageDraw.Draw(b).line((19,20,19,54),fill="#154c94")
            self.assertEqual(measure(self.a)["line_dashes"]["status"],"pass")
            self.assertEqual(measure(b)["line_dashes"]["status"],"fail")
        def test_selection_missing_edge(self):
            self.rect=dict(x=20,y=20,w=80,h=35,row=1,col=1)
            self.cell["shown"]["text"]=""
            self.a=Image.new("RGB",(128,80),"white")
            ImageDraw.Draw(self.a).rectangle((19,19,99,54),outline="#207f40")
            source={"cells":{},"default_style":self.style,"selections":[(1,1,1,1)]}
            def measure(b):
                return parity(self.a.crop((20,20,100,55)),b.crop((20,20,100,55)),dict(x=0,y=0,w=0,h=0),"",self.cell,self.rect,self.rect,self.a,b,source)
            self.assertEqual(measure(self.a)["selection"]["status"],"pass")
            self.assertEqual(measure(Image.new("RGB",(128,80),"white"))["selection"]["status"],"fail")
        def test_two_strips_without_complete_dash_lengths_do_not_pass(self):
            self.rect=dict(x=20,y=20,w=80,h=35,row=1,col=1)
            self.cell["shown"]["text"]=""
            self.style["borders"]=[{"style":"dashed","color":"#154c94"},None,None,None]
            self.a=Image.new("RGB",(128,80),"white")
            ImageDraw.Draw(self.a).line((19,20,19,54),fill="#154c94")
            result=parity(self.a.crop((20,20,100,55)),self.a.crop((20,20,100,55)),dict(x=0,y=0,w=0,h=0),"",self.cell,self.rect,self.rect,self.a,self.a)
            self.assertEqual(result["line_dashes"]["status"],"unmeasured")
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Controls))
    if not result.wasSuccessful():
        raise SystemExit(1)


def compare(out, report=None):
    report = report or out
    report.mkdir(parents=True, exist_ok=True)
    browser = json.loads((out / "browser.json").read_text(encoding="utf-8"))
    # canonical Windows paths may carry the Win32 extended path prefix.
    refdir = Path(browser["reference"])
    refs = json.loads((refdir / "excel.json").read_text(encoding="utf-8-sig"))
    refs = [calibrate(ref, Image.open(refdir / (ref["prefix"] + ".png")).convert("RGB")) for ref in refs]
    (report / "calibrated-excel.json").write_text(json.dumps(refs, indent=2), encoding="utf-8")
    request = json.loads((out / "request.json").read_text(encoding="utf-8-sig"))
    book_by_hash = {book["sha256"]: Path(book["path"]) for book in request["books"]}
    sources = {}
    results = []
    for capture in browser["captures"]:
        ref = next(r for r in refs if (r["sha256"], r["sheet"], r["zoom"]) ==
                   (capture["sha256"], capture["sheet"], capture["zoom"]))
        prefix = capture["prefix"]
        png = out / (prefix + ".png")
        encoded_png = out / (prefix + ".png.base64")
        if encoded_png.exists():
            raw = encoded_png.read_bytes()
            recorded_hash = capture.get("png_base64_sha256")
            if recorded_hash and hashlib.sha256(raw).hexdigest() != recorded_hash:
                raise ValueError("Browser capture hash changed")
            png.write_bytes(base64.b64decode(raw))
        elif not png.exists():
            raise ValueError("Browser capture image is missing")
        dna = Image.open(png).convert("RGB")
        reference_png = refdir / (ref["prefix"] + ".png")
        if hashlib.sha256(reference_png.read_bytes()).hexdigest() != ref["image_sha256"]:
            raise ValueError("Reference image hash changed")
        excel = Image.open(reference_png).convert("RGB")
        dpr = capture["dpr"]
        if abs(dpr - ref["dpi"] / 96) > 1e-6:
            raise ValueError("Different display scales; refusing a resampled comparison")
        plan = capture["plan"]
        key = capture["sha256"], capture["sheet"]
        if key not in sources:
            path = book_by_hash[key[0]]
            if hashlib.sha256(path.read_bytes()).hexdigest() != key[0]:
                raise ValueError("Input workbook hash changed")
            sources[key] = workbook_cells(path, key[1])
        source_cells = sources[key]
        shown_cells = {(c["row"], c["col"]): c for c in plan["cells"]}
        layer = capture["layer_rect"]
        cols = {c["col"]: c["rect"] for c in plan["col_headers"]}
        rows = {r["row"]: r["rect"] for r in plan["row_headers"]}
        # Capture enforces zero scroll. Any leading hidden bands have zero extent, so the
        # first visible headers share the sheet origin even when row1/columnA are hidden.
        origin = {"x": (layer["x"] + cols[min(cols)]["x"]) * dpr,
                  "y": (layer["y"] + rows[min(rows)]["y"]) * dpr}
        refcells = {(c["row"], c["col"]): c for c in ref["cells"]}
        comparisons = []
        skipped = []
        for (row, col), r in refcells.items():
            if r["points"]["w"] == 0 or r["points"]["h"] == 0:
                skipped.append({"row": row, "col": col, "reason": "hidden Excel row or column"})
                continue
            if not r["raster_calibrated"]:
                skipped.append({"row": row, "col": col, "reason": "Excel edge outside calibrated raster"})
                continue
            if row not in rows or col not in cols:
                skipped.append({"row": row, "col": col, "reason": "outside DNA viewport"})
                continue
            rect = {"x": (layer["x"] + cols[col]["x"]) * dpr,
                    "y": (layer["y"] + rows[row]["y"]) * dpr,
                    "w": cols[col]["w"] * dpr, "h": rows[row]["h"] * dpr}
            bounds = ref["sheet_bounds"]
            if (r["y"] + r["h"] > bounds["y"] + bounds["h"] or
                    r["x"] + r["w"] > bounds["x"] + bounds["w"] or
                    rows[row]["y"] + rows[row]["h"] > plan["viewport"]["height"] or
                    cols[col]["x"] + cols[col]["w"] > plan["viewport"]["width"]):
                skipped.append({"row": row, "col": col, "reason": "partly outside sheet viewport"})
                continue
            # Compare the shared local extent, and report geometry separately. No stretching.
            w, h = min(round(r["w"]), round(rect["w"])), min(round(r["h"]), round(rect["h"]))
            a, b = crop(excel, dict(r, w=w, h=h)), crop(dna, dict(rect, w=w, h=h))
            if a is None or b is None:
                skipped.append({"row": row, "col": col, "reason": "outside captured image or hidden"})
                continue
            delta = {"x": rect["x"] - origin["x"] - (r["x"] - ref["origin"]["x"]),
                     "y": rect["y"] - origin["y"] - (r["y"] - ref["origin"]["y"]),
                     "w": rect["w"] - r["w"], "h": rect["h"] - r["h"]}
            ink_a, ink_b = dark_ink(a), dark_ink(b)
            shown = shown_cells.get((row, col), {}).get("shown")
            comparisons.append({"dna_text": shown["text"] if shown else "",
                                "dna_kind": shown["kind"] if shown else None,
                                "dna_provenance": shown["provenance"] if shown else None,
                                "excel_number_format": r.get("number_format"),
                                "excel_number_format_local": r.get("number_format_local"),
                                "source_formula": shown["formula"] if shown else None,"row": row, "col": col, "excel_text": r["text"],
                                "geometry_delta_px": delta, "exact": changed(a, b),
                                "over16": changed(a, b, 16), "over64": changed(a, b, 64),
                                "dark_ink_bounds": {"excel": ink_a, "dna": ink_b},
                                "missing_text_ink": bool(r["text"] and ink_a and not ink_b)})
            samples = overflow_samples(excel,dna,r,rect,row,col,refcells,cols,source_cells,shown_cells.get((row,col)))
            comparisons[-1]["parity"] = parity(a,b,delta,r["text"],shown_cells.get((row,col)),r,rect,excel,dna,source_cells,capture["zoom"]/100,samples)
            comparisons[-1]["cell_result"] = cell_result(comparisons[-1]["parity"],source_cells,row,col,r["text"],shown_cells.get((row,col)))
        if not comparisons:
            raise ValueError("No comparable cells")
        calibrated_cells = [c for c in ref["cells"] if c["raster_calibrated"]]
        w = min(round(max(c["x"] + c["w"] for c in calibrated_cells) - ref["origin"]["x"]), round(dna.width - origin["x"]), round(excel.width - ref["origin"]["x"]))
        h = min(round(max(c["y"] + c["h"] for c in calibrated_cells) - ref["origin"]["y"]), round(dna.height - origin["y"]), round(excel.height - ref["origin"]["y"]))
        w = min(w, round((layer["x"] + plan["viewport"]["width"]) * dpr - origin["x"]), round(ref["sheet_bounds"]["x"] + ref["sheet_bounds"]["w"] - ref["origin"]["x"]))
        h = min(h, round((layer["y"] + plan["viewport"]["height"]) * dpr - origin["y"]), round(ref["sheet_bounds"]["y"] + ref["sheet_bounds"]["h"] - ref["origin"]["y"]))
        a, b = crop(excel, dict(ref["origin"], w=w, h=h)), crop(dna, dict(origin, w=w, h=h))
        if a is None or b is None:
            raise ValueError("Aligned sheet is outside the image")
        low, high = a.convert("L").getextrema()
        if high < 240 or (any(c["text"] for c in ref["cells"]) and low > 200):
            raise ValueError("Excel capture is blank or black instead of a painted sheet")
        diff = ImageChops.difference(a, b)
        a.save(report / (prefix + "-reference-sheet.png"))
        b.save(report / (prefix + "-dna-sheet.png"))
        diff.save(report / (prefix + "-difference.png"))
        Image.blend(a, b, .5).save(report / (prefix + "-overlay.png"))
        headers = {}
        er, dr = ref["sheet_bounds"], {"x": layer["x"] * dpr, "y": layer["y"] * dpr}
        ew, dw = ref["origin"]["x"] - er["x"], origin["x"] - dr["x"]
        eh, dh = ref["origin"]["y"] - er["y"], origin["y"] - dr["y"]
        for name, ar, br in [
            ("rows", {"x": ref["origin"]["x"] - min(ew, dw), "y": ref["origin"]["y"], "w": min(ew, dw), "h": h},
                     {"x": origin["x"] - min(ew, dw), "y": origin["y"], "w": min(ew, dw), "h": h}),
            ("columns", {"x": ref["origin"]["x"], "y": ref["origin"]["y"] - min(eh, dh), "w": w, "h": min(eh, dh)},
                        {"x": origin["x"], "y": origin["y"] - min(eh, dh), "w": w, "h": min(eh, dh)})]:
            ha, hb = crop(excel, ar), crop(dna, br)
            headers[name] = {"excel_extent_px": ew if name == "rows" else eh,
                             "dna_extent_px": dw if name == "rows" else dh,
                             "common_extent_over16": changed(ha, hb, 16) if ha and hb else None}
        result = {"prefix": prefix, "sha256": capture["sha256"], "zoom": capture["zoom"],
                  "painter": capture["painter"], "sheet": capture["sheet"],
                  "translation_px": {k: origin[k] - ref["origin"][k] for k in origin},
                  "global_exact": changed(a, b), "global_over16": changed(a, b, 16),
                  "headers": headers,
                  "missing_text_ink_cells": sum(c["missing_text_ink"] for c in comparisons),
                  "cells": comparisons, "not_compared": skipped}
        result["parity_summary"] = parity_summary(comparisons)
        result["cell_summary"] = cell_summary(comparisons)
        result["cells_with_failures"] = sum(any(v["status"] == "fail" for v in c["parity"].values()) for c in comparisons)
        result["number_text_differences"] = [{key:c[key] for key in ("row","col","excel_text","dna_text","dna_provenance","source_formula")} for c in comparisons if c["dna_kind"]=="number" and c["parity"]["display_text"]["status"]=="fail"]
        result["image_sha256"] = {"excel": ref["image_sha256"], "dna":hashlib.sha256(png.read_bytes()).hexdigest()}
        result["coverage"] = {"normal_font": ref["normal_font"], "normal_size": ref["normal_size"],
                              "dpi": ref["dpi"], "freeze": ref["freeze"], "scroll": {k:plan["viewport"][k] for k in ("scroll_x","scroll_y")},
                              "source_merges": source_cells["merges"], "source_selection": source_cells["selections"]}
        result["format_environment"] = ref.get("format_environment")
        result["browser_locale"] = capture.get("locale")
        low_b,high_b = b.convert("L").getextrema()
        result["pixel_guard"] = check(not (high_b < 240 or (any(c["excel_text"] for c in comparisons) and low_b > 200)),
                                      method="coarse blank/black-frame guard; difference rates do not determine cell results")
        results.append(result)
        cs=result["cell_summary"]
        print(f"PAINT {prefix}: {cs['status']}; {cs['measured']} measured, {cs['passed']} pass, {cs['failed']} fail, {cs['unmeasured']} lack required measurements; {len(skipped)} outside measured area")
    data = {"schema": "dnacalc-paint/2", "results": results,
        "desktop": browser["desktop"],
        "parity_summary": parity_summary([c for r in results for c in r["cells"]]),
        "cell_summary": cell_summary([c for r in results for c in r["cells"]]),
        "source": {"browser_json_sha256": hashlib.sha256((out/"browser.json").read_bytes()).hexdigest(),
                   "request_sha256": hashlib.sha256((out/"request.json").read_bytes()).hexdigest(),
                   "bundle_source": browser.get("bundle_source"), "bundle": browser.get("bundle"),
                   "source_directory": str(out)},
        "comparison_method_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "method": "A1 translation only; original rasters; cell checks drive results; pixel differences are a coarse regression observation; unmeasured checks never count as passes"}
    (report / "comparison.json").write_text(json.dumps(data, indent=2), encoding="utf-8")
    write_report(report, data)
    cs=data["cell_summary"]
    print(f"PARITY cells: {cs['status']}; measured {cs['measured']}, pass {cs['passed']}, fail {cs['failed']}, unmeasured {cs['unmeasured']}; outside measured area {sum(len(r['not_compared']) for r in results)}")
    for name, count in data["parity_summary"].items():
        print(f"PARITY {name}: {count['status']}; measured {count['measured']}, pass {count['passed']}, fail {count['failed']}, unmeasured/not applicable {count['unmeasured']}")


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", nargs="?", type=Path, help="directory containing request.json, browser.json and original captures")
    parser.add_argument("--out", type=Path, help="write the report separately without changing earlier report files")
    parser.add_argument("--self-test", action="store_true", help="check known damaged rasters through the report measurement functions; starts no apps")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.source:
        compare(args.source, args.out)
    else:
        parser.error("source is required unless --self-test is used")
