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


def write_report(out, data):
    # Embed the same observations as comparison.json; workbook strings remain text, not HTML.
    encoded = json.dumps(data).replace("<", "\\u003c").replace("&", "\\u0026")
    page = r'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Excel and DNA Calc painting</title>
<style>
body{font:14px system-ui;margin:24px;color:#18262c;background:#fff}h1{font-size:24px}label{display:inline-block;margin:8px 16px 8px 0}select,input{font:inherit;padding:5px}p{max-width:100ch;line-height:1.5}#images{display:flex;gap:16px;overflow:auto}figure{margin:0}figcaption{font-weight:600;margin:8px 0}img{display:block;max-width:none;image-rendering:pixelated;border:1px solid #ddd}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}th,td{padding:7px;border-bottom:1px solid #ddd;text-align:left;vertical-align:top}th{position:sticky;top:0;background:#eef2f3}td:nth-child(2){white-space:pre-wrap;max-width:30em;overflow-wrap:anywhere}caption{text-align:left;margin:16px 0}pre{white-space:pre-wrap}a{color:#005c77}
</style></head><body>
<h1>Excel and DNA Calc painting</h1>
<p>Original sheet crops aligned at A1. Images display at their original pixel size; scroll to see the full area. Differences are observations, not a pixel-equivalence score. A lower percentage alone does not establish improvement.</p>
<p id="desktop"></p><p><a href="browser.json">Source and browser metadata</a> · <a href="comparison.json">Full cell observations</a></p>
<label>Capture <select id="capture"></select></label>
<label>Images <select id="view"><option value="pair">Excel and DNA Calc</option><option value="overlay">Overlay</option><option value="difference">Difference</option></select></label>
<p id="summary"></p><div id="images"></div>
<label>Find cell or Excel text <input id="find" type="search"></label>
<label>Sort <select id="sort"><option value="cell">Cell address</option><option value="difference">Largest difference above 16</option></select></label>
<label><input id="changed" type="checkbox" checked> Show changed cells only</label>
<table><caption id="caption"></caption><thead><tr><th>Cell</th><th>Excel text</th><th>Exact difference %</th><th>Above 16 %</th><th>Δ x, y, width, height (px)</th><th>Missing dark ink</th></tr></thead><tbody id="cells"></tbody></table>
<details><summary>Cells excluded from comparison</summary><pre id="excluded"></pre></details>
<p>Above 16 means a difference greater than 16 in at least one colour channel. It is not a pass threshold. Dark-ink bounds may include borders, fills or Excel indicators; they do not prove complete glyph identity. Hidden or clipped cells remain excluded.</p>
<script type="application/json" id="observations">__PAINT_DATA__</script>
<script>
const data=JSON.parse(document.getElementById('observations').textContent),get=id=>document.getElementById(id);
const address=c=>{let n=c.col,s='';while(n){n--;s=String.fromCharCode(65+n%26)+s;n=Math.floor(n/26)}return s+c.row};
get('desktop').textContent='Desktop: '+data.desktop.status+'. '+data.desktop.reason;
data.results.forEach((r,i)=>{const o=document.createElement('option');o.value=i;o.textContent=r.prefix;get('capture').append(o)});
let pictureKey='';
function draw(){
  const r=data.results[Number(get('capture').value)];if(!r)return;
  get('summary').textContent=`${r.cells.length} cells compared; ${r.not_compared.length} excluded. Exact changed pixels: ${r.global_exact.changed}/${r.global_exact.pixels} (${r.global_exact.percent.toFixed(2)}%). Above 16: ${r.global_over16.percent.toFixed(2)}%. Translation: ${r.translation_px.x}, ${r.translation_px.y} px. Missing-dark-ink flags: ${r.missing_text_ink_cells}.`;
  const key=get('capture').value+':'+get('view').value;
  if(key!==pictureKey){pictureKey=key;get('images').replaceChildren();
  const views=get('view').value==='pair'?[['reference-sheet','Excel'],['dna-sheet','DNA Calc']]:[[get('view').value,get('view').value==='overlay'?'Overlay':'Difference']];
  for(const[suffix,label]of views){const f=document.createElement('figure'),c=document.createElement('figcaption'),img=document.createElement('img');c.textContent=label;img.alt=label+' sheet area aligned at A1';img.src=encodeURIComponent(r.prefix)+'-'+suffix+'.png';f.append(c,img);get('images').append(f)}
  }
  const q=get('find').value.toLowerCase();
  let cells=r.cells.filter(c=>(!get('changed').checked||c.exact.changed>0||Object.values(c.geometry_delta_px).some(v=>v!==0))&&(!q||address(c).toLowerCase().includes(q)||c.excel_text.toLowerCase().includes(q)));
  cells.sort(get('sort').value==='difference'?(a,b)=>b.over16.percent-a.over16.percent:(a,b)=>a.row-b.row||a.col-b.col);
  get('cells').replaceChildren();
  for(const c of cells){const tr=document.createElement('tr');tr.style.background=`rgba(210,45,35,${c.over16.percent/600})`;for(const v of[address(c),c.excel_text,c.exact.percent.toFixed(2),c.over16.percent.toFixed(2),['x','y','w','h'].map(k=>c.geometry_delta_px[k]).join(', '),c.missing_text_ink?'Yes':'No']){const td=document.createElement('td');td.textContent=v;tr.append(td)}get('cells').append(tr)}
  get('caption').textContent=`${cells.length} displayed of ${r.cells.length} compared cells. No tolerance is counted as a pass.`;
  get('excluded').textContent=r.not_compared.length?JSON.stringify(r.not_compared,null,2):'None';
}
for(const id of['capture','view','find','sort','changed'])get(id).addEventListener('input',draw);
draw();
</script></body></html>'''
    (out / "comparison.html").write_text(page.replace("__PAINT_DATA__", encoded), encoding="utf-8")


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
    data = {"schema": "dnacalc-paint/1", "results": results,
        "desktop": browser["desktop"], "method": "A1 translation only; original raster; local common cell extent plus geometry deltas; all unmatched extents reported"}
    (out / "comparison.json").write_text(json.dumps(data, indent=2), encoding="utf-8")
    write_report(out, data)


if __name__ == "__main__":
    compare(Path(sys.argv[1]))
