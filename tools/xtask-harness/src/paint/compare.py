"""Compare original pixels. Translation only: no scaling, warping, recolouring or golden updates."""
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


def compare(out):
    browser = json.loads((out / "browser.json").read_text(encoding="utf-8"))
    # canonical Windows paths may carry the Win32 extended path prefix.
    refdir = Path(browser["reference"])
    refs = json.loads((refdir / "excel.json").read_text(encoding="utf-8-sig"))
    refs = [calibrate(ref, Image.open(refdir / (ref["prefix"] + ".png")).convert("RGB")) for ref in refs]
    (out / "calibrated-excel.json").write_text(json.dumps(refs, indent=2), encoding="utf-8")
    results = []
    for capture in browser["captures"]:
        ref = next(r for r in refs if (r["sha256"], r["sheet"], r["zoom"]) ==
                   (capture["sha256"], capture["sheet"], capture["zoom"]))
        prefix = capture["prefix"]
        png = out / (prefix + ".png")
        png.write_bytes(base64.b64decode((out / (prefix + ".png.base64")).read_text()))
        dna = Image.open(png).convert("RGB")
        reference_png = refdir / (ref["prefix"] + ".png")
        if hashlib.sha256(reference_png.read_bytes()).hexdigest() != ref["image_sha256"]:
            raise ValueError("Reference image hash changed")
        excel = Image.open(reference_png).convert("RGB")
        dpr = capture["dpr"]
        if abs(dpr - ref["dpi"] / 96) > 1e-6:
            raise ValueError("Different display scales; refusing a resampled comparison")
        plan = capture["plan"]
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
            comparisons.append({"row": row, "col": col, "excel_text": r["text"],
                                "geometry_delta_px": delta, "exact": changed(a, b),
                                "over16": changed(a, b, 16), "over64": changed(a, b, 64),
                                "dark_ink_bounds": {"excel": ink_a, "dna": ink_b},
                                "missing_text_ink": bool(r["text"] and ink_a and not ink_b)})
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
        a.save(out / (prefix + "-reference-sheet.png"))
        b.save(out / (prefix + "-dna-sheet.png"))
        diff.save(out / (prefix + "-difference.png"))
        Image.blend(a, b, .5).save(out / (prefix + "-overlay.png"))
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
        results.append(result)
        print(f"PAINT {prefix}: {len(comparisons)} cells; {len(skipped)} not compared; "
              f"{result['global_over16']['percent']:.2f}% pixels differ by >16; "
              f"{result['missing_text_ink_cells']} cells missing dark text ink")
    (out / "comparison.json").write_text(json.dumps({"schema": "dnacalc-paint/1", "results": results,
        "desktop": browser["desktop"], "method": "A1 translation only; original raster; local common cell extent plus geometry deltas; all unmatched extents reported"}, indent=2), encoding="utf-8")


if __name__ == "__main__":
    compare(Path(sys.argv[1]))
