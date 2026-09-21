# Test plan

## 1. What has to be proven

The mandatory requirements of [`SPEC.md`](SPEC.md) §2.1 are the test plan's
outline. Four of them are cheap to test. The fifth is the whole job.

| Requirement | Proven by |
| --- | --- |
| M1 file path input | §4 tool tests |
| M2 absolute pixel size | §5 sizing and fit tests |
| M3 free target format | §7 encoding tests |
| M4 free output path | §8 path policy tests |
| **M5 full SVG feature fidelity** | **§2, §3 and §6 — the corpus, the reference images and the no-silent-gap tests** |

A test that asserts "a PNG file appeared" proves nothing about M5 and is not
counted as a fidelity test anywhere in this plan. Every fidelity test compares
pixels against a reference image, or asserts a specific structural property of
those pixels.

## 2. The reference corpus

Two corpora, used for different things.

### 2.1 The upstream suite

[`linebender/resvg-test-suite`](https://github.com/linebender/resvg-test-suite)
carries roughly 1600 SVG-to-PNG regression tests with reference renders. It is
included as a git submodule under `tests/upstream/`.

It is the renderer's own suite, so it does not test this server. What it does is
guard the assumption the whole project rests on: that the renderer covers the
SVG features [`SPEC.md`](SPEC.md) §9.1 claims. A `cargo test --test upstream`
run renders the suite through this server's own pipeline at the suite's declared
size and compares against the suite's references.

Its real value is on the day the renderer is upgraded: a dependency bump that
breaks `feTurbulence` shows up in 1600 tests before it shows up in a user's
icon.

Failures that are known upstream limitations are listed in
`tests/upstream/known-failures.toml` with the reason and the upstream issue.
That file only shrinks; an entry may not be added without a link to the upstream
issue, because "add it to the ignore list" is exactly how a fidelity claim rots.

### 2.2 This server's own corpus

`tests/corpus/` holds SVG documents written for this server, each targeting one
feature or one interaction, each with a committed reference PNG under
`tests/corpus/reference/`. These are the documents that prove the promises of
[`SPEC.md`](SPEC.md) §9.1 at the sizes and in the formats this server offers.

The corpus is deliberately small enough to be reviewed by eye and large enough
to cover every claim.

| File | Exercises | What the assertion checks |
| --- | --- | --- |
| `filter-blur.svg` | `feGaussianBlur` at three radii, on a shape, on a group, on text | Reference match; and that the blurred edge spans the expected pixel count, so a no-op blur cannot pass |
| `filter-blur-edge.svg` | Blur crossing the filter region boundary, `filterUnits` both values | Reference match; no clipping artefact at the region edge |
| `filter-turbulence-grain.svg` | `feTurbulence type="fractalNoise"` as a film-grain overlay, with `feComposite` | Reference match; and the standard deviation of the luminance channel is above a floor, so a flat fill cannot pass as grain |
| `filter-turbulence-noise.svg` | `feTurbulence type="turbulence"`, several octaves, non-integer `baseFrequency`, explicit `seed` | Reference match; exact match required, because turbulence is the primitive most likely to differ subtly between renderer versions |
| `filter-displacement.svg` | `feDisplacementMap` driven by turbulence | Reference match |
| `filter-morphology.svg` | `feMorphology` erode and dilate | Reference match |
| `filter-convolve.svg` | `feConvolveMatrix` sharpen and emboss kernels | Reference match |
| `filter-lighting.svg` | `feDiffuseLighting` and `feSpecularLighting` with all three light sources | Reference match |
| `filter-dropshadow.svg` | `feDropShadow`, including a coloured shadow with opacity | Reference match |
| `filter-chain.svg` | A chain of six primitives with named `result` inputs, `BackgroundImage` fallbacks, `feMerge` | Reference match |
| `filter-colorspace.svg` | The same filter with `color-interpolation-filters` sRGB and linearRGB side by side | Reference match; and that the two halves differ, so a renderer ignoring the property fails |
| `gradient-linear.svg` | Linear gradients, all spread methods, `gradientTransform`, `objectBoundingBox` and `userSpaceOnUse` | Reference match |
| `gradient-radial.svg` | Radial gradients with focal points, and gradients on strokes | Reference match |
| `mask-luminance.svg` | `mask` with a gradient, luminance masking | Reference match; alpha at three sample points within one unit of the expected value |
| `mask-alpha.svg` | `mask-type="alpha"`, nested masks | Reference match |
| `clip-path.svg` | `clipPath`, nested, `clip-rule="evenodd"`, `clipPathUnits` | Reference match; hard edges stay hard |
| `pattern.svg` | `pattern` with its own viewBox, `patternTransform`, a pattern containing a filtered element | Reference match; tile phase correct at the canvas edge |
| `opacity-blend.svg` | Group and element opacity, every CSS blend mode, `isolation` | Reference match |
| `text-basic.svg` | Text in a bundled font, `tspan`, `dx`/`dy`/`rotate`, `letter-spacing` | Reference match with `skip_system_fonts` and the bundled font only |
| `text-on-path.svg` | `textPath`, `startOffset`, `textLength` | Reference match |
| `text-bidi.svg` | Right-to-left and mixed-direction text, `writing-mode` | Reference match |
| `text-filtered.svg` | Text under a blur and under a mask | Reference match — the interaction that most renderers get wrong |
| `text-missing-font.svg` | A `font-family` that is deliberately not available | **Not** a reference match: asserts that a `font_substituted` warning is present and names both families |
| `image-embedded.svg` | An embedded PNG and JPEG, `image-rendering` both values | Reference match |
| `structure-use-symbol.svg` | `use`, `symbol`, nested `svg`, `switch` with `systemLanguage` | Reference match |
| `units-dpi.svg` | Lengths in `mm`, `cm`, `in`, `pt`, rendered at 96 and at 300 dpi | Two references; asserts the size ratio is exactly 300/96 |
| `viewbox-aspect.svg` | A 100 × 80 document rendered into square canvases | One reference per `fit` strategy — this is the §5 fixture |
| `gaps-animation.svg` | `animate`, `animateTransform`, `animateMotion`, `set` | **Not** a reference match: asserts an `animation` warning per element and that the initial state was drawn |
| `gaps-foreignobject.svg` | `foreignObject` with HTML content | Asserts a `foreign_object` warning and that nothing was drawn for it |
| `gaps-script.svg` | `script` and event attributes | Asserts a `scripting` warning |
| `gaps-css.svg` | CSS beyond the supported subset | Asserts a `css_advanced` warning naming the selector |
| `gaps-interactive.svg` | `a`, `view` and `cursor` elements | Asserts an `interactive` warning, and that the children of `a` were drawn |
| `gaps-svg-tiny.svg` | SVG Tiny 1.2 specific elements | Asserts an `svg_tiny_1_2` warning |
| `gaps-broken-ref.svg` | `href` to a file that does not exist, and a filter referencing a missing result | Asserts `unresolved_reference` and `filter_unsupported` warnings, and that the affected element was dropped rather than drawn unfiltered |

Each corpus file carries a comment at the top saying which
[`SPEC.md`](SPEC.md) §9.1 claim it discharges. A claim in §9.1 with no corpus
file behind it is a gap in this plan, and §9 below is the test that catches it.

## 3. Test levels

| Level | Location | What it covers |
| --- | --- | --- |
| Unit | `#[cfg(test)]` next to each module | The size model, the fit transforms, the alpha and flatten matrix, the allowlist, format dispatch, colour parsing |
| Module | `tests/render.rs`, `tests/encode.rs` | The pipeline from bytes to pixmap and from pixmap to encoded file, without MCP |
| Fidelity | `tests/fidelity.rs` | §2.2 corpus against its references |
| Upstream | `tests/upstream.rs` | §2.1 suite |
| Tool | `tests/tools/*.rs` | Each tool's parameters, results, warnings and errors, called through the tool router with no transport |
| Protocol | `tests/protocol.rs` | The server started as a subprocess over real stdio: initialise, `tools/list`, a render, a failure, shutdown |

The module split of [`ARCHITECTURE.md`](ARCHITECTURE.md) §2 exists so that the
first four levels need no MCP client at all.

## 4. Tool tests

Per tool, at minimum:

* **`render_svg`** — each input source; every format from
  [`SPEC.md`](SPEC.md) §4.5; `output_path` honoured exactly, including a path
  with spaces and a non-ASCII path; `return_mode` in all three settings;
  `export_id` and each `export_area`; `on_unsupported` in both settings.
* **`render_svg_batch`** — one parse, many outputs; a mixed run in which one
  entry fails and the rest succeed, asserting `succeeded`, `failed` and the
  per-entry payloads; `stop_on_error` in both settings; a document-level
  parameter in an entry rejected as `invalid_input`.
* **`render_icon`** — an ICO whose entries are all present at the declared
  sizes, verified by decoding the file back and checking each frame's
  dimensions; a size above 256 rejected; a `png_set` writing exactly the
  expected filenames; each size rendered from the vector, verified by comparing
  the 16 px entry against a direct 16 px render rather than against a downscaled
  256 px one.
* **`probe_svg`** — intrinsic size from attributes, from `viewBox`, and the
  fallback; the content inventory against a document with a known feature set;
  `fonts_requested` resolved and unresolved; `unsupported` populated from the
  `gaps-*.svg` corpus.
* **`optimize_svg`** — `normalize` output renders to the same pixels as the
  input, which is the only optimisation assertion that matters; `minify` keeps
  the document parseable and smaller; `text_to_paths` removes the font
  dependency, verified by rendering the result with `skip_system_fonts: true`
  and an empty font set.
* **`convert_image`** — every decode format to every encode format; Base64 in
  and out; an SVG input rejected with the hint naming `render_svg`.
* **`get_capabilities`** — the reported format lists match what the build can
  actually encode, asserted by attempting an encode per listed format rather
  than by comparing against a hand-written list.

## 5. Sizing, fit and padding (M2, and SPEC §4.3 and §4.6)

`viewbox-aspect.svg` is 100 × 80. Rendered into 512 × 512:

| Case | Assertion |
| --- | --- |
| `fit: "contain"`, default | Canvas exactly 512 × 512; drawing 512 × 410 centred; 51 transparent rows top and bottom, verified by sampling the alpha channel; `aspect_adjusted` warning with `padding_px` `{51, 0, 51, 0}` |
| `fit: "contain"`, `align: "top"` | Padding entirely at the bottom |
| `fit: "stretch"` | Canvas exactly 512 × 512; no transparent border; a circle in the source is an ellipse in the result, verified by measuring its bounding box |
| `fit: "cover"` | Canvas exactly 512 × 512; no transparent border; content cropped left and right by the expected amount |
| `fit: "error"` | `aspect_mismatch` error; no file written |
| `width` only | 512 × 410, no padding, no warning |
| `height` only | 640 × 512 |
| `scale: 2` | 200 × 160 |
| nothing given | 100 × 80, `size_origin` `attributes` |
| `scale` together with `width` | `invalid_input` error |
| `width: 0`, `width: 70000` | `invalid_input`, `size_limit_exceeded` |
| `width: 20000, height: 20000` with the default `max_pixels` | `size_limit_exceeded`; no file written, no clamp |

The padding and flattening matrix of [`SPEC.md`](SPEC.md) §4.6 is tested row by
row as a table-driven unit test, each row asserting the colour of one padding
pixel and one pixel behind the drawing:

| Format | `transparent` | `background` | `padding_color` | Padding pixel | Pixel behind the drawing |
| --- | --- | --- | --- | --- | --- |
| png | unset | unset | unset | `00000000` | `00000000` |
| png | unset | `#ff0000` | unset | `ff0000ff` | `ff0000ff` |
| png | unset | unset | `#0000ff` | `0000ffff` | `00000000` |
| png | `false` | unset | unset | `ffffffff` | `ffffffff` |
| bmp | unset | unset | unset | `00000000` | `00000000` |
| bmp | `false` | unset | unset | `ffffffff` | `ffffffff` |
| jpeg | unset | unset | unset | white ± JPEG tolerance, `alpha_flattened` warning | white |
| jpeg | unset | `#ff0000` | unset | red ± tolerance, `alpha_flattened` warning | red |
| jpeg | `true` | any | any | `unsupported_format` error | — |

JPEG comparisons use a small per-channel tolerance because the format is lossy;
every other row is exact.

## 6. Reference images and cross-platform determinism

**Generation.** References are generated by a `cargo xtask bless` command,
never by hand, and always on the Linux CI image so that the committed PNGs come
from one known environment. Regenerating is a reviewable act: the diff shows the
changed images, and a pull request that changes a reference without explaining
why is not merged.

**Comparison.** The renderer is deterministic, so the primary comparison is
exact: the encoded PNG must be byte-identical to the reference. When it is not,
the failure report gives the number of differing pixels, the maximum per-channel
delta, the bounding box of the difference, and writes the actual image and a
difference image next to the reference so a human can look at all three.

**Tolerance.** A tolerance is available per corpus file, declared in
`tests/corpus/tolerances.toml`, and is used only for lossy formats and for
documents whose reference was produced before a deliberate renderer upgrade. The
default is zero and every non-zero entry carries a comment saying why.

**Cross-platform.** The fidelity suite runs on `ubuntu-latest` and on
`windows-latest` in CI ([`ARCHITECTURE.md`](ARCHITECTURE.md) §5.2), against the
same committed references, with the same assertions. Every corpus file that
contains text sets `skip_system_fonts: true` and loads only the fonts bundled
under `tests/corpus/fonts/`, so a divergence between the two runs is a genuine
renderer or encoder divergence and not a font-installation difference. That is
the entire point of running it twice: a Windows-only difference in text output
is the most likely way this server would quietly start producing different
images on different machines, and this test is what catches it.

`text-missing-font.svg` is the counterpart: it deliberately asks for a font that
is on neither platform and asserts the warning rather than the pixels.

## 7. Encoding tests (M3)

For every format in [`SPEC.md`](SPEC.md) §4.5:

* The written file is decodable, and its decoded dimensions equal the requested
  ones.
* The file's magic bytes match the format, so an encoder writing the wrong
  container cannot pass.
* Alpha behaves as §4.6 requires — a transparent pixel stays transparent in an
  alpha-capable format and becomes the flatten colour in the others.
* A format-specific option changes the file: `jpeg_quality` 10 versus 95
  produces a smaller file and a larger per-pixel error; `png_compression`
  `none` versus `best` produces a larger and a smaller file with identical
  decoded pixels; `png_optimize` produces a smaller file with identical decoded
  pixels; `tiff_compression` variants all decode to the same pixels.
* `density_metadata` is present in the written file for PNG, JPEG and TIFF, read
  back from the metadata, and raises `metadata_unsupported` elsewhere.
* WebP with `jpeg_quality` set raises `lossless_only` and still produces a
  lossless file, verified by decoding it and comparing against the PNG render
  of the same document for exact equality.

## 8. Path, policy and error tests (M4)

* With no allowlist configured, a file is written to an arbitrary absolute path
  and to a relative path, and lands exactly there.
* With an allowlist configured, a path inside it succeeds; a path outside it
  produces `path_not_allowed` whose `detail.allowed_output_dirs` lists the
  configuration — **and no file exists anywhere afterwards.** The test walks the
  allowed directories to assert that nothing was silently written there. This is
  the surferdot failure mode and it gets its own explicit assertion.
* `../` traversal out of an allowed directory is rejected; a symlink pointing
  out of one is rejected with `follow_symlinks: false`.
* `overwrite: false` onto an existing file produces `output_exists` and leaves
  the existing file byte-identical.
* `create_dirs: false` with a missing parent produces `output_unwritable` and
  creates nothing.
* A failure during encoding leaves no file and no temporary file in the target
  directory — asserted by listing the directory before and after.
* `svg_url` with remote input disabled produces `remote_disabled`; an SVG with a
  remote `href` and remote references disabled produces `unresolved_reference`
  and no network request, asserted with a local server that records requests.
* Every error code in [`SPEC.md`](SPEC.md) §6 has at least one test that
  produces it, asserted by code rather than by message text.

## 9. The no-silent-gap tests (M5)

These exist because the worst failure this project can have is rendering
something wrong without saying so. They are the tests that make §9.2 of the
specification enforceable rather than decorative.

1. **Every declared gap is detected.** One `gaps-*.svg` corpus file per entry in
   [`SPEC.md`](SPEC.md) §9.2, each asserting the exact warning code. A test
   enumerates the gap catalogue in the code and fails if any entry has no corpus
   file — so adding a gap to the catalogue without a test is a build failure.
2. **Every declared capability has a corpus file.** The same enumeration in the
   other direction, over the `svg_support.filter_primitives` list that
   `get_capabilities` reports: a primitive reported as supported with no corpus
   file behind it fails the build.
3. **`on_unsupported: "error"` refuses.** Each `gaps-*.svg` rendered with the
   strict setting produces `unsupported_feature`, lists every finding in
   `detail`, and writes nothing.
4. **`probe_svg` agrees with `render_svg`.** For every corpus file, the
   `unsupported` entries from `probe_svg` and the warnings from `render_svg` are
   compared; a document that probes clean must render without fidelity
   warnings, and vice versa. A disagreement between the two is a bug in exactly
   the place where a caller would be misled.
5. **A clean document is clean.** Every non-`gaps-*` corpus file renders with an
   empty `warnings` array. A stray warning on a document that renders correctly
   trains callers to ignore warnings, which defeats the whole mechanism.

## 10. Robustness and limits

* Malformed XML, an empty file, a file that is not SVG, a 200 MB SVG, an SVG
  with a 10 000-deep `use` recursion, an SVG with a billion-laughs entity
  expansion — each produces a specific error and a live server afterwards.
* A document with a pathological filter chain hits `render_timeout_ms` and
  returns an error rather than hanging; the server answers the next request.
* A `render_svg_batch` of 64 entries completes, and 65 entries is rejected.
* Memory stays bounded across 1000 sequential renders, checked by a soak test
  that is run manually before a release rather than in CI.

## 11. Running it

```
cargo test                                    # unit, module, tool, protocol
cargo test --test fidelity                    # the corpus against its references
cargo test --test upstream -- --ignored       # the 1600-test upstream suite
cargo xtask bless                             # regenerate references, Linux only
cargo xtask corpus-check                      # §9 tests 1 and 2, also run in CI
```

CI runs everything except the upstream suite on every push; the upstream suite
runs nightly and on any pull request that touches a rendering dependency,
because it is slow and its purpose is to catch dependency drift rather than
code changes.
