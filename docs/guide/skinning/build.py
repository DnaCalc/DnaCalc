#!/usr/bin/env python3
"""Assembles docs/guide/skinning/index.html from src/part*.html and shots/*.webp.

The output is one self-contained file in the form the claude.ai Artifact viewer takes: no doctype,
html, head or body tags; CSS and JS inline; screenshots as data URIs. Run: python3 build.py
"""
import base64
import pathlib
import re
import sys

here = pathlib.Path(__file__).resolve().parent
text = "".join((here / "src" / f"part{n}.html").read_text(encoding="utf-8") for n in (1, 2, 3))


def image(match):
    data = (here / "shots" / match.group(1)).read_bytes()
    return "data:image/webp;base64," + base64.b64encode(data).decode("ascii")


text = re.sub(r"\{\{IMG:([a-z0-9.-]+)\}\}", image, text)

problems = []
if re.search(r"<!doctype|<(html|head|body)[\s>]", text, re.I):
    problems.append("the page contract forbids doctype, html, head and body tags")
if not text.startswith("<title>DNA Calc Skinning Guide</title>"):
    problems.append("the file must start with its title")
body = text.split("</style>", 1)[1]
for attr in re.findall(r'(?:fill|stroke)="(#[0-9A-Fa-f]{3,8})"', body):
    problems.append(f"an SVG colour is not a token: {attr}")
for src in re.findall(r'(?:src|href)="(https?://[^"]+)"', text):
    if "<script" in text and re.search(r'<script[^>]+src="' + re.escape(src), text) and "cdnjs.cloudflare.com" not in src:
        problems.append(f"a script comes from outside cdnjs: {src}")
if re.search(r"\{\{[A-Z]+", text):
    problems.append("an unfilled placeholder is left")
out = here / "index.html"
out.write_text(text, encoding="utf-8")
print(f"{out.name}: {len(text.encode('utf-8'))} bytes, {text.count('<section class=')} sections, "
      f"{text.count('<svg')} SVG figures, {text.count('data:image/webp')} screenshots")
for problem in problems:
    print("PROBLEM:", problem)
sys.exit(1 if problems else 0)
