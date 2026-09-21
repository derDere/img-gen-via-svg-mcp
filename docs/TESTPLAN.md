# Test plan

## 1. What has to be proven

The mandatory requirements of [`SPEC.md`](SPEC.md) §2.1 are this plan's outline.
Four of them are cheap to prove. The fifth is the whole job.

| Requirement | Proven by |
| --- | --- |
| M1 file path input | §4, every tool test |
| M2 absolute pixel size | §5, the sizing and fit tests |
| M3 free target format | §4 and §6, one encode-and-decode per format |
| M4 free output path | §7, the path policy tests |
| **M5 full SVG feature fidelity** | **§2, §8 and §9 — the corpus, the no-silent-gap tests and the browser comparison** |

A test that asserts "a PNG file appeared" proves nothing about M5 and is not
counted as a fidelity test anywhere here. Every fidelity assertion looks at
pixels, at a decoded file, or at a specific warning code.

## 2. The corpus

`dev/demos/svg` holds 31 documents, each written to exercise one thing the
specification claims. It is both the demo set a reader can look at and the
fixture set the tests run against, so a document in it is load-bearing: removing
one breaks a test.

| Group | Documents | What they cover |
| --- | --- | --- |
| Filters | `filter-blur`, `filter-turbulence-grain`, `filter-turbulence-noise`, `filter-displacement`, `filter-morphology`, `filter-convolve`, `filter-lighting`, `filter-dropshadow`, `filter-chain`, `filter-colorspace` | Every filter primitive [`SPEC.md`](SPEC.md) §9.1 claims, including turbulence as film grain and as noise, a six-primitive chain with named results, and the same blur in both interpolation colour spaces |
| Paint and compositing | `gradient-linear`, `gradient-radial`, `mask-luminance`, `mask-alpha`, `clip-path`, `pattern`, `opacity-blend` | Spread methods, focal points, gradients on strokes, luminance and alpha masking, nested clip paths with an even-odd rule, a pattern with its own viewBox and a filtered tile, group and element opacity, the CSS blend modes |
| Text | `text-basic`, `text-on-path`, `text-filtered`, `text-missing-font` | tspan offsets and rotation, letter and word spacing, text on a path with a fixed length, text under a blur and under a mask, and a family nobody has |
| Structure | `structure-use-symbol`, `image-embedded`, `viewbox-aspect` | `use`, `symbol`, a nested `svg`, `switch` with `systemLanguage`, an embedded raster at both rendering hints, and the 100×80 fixture the fit strategies are measured on |
| Declared gaps | `gaps-animation`, `gaps-foreignobject`, `gaps-script`, `gaps-css`, `gaps-interactive`, `gaps-svg-tiny`, `gaps-broken-ref` | One document per entry in the gap catalogue, so that §8 can check the catalogue against reality |

## 3. Test levels

| Level | File | What it covers |
| --- | --- | --- |
| Unit | `#[cfg(test)]` beside each module | The size model, the fit transforms, the padding and flatten matrix, colour parsing, the allowlist, atomic writes, format dispatch, encoder options, the scan, the gap catalogue, minification |
| Tool | `tests/tools.rs` | Each tool through its `run` function with real server state: parameters, results, written files, warning codes, errors |
| Protocol | `tests/protocol.rs` | The built binary as a subprocess over real stdio: initialise, `tools/list`, a render, a failure, inline image content, and that the server still answers afterwards |

The module split in [`ARCHITECTURE.md`](ARCHITECTURE.md) §2 exists so that the
first two levels need no MCP client at all.

## 4. Per-tool assertions

* **`render_svg`** — every format this build encodes writes a file that decodes
  back at exactly the requested dimensions; a source string and a file produce
  byte-identical output; `return_mode` returns an image block and refuses one
  above `max_inline_bytes`; `export_id` draws the named element and an unknown id
  is `element_not_found`; malformed input, a missing file and a disabled remote
  URL each produce their own error code.
* **`render_svg_batch`** — one parse, several outputs, with a deliberately
  failing entry in the middle: `succeeded`, `failed` and the per-entry payloads
  are all asserted, and the successful entries are decoded from disk.
* **`render_icon`** — the ICO's own header is checked for the entry count, and
  each size is compared against a direct render at that size to prove it came
  from the vector rather than from a downscale; a size above 256 is refused; a
  PNG set writes exactly the expected filenames.
* **`probe_svg`** — intrinsic size, the content inventory, the filter primitive
  list and the requested fonts, against documents with a known feature set.
* **`optimize_svg`** — `normalize` output renders to the same pixels as the
  input, which is the only optimisation assertion that matters; `minify` stays
  parseable and gets smaller; `text_to_paths` output renders with no font
  database at all and raises no `font_missing`.
* **`convert_image`** — format conversion with a resize, Base64 output, and an
  SVG input refused with a hint naming `render_svg`.
* **`get_capabilities`** — every format the report claims is then actually
  encoded, so the report cannot drift from the build, and the gap count matches
  the catalogue.

## 5. Sizing, fit and padding (M2, and SPEC §4.3 and §4.6)

`viewbox-aspect.svg` is 100×80. Rendered into 512×512:

| Case | Assertion |
| --- | --- |
| `fit: "contain"`, the default | Canvas exactly 512×512, padding 51 above and below and 0 at the sides, an `aspect_adjusted` warning, a transparent pixel sampled in the letterbox and an opaque one in the middle |
| `fit: "stretch"` | No padding, and the sampled letterbox pixel is opaque |
| `fit: "cover"` | No padding |
| `fit: "error"` | `aspect_mismatch`, and no file written |
| `width` only | 512×410 |
| `scale: 2` | 200×160 |
| `scale` with `width` | `invalid_input` |
| Beyond `max_pixels` | `size_limit_exceeded`, never a clamp |

The §4.6 matrix is asserted pixel by pixel: `padding_color` colours the padding
and leaves the drawing's own transparency, `background` fills the whole canvas,
JPEG flattens to white and raises `alpha_flattened`, JPEG with a background
flattens to that colour, and BMP keeps a transparent letterbox because it is
written with an alpha channel.

## 6. Encoder options

`jpeg_quality` 10 versus 95 produces a smaller file; `png_compression` `none`
versus `best` produces a larger and a smaller file whose decoded pixels are
identical; `png_optimize` is likewise lossless; `density_metadata` puts a `pHYs`
chunk into a PNG and raises `metadata_unsupported` for a BMP.

## 7. Path, policy and error tests (M4)

* With no allowlist, a file lands exactly at the absolute path it was given.
* With an allowlist, a path outside it produces `path_not_allowed`, **and the
  allowed directory is then read to prove nothing was silently written into it.**
  That is the failure mode this server exists to replace, so it gets its own
  assertion rather than an implied one.
* `overwrite: false` leaves the existing file byte-identical.
* `create_dirs: false` creates nothing.
* A failing render leaves the target directory completely empty — no file and no
  temporary.

## 8. The no-silent-gap tests (M5)

These make [`SPEC.md`](SPEC.md) §9.2 enforceable rather than decorative.

1. **Every declared gap has a document that provokes it.** The test walks the
   catalogue and the corpus and fails when an entry has nothing behind it, so
   adding a gap without a document is a build failure rather than an oversight.
2. **`probe_svg` and `render_svg` agree.** For each document, everything
   `probe_svg` lists as unsupported must appear as a warning from `render_svg`.
   A disagreement between them misleads a caller in exactly the place it decided
   to trust the tool.
3. **A clean document is clean.** Documents that render faithfully must produce
   an empty `warnings` array. A stray warning teaches callers to ignore warnings,
   which is what would make every other warning worthless.
4. **Strict mode refuses.** `on_unsupported: "error"` on a document with an
   animation produces `unsupported_feature` and writes nothing.
5. **A missing font is named and can be made fatal.** `text-missing-font` raises
   `font_missing`, and `on_missing_font: "error"` turns it into a refusal.
6. **The displacement defect is reported.** `filter-displacement` must raise
   `filter_displacement_scale`. The image is wrong whatever this server does
   ([`SPEC.md`](SPEC.md) §9.4); the one thing it must not do is stay quiet.

## 9. The browser comparison

`dev/demos` renders all 31 documents through the server over stdio and builds a
page showing each one next to the browser's own rendering, with its warnings
underneath. It is how a human checks what no assertion can: that the image looks
like the document.

```
cargo build --release
python dev/demos/render_demos.py
python dev/demos/build_compare.py
python -m http.server 8731 --directory dev/demos
```

`dev/demos/README.md` says which documents are expected to differ and why.

## 10. Running it

```
cargo test                                          # unit, tool and protocol tests
cargo test --test tools                             # the tool level alone
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

CI runs all of it on `ubuntu-latest` and `windows-latest`
([`ARCHITECTURE.md`](ARCHITECTURE.md) §5.2). Running the same assertions on both
is what would catch a platform divergence in text rendering, which is the most
likely way this server would quietly start producing different images on
different machines.

## 11. What this plan does not yet cover

Named here so that the gap is visible rather than assumed away.

* **Committed reference images.** The tests assert on sampled pixels, decoded
  dimensions and structural properties, not against stored renders. A change
  that shifted every gradient by one shade would pass. Adding a `bless` command
  and a set of reference PNGs, compared byte for byte, is the obvious next step.
* **The upstream regression suite.** resvg publishes roughly 1600 SVG-to-PNG
  tests as [`linebender/resvg-test-suite`](https://github.com/linebender/resvg-test-suite).
  Running them through this server's pipeline would guard the renderer
  assumption on every dependency bump; it is not wired up.
* **A soak test.** Nothing checks that memory stays bounded across a thousand
  sequential renders.
* **Pathological input.** There is no test for a billion-laughs entity
  expansion, a deeply recursive `use`, or a filter chain built to exceed the
  render timeout.
