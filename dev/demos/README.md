# Demo corpus

Thirty SVG documents, each written to exercise one thing the specification
claims, and the machinery to render them through the server and compare the
result against a browser's own rendering.

The corpus doubles as the fixture set for `tests/tools.rs`, so a document here
is not decoration: removing one breaks a test.

## What is here

| Path | Contents |
| --- | --- |
| `svg/` | The documents. `filter-*` cover the filter primitives, `gradient-*`, `mask-*`, `clip-path`, `pattern`, `opacity-blend` the paint and compositing model, `text-*` the text model, `structure-use-symbol` and `image-embedded` the structural features, `viewbox-aspect` the fit strategies, and `gaps-*` the constructs this renderer does not reproduce. |
| `out/` | The PNGs the server produced, 480×480 each. |
| `report.json` | What the server reported per document: intrinsic size, feature inventory, fonts, warnings. |
| `compare.html` | Each document side by side with the server's rendering, with the warnings underneath. |
| `sheet.html` | The same as a contact sheet, for scanning all thirty at once. |

## Running it

```
cargo build --release
python dev/demos/render_demos.py
python dev/demos/build_compare.py
```

`render_demos.py` speaks MCP over stdio to the built binary, one `probe_svg` and
one `render_svg` call per document, so it exercises the same path a client does
rather than reaching into the library. `build_compare.py` turns the report into
the two pages.

To look at the pages in a browser, serve the folder rather than opening the
files directly, because a browser will not load the images from a `file:` URL:

```
python -m http.server 8731 --directory dev/demos
```

## Reading the comparison

The chequerboard behind an image is the page showing through wherever the image
is transparent. `viewbox-aspect` is the document to look at for that: it is
100×80, rendered into a 480×480 canvas, and the transparent bands above and
below it are what `fit: "contain"` produces.

Three documents are expected to differ from the browser, and each says so in its
warnings:

* `gaps-*` — the constructs the renderer does not reproduce. The browser draws
  the animation, the HTML in the `foreignObject` and the CSS custom property;
  the server draws the initial state, nothing, and the unresolved value, and
  names each one.
* `filter-displacement` — the renderer applies `feDisplacementMap`'s `scale`
  twice, so the document's content is displaced off its own filter region and
  the result is empty. The warning names the defect and the value that works
  around it.
* `text-missing-font` — the document asks for a font nobody has. The browser
  falls back; this renderer draws no text at all, and the warning says so.

Everything else should match closely. Lighting and morphology differ by a few
percent in places, which is the difference between two implementations of the
same equations rather than a lost feature.
