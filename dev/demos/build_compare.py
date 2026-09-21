#!/usr/bin/env python3
"""Build the side-by-side comparison page for the demo corpus.

Each row shows the browser's own rendering of a document next to the PNG this
server produced from it, so that a difference is visible rather than inferred.
The AI agent, or the user, opens ``compare.html`` or screenshots it.
"""

from __future__ import annotations

import html
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPORT = HERE / "report.json"
PAGE = HERE / "compare.html"
SHEET = HERE / "sheet.html"


def row(entry: dict) -> str:
    """Renders one document as a table row."""
    name = html.escape(entry["name"])
    svg = f"svg/{entry['name']}.svg"
    png = f"out/{entry['name']}.png"
    warnings = entry.get("render_warnings") or []
    unsupported = entry.get("unsupported") or []

    notes = "".join(
        f'<li><code>{html.escape(w["code"])}</code> {html.escape(w["message"])}</li>'
        for w in warnings
    )
    notes_block = f"<ul class=warn>{notes}</ul>" if notes else '<p class=clean>no warnings</p>'
    gap_block = (
        f'<p class=gap>{len(unsupported)} declared gap(s)</p>' if unsupported else ""
    )

    return f"""
    <section id="{name}">
      <h2>{name}</h2>
      <div class=pair>
        <figure><img src="{svg}" alt="{name} in the browser"><figcaption>browser</figcaption></figure>
        <figure><img src="{png}" alt="{name} rendered by the server"><figcaption>img-gen-via-svg-mcp</figcaption></figure>
      </div>
      <div class=meta>{gap_block}{notes_block}</div>
    </section>"""


def cell(entry: dict) -> str:
    """Renders one document as a contact-sheet cell."""
    name = html.escape(entry["name"])
    count = len(entry.get("render_warnings") or [])
    badge = f" &#9888;{count}" if count else ""
    return (
        f'<div class=cell><div class=name>{name}{badge}</div>'
        f'<div class=pair><img src="svg/{name}.svg"><img src="out/{name}.png"></div></div>'
    )


def main() -> int:
    if not REPORT.exists():
        print("run render_demos.py first")
        return 1
    report = json.loads(REPORT.read_text())
    sections = "\n".join(row(entry) for entry in report["documents"])

    PAGE.write_text(f"""<!doctype html>
<html lang=en>
<meta charset=utf-8>
<title>img-gen-via-svg-mcp demo comparison</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ font: 14px/1.5 system-ui, sans-serif; margin: 0; padding: 24px; background: #f4f4f5; color: #18181b; }}
  h1 {{ font-size: 20px; margin: 0 0 4px; }}
  .lede {{ margin: 0 0 24px; color: #52525b; }}
  section {{ background: #fff; border: 1px solid #e4e4e7; border-radius: 10px; padding: 14px; margin-bottom: 18px; }}
  h2 {{ font-size: 15px; margin: 0 0 10px; font-family: ui-monospace, monospace; }}
  .pair {{ display: flex; gap: 14px; }}
  figure {{ margin: 0; }}
  figcaption {{ font-size: 11px; color: #71717a; text-align: center; margin-top: 4px; }}
  img {{ width: 240px; height: 240px; display: block; border: 1px solid #e4e4e7;
         background: repeating-conic-gradient(#e9e9ec 0% 25%, #fff 0% 50%) 50% / 16px 16px; }}
  .meta {{ margin-top: 10px; }}
  ul.warn {{ margin: 0; padding-left: 18px; color: #92400e; font-size: 12px; }}
  .clean {{ margin: 0; color: #15803d; font-size: 12px; }}
  .gap {{ margin: 0 0 4px; color: #1d4ed8; font-size: 12px; }}
  code {{ background: #f4f4f5; padding: 0 3px; border-radius: 3px; }}
</style>
<h1>Demo comparison</h1>
<p class=lede>The browser's rendering of each document, next to the PNG this server produced from it at 480&times;480.
The chequerboard behind an image is the page, so it shows wherever the image is transparent.</p>
{sections}
</html>
""")
    cells = "".join(cell(entry) for entry in report["documents"])
    SHEET.write_text(f"""<!doctype html>
<html lang=en>
<meta charset=utf-8>
<title>img-gen-via-svg-mcp contact sheet</title>
<style>
  body {{ font: 12px system-ui, sans-serif; margin: 0; padding: 8px; background: #fafafa; }}
  .grid {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }}
  .cell {{ background: #fff; border: 1px solid #ddd; border-radius: 6px; padding: 5px; }}
  .name {{ font: 11px ui-monospace, monospace; margin-bottom: 4px; color: #333; }}
  .pair {{ display: flex; gap: 4px; }}
  img {{ width: 150px; height: 150px; border: 1px solid #eee;
         background: repeating-conic-gradient(#e9e9ec 0 25%, #fff 0 50%) 50% / 12px 12px; }}
</style>
<div class=grid>{cells}</div>
</html>
""")
    print(f"wrote {PAGE} and {SHEET} with {len(report['documents'])} comparisons")
    return 0


if __name__ == "__main__":
    sys.exit(main())
