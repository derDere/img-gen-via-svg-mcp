# Specification — img-gen-via-svg-mcp

An MCP server that renders SVG documents to raster images. This document is the
contract an implementation has to satisfy. It is normative.

## 1. Conventions

* **MUST** — mandatory. An implementation that does not do this is not a
  conforming implementation. Mandatory requirements carry IDs `M1`…`M5`.
* **MAY** — optional. Planned, specified, and expected in the finished server,
  but a build without it is still conforming. Optional requirements carry IDs
  `O1`…`O13`.
* **PENDING** — a decision the project owner has not taken yet. Every such place
  carries an ID `Q-A`…`Q-D`, is marked in the text, and is listed in
  [`OPEN_QUESTIONS.md`](OPEN_QUESTIONS.md). Each has a provisional default so
  that implementation is not blocked; the provisional default is not a decision.
* Sizes are in pixels and are integers unless stated otherwise.
* All JSON field names are `snake_case`.
* "The caller" is the MCP client, usually an AI agent. "The operator" is the
  person who runs the server process and controls its configuration.

## 2. Requirements

### 2.1 Mandatory (MUST)

| ID | Requirement |
| --- | --- |
| **M1** | The server accepts an SVG as a **file path**. The path is the primary input form and is always available. |
| **M2** | The server accepts an **absolute target size in pixels** (`width` and `height`), not a scaling factor. The absolute size is the primary way of specifying output dimensions. A scale factor may exist in addition (`O2`) but never replaces it. |
| **M3** | The target format is freely selectable and covers at least **PNG, JPEG and BMP**, plus every further format the encoder offers without contortions. |
| **M4** | The caller determines the **output path**. No temporary directory, no forced allowlist, and above all no silent redirection. A directory allowlist exists only when the operator configures one, and a path outside it is rejected with an explicit error (`path_not_allowed`). |
| **M5** | **Full SVG feature fidelity.** Everything an SVG can express has to arrive in the raster image: filters (`feGaussianBlur`, `feTurbulence`, `feDisplacementMap`, `feMorphology`, `feConvolveMatrix`, `feDiffuseLighting`, `feSpecularLighting`, `feDropShadow`, `feComposite`, `feBlend`, `feMerge`, `feFlood`, `feImage`, `feTile`, `feOffset`, `feColorMatrix`, `feComponentTransfer`), gradients, masks, clip paths, patterns, opacity, blend modes, and text with fonts. Any feature that is not supported is named as a known gap in §9 **and** reported to the caller at runtime as a warning or an error. Rendering something wrong without saying so is the worst failure this project can have. |

M5 is the requirement the project is measured against. §9 defines the fidelity
contract that discharges it, and [`TESTPLAN.md`](TESTPLAN.md) defines how it is
proven.

### 2.2 Optional (MAY)

| ID | Requirement | Replaces |
| --- | --- | --- |
| **O1** | SVG source passed directly as a string, and as an HTTP(S) URL | surferdot, @svg-mcp |
| **O2** | Scale factor as an alternative to the absolute size | surferdot |
| **O3** | Automatic size derivation from `width`/`height`/`viewBox` when nothing is given | surferdot, @svg-mcp |
| **O4** | Background colour; transparency forced on or off | surferdot |
| **O5** | JPEG quality, PNG compression level, DPI/density | surferdot, image-processing-mcp |
| **O6** | Result returned as Base64 / as MCP image content instead of, or in addition to, a file | @svg-mcp |
| **O7** | Multi-resolution icons: ICO and ICNS | ppbong |
| **O8** | SVGO-style optimisation | image-processing-mcp |
| **O9** | Base64 ↔ image conversion, raster format conversion | image-processing-mcp |
| **O10** | Batch: several sizes and formats from one SVG in one call | — (icon sets) |
| **O11** | Aspect-ratio strategy as a parameter (fit, stretch, fill-and-crop, refuse) | — |
| **O12** | Sub-element export by `id`, and export of the tight drawing bounding box | resvg CLI parity |
| **O13** | Stretch goal, recorded as an idea only: animated SVG (SMIL) → GIF/APNG. See §10. | — |

[`COMPARISON.md`](COMPARISON.md) carries the full matrix of these capabilities
against the six servers this one replaces, and names the tool that takes over
each of them.

## 3. Transport and protocol

The server speaks MCP over **stdio**, and over stdio only. There is no HTTP
transport and no SSE transport. A single transport keeps the process model
trivial: the server is a command that a client starts, talks to over the pipe
pair, and stops.

It therefore registers as a command entry in the client's configuration — an
`mcp.yaml` entry or a `claude_desktop_config.json` entry:

```json
{
  "mcpServers": {
    "img-gen-via-svg": {
      "command": "/usr/local/bin/img-gen-via-svg-mcp",
      "args": [],
      "env": { "IMG_SVG_MCP_ALLOWED_OUTPUT_DIRS": "/home/phillip/render" }
    }
  }
}
```

Nothing beyond that is required of the client.

* Protocol logging goes to `stderr`, never to `stdout` — `stdout` carries the
  MCP framing.
* Tool results are returned as structured content (a JSON object matching the
  output schema in each tool's section) plus a short human-readable text block.
  When `return_mode` requests inline image data, one MCP `image` content block
  per produced image is attached in addition.

## 4. Common concepts

These concepts are shared by several tools. Each tool's section says which of
them apply.

### 4.1 Input source

Exactly one of the following three parameters must be present. Zero or more than
one is an `invalid_input` error; the error message names which were given.

| Parameter | Type | Requirement | Semantics |
| --- | --- | --- | --- |
| `svg_path` | string | **M1** | Filesystem path to an `.svg` or gzip-compressed `.svgz` file. Absolute, or relative to the server process's working directory. Subject to the input allowlist (§7.2). |
| `svg_source` | string | O1 | The SVG document itself. May be a full document or a fragment with an `<svg>` root. |
| `svg_url` | string | O1 | `http://` or `https://` URL. Only usable when the operator has enabled remote input (§7.3); otherwise `remote_disabled`. |

`resources_dir` (string, optional) sets the base directory for resolving
relative references inside the SVG (`<image href="logo.png">`, `@font-face`
sources). Its default is the directory containing `svg_path`; for `svg_source`
and `svg_url` there is no default and relative references cannot be resolved,
which produces an `unresolved_reference` warning.

### 4.2 Size model

This is the heart of **M2**. Sizing is resolved in this order, and the first
matching rule wins:

1. **`width` and `height` both given** — the output is exactly `width` ×
   `height` pixels. The `fit` strategy (§4.3) decides how the SVG is mapped onto
   that canvas.
2. **Only `width` given** — the output is `width` pixels wide; the height
   follows from the source aspect ratio, rounded up.
3. **Only `height` given** — mirror image of rule 2.
4. **Only `scale` given** (O2) — the output is the source size multiplied by
   `scale`, rounded up.
5. **Nothing given** (O3) — the source size is used, derived in this order:
   the `width`/`height` attributes of the root `<svg>`; failing that, the
   `viewBox`; failing that, `default_size` from the configuration (default
   100 × 100), which also raises a `size_fallback` warning.

`width`, `height` and `scale` are mutually constrained: giving `scale` together
with `width` or `height` is an `invalid_input` error rather than a silent
precedence rule, because a silent precedence rule is exactly the kind of
surprise M2 exists to prevent.

Percentage root dimensions (`width="100%"`) do not yield a size; they fall
through to the `viewBox` and then to `default_size`.

Limits: `width` and `height` are each in `1..=65535`, and `width * height` must
not exceed `max_pixels` from the configuration (default 100 000 000, about
10 000 × 10 000). Exceeding either is a `size_limit_exceeded` error, never a
silent clamp.

### 4.3 Aspect-ratio strategy (`fit`, O11)

Applies only when both `width` and `height` are given and the requested aspect
ratio differs from the source's. The output canvas is always exactly `width` ×
`height` pixels.

| Value | Behaviour |
| --- | --- |
| `contain` | **Default.** Uniform scale so that the whole drawing fits inside the canvas. Nothing is distorted and nothing is lost. The area the drawing does not occupy is the *padding*, and §4.6 defines what fills it — transparent by default. |
| `stretch` | Non-uniform scale. The drawing fills the canvas completely and is distorted. No padding. |
| `cover` | Uniform scale so that the canvas is completely covered. Whatever sticks out is cropped. No padding. |
| `error` | Refuse. A requested size whose aspect ratio differs from the source's produces an `aspect_mismatch` error. For callers that would rather fix their numbers than receive a compromise. |

`align` (string, default `center`) positions the drawing for `contain` and
`cover`. Permitted values: `center`, `top`, `bottom`, `left`, `right`,
`top-left`, `top-right`, `bottom-left`, `bottom-right`. It has no effect for
`stretch` and `error`.

The root element's own `preserveAspectRatio` is **not** consulted for this. The
`fit` parameter describes the mapping from the SVG's user space onto the
requested canvas and overrides whatever the document would do on its own; this
keeps the tool's behaviour predictable across documents.

`contain` is the default because it is the only strategy that neither distorts
the drawing nor discards part of it. A caller who wants distortion or cropping
asks for it; a caller who did not want it cannot undo it once the pixels are
written.

Whenever `fit` actually takes effect — that is, whenever the requested and the
source aspect ratio differ — the result carries an `aspect_adjusted` warning
naming the strategy applied, the resulting scale factors and, for `contain`, the
padding in pixels on each side. No caller is surprised by letterboxing or
cropping.

### 4.4 Output target

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `output_path` | string | — | Where the file is written. Required unless `return_mode` is `image`. Absolute, or relative to the server's working directory. **M4**: the path is used as given, or the call fails. |
| `overwrite` | boolean | `true` | **PENDING (Q-A).** When `false`, an existing target produces an `output_exists` error. Provisional default `true`, because agents re-render the same target repeatedly while iterating. |
| `create_dirs` | boolean | `false` | When `true`, missing parent directories are created. When `false`, a missing parent is an `output_unwritable` error. |
| `return_mode` | string | `file` | `file` — write only. `image` — return inline image content only, no file, `output_path` must be absent. `both` — write and return. (O6) |
| `max_inline_bytes` | integer | `5242880` | Guard for `image` and `both`. A larger result raises an `inline_too_large` error for `image`; for `both` the file is still written and the result carries an `inline_omitted` warning instead of the image block. |

Writes are atomic: the implementation writes to a temporary file in the target
directory and renames it into place, so a failed encode never leaves a truncated
image behind.

### 4.5 Format selection

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `format` | string | derived | The output format. When absent it is derived from the extension of `output_path`; when there is no `output_path` or the extension is unknown, it is `png`. |

Accepted values, and what each one can carry:

| `format` | Extension | Alpha | Notes |
| --- | --- | --- | --- |
| `png` | `.png` | yes | Default. 8-bit RGBA. |
| `jpeg` | `.jpg`, `.jpeg` | no | No alpha channel. Transparency and `contain` padding are flattened per §4.6 — against `background`, or against white when none is given, with an `alpha_flattened` warning. |
| `bmp` | `.bmp` | yes | Written as a 32-bit BITMAPV4 bitmap with a real alpha channel when alpha is present, so `contain` padding stays transparent. See §4.6 for writing an opaque BMP instead. |
| `gif` | `.gif` | 1-bit | Palette of at most 256 colours, chosen by the encoder; every GIF raises a `color_quantized` warning. Alpha is reduced to fully transparent or fully opaque. |
| `tiff` | `.tif`, `.tiff` | yes | Written with the encoder's own compression, which is not selectable. |
| `webp` | `.webp` | yes | **Lossless only.** The encoder in use has no lossy WebP path; `jpeg_quality` is ignored for WebP and a `lossless_only` warning is raised if it was set. |
| `ico` | `.ico` | yes | Single image here; multi-resolution icons belong to `render_icon`. Each dimension must be ≤ 256. |
| `tga` | `.tga` | yes | |
| `qoi` | `.qoi` | yes | |
| `avif` | `.avif` | yes | Only in a build with the `avif` Cargo feature, which is off by default because the encoder is slow to compile. Encoding only; there is no AVIF decoder. `get_capabilities` reports whether this build has it. |
| `pnm` | `.pnm`, `.ppm`, `.pgm`, `.pbm` | no | Flattened per §4.6. |
| `farbfeld` | `.ff` | yes | |
| `openexr` | `.exr` | yes | 32-bit float; the 8-bit render is converted. |
| `hdr` | `.hdr` | no | Flattened per §4.6. |

`icns` is not in this list: it is a container of several images and is produced
by `render_icon` only.

An unknown `format`, or a `format` that contradicts the `output_path` extension,
is an `invalid_input` error. The extension is never silently corrected.

### 4.6 Background, transparency and padding (O4)

`fit: "contain"` is the default, so padding is the normal case rather than the
exceptional one, and three of the formats this server writes have no alpha
channel at all. What fills the padding, and what happens when the format cannot
express transparency, is therefore defined exhaustively here.

**Parameters**

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `background` | string or null | `null` | A CSS colour: `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, `hsl()`, or an SVG named colour. Fills the entire canvas — the padding and the area behind the drawing alike — before the SVG is composited onto it. `null` means a transparent canvas. |
| `padding_color` | string or null | `null` | Fills the padding region only, leaving the area behind the drawing untouched. `null` means the padding shows whatever `background` put there. Has an effect only with `fit: "contain"`. |
| `transparent` | boolean or null | `null` | `true` forces an alpha channel. `false` forces an opaque result. `null` lets the format decide. |

**Terms.** The *canvas* is the output bitmap, exactly `width` × `height` pixels.
The *drawing area* is the part of the canvas the scaled SVG occupies. The
*padding* is the canvas minus the drawing area; it is empty for every `fit`
strategy other than `contain`, and it is empty for `contain` too when the aspect
ratios happen to match.

**Composition order.** Each step happens exactly once, in this order:

1. The canvas is created fully transparent.
2. If `background` is set, the whole canvas is filled with it.
3. If `padding_color` is set, the padding region is filled with it, over
   whatever step 2 left there.
4. The SVG is rendered into the drawing area and composited onto the canvas.
5. If the result has to be opaque — see the flattening rule below — the whole
   canvas is composited onto the *flatten colour*.

**The flattening rule.** The result has to be opaque when the target format has
no alpha channel (`jpeg`, `hdr`, `pnm`), or when `transparent` is `false`. In
that case the flatten colour is:

* `background`, when `background` is set and fully opaque;
* `background` composited onto white, when `background` is set with partial
  alpha;
* **white (`#ffffff`)**, when `background` is not set.

Flattening always raises an `alpha_flattened` warning naming the colour used, so
a caller who expected transparency finds out from the result rather than from
the image.

**The combinations, in full.** "Alpha-capable" means `png`, `bmp`, `gif`, `tiff`,
`webp`, `ico`, `tga`, `qoi`, `avif`, `farbfeld`, `openexr`; "alpha-less" means
`jpeg`, `hdr`, `pnm`.

| Format | `transparent` | `background` | Padding | Behind the drawing |
| --- | --- | --- | --- | --- |
| alpha-capable | `null` or `true` | `null` | transparent | transparent |
| alpha-capable | `null` or `true` | a colour | that colour | that colour |
| alpha-capable | `null` or `true` | `null`, `padding_color` set | `padding_color` | transparent |
| alpha-capable | `false` | `null` | white | white |
| alpha-capable | `false` | a colour | that colour | that colour |
| alpha-less | `null` or `false` | `null` | **white**, with `alpha_flattened` | white |
| alpha-less | `null` or `false` | a colour | that colour, with `alpha_flattened` | that colour |
| alpha-less | `true` | any | — | `unsupported_format` error naming the format |

Two consequences are worth stating outright, because they are the two questions
a caller actually has:

* **BMP keeps transparency.** BMP is written as a 32-bit BITMAPV4 bitmap with a
  real alpha channel when alpha is present, so it behaves like PNG here.
  Consumers that only understand 24-bit BMP will see the padding as black; a
  caller writing BMP for such a consumer sets `background` or
  `transparent: false` and gets white.
* **JPEG cannot.** A JPEG always comes out opaque. Without `background` the
  padding is white and the result says so.

**Padding in a colour on an alpha-capable format** is `padding_color`. Setting
`background` instead also colours the padding, but it colours the area behind
the drawing as well, which matters whenever the SVG is itself partly
transparent. `padding_color` is the parameter for letterbox bars;
`background` is the parameter for a backdrop.

`background` with an alpha component below 1 is composited onto transparency,
not onto white, except where the flattening rule above applies.

### 4.7 Encoder options (O5)

| Parameter | Type | Range | Default | Applies to |
| --- | --- | --- | --- | --- |
| `jpeg_quality` | integer | 1–100 | `90` | `jpeg` |
| `png_compression` | string | `fast`, `default`, `best`, `none`, or `level:0`…`level:9` | `default` | `png` |
| `png_optimize` | boolean | — | `false` | `png` — runs a lossless post-optimisation pass over the encoded file. Slower; typically 10–30 % smaller, and the decoded pixels are identical. |
| `dpi` | number | 1–5000 | `96` | Unit resolution while parsing the SVG. Affects how `mm`, `cm`, `in` and `pt` lengths resolve to user units, and therefore the derived source size. |
| `density_metadata` | number or null | 1–5000 | `null` | Physical resolution recorded in the output file's metadata, in DPI. Independent of `dpi`. PNG carries it in a `pHYs` chunk and JPEG in its JFIF header; every other format raises a `metadata_unsupported` warning. |

An encoder option given for a format it does not apply to is reported rather
than ignored: `jpeg_quality` on a PNG raises `option_ignored`, and on a WebP it
raises `lossless_only`, because the WebP encoder here has no lossy path at all.

TIFF compression and GIF dithering are not selectable. The encoders decide, and
the results are deterministic; there is no parameter that pretends otherwise.

`dpi` and `density_metadata` are deliberately separate. `dpi` changes what gets
rendered; `density_metadata` changes only what a downstream program reads out of
the file.

### 4.8 Text and fonts

Text is the part of SVG with the largest gap between "rendered" and "rendered
correctly", because the result depends on fonts that exist outside the document
— and the binary runs on both Linux and Windows, where the installed fonts are
not the same. Fonts are therefore the one place where this server's output is
not automatically identical across platforms, and the rules below exist to make
that controllable rather than surprising.

The renderer uses no system text stack of its own. It resolves fonts from a font
database that the server builds at start-up and that the caller may extend per
call.

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `fonts.default_family` | string | `Times New Roman` | Family used when the SVG sets no `font-family`. |
| `fonts.serif_family` | string | from configuration | Resolution of the generic `serif` family. |
| `fonts.sans_serif_family` | string | from configuration | Resolution of `sans-serif`. |
| `fonts.cursive_family` | string | from configuration | Resolution of `cursive`. |
| `fonts.fantasy_family` | string | from configuration | Resolution of `fantasy`. |
| `fonts.monospace_family` | string | from configuration | Resolution of `monospace`. |
| `fonts.extra_dirs` | string[] | `[]` | Additional font directories for this call, loaded on top of the start-up database. |
| `fonts.extra_files` | string[] | `[]` | Additional font files for this call. |
| `fonts.skip_system_fonts` | boolean | from configuration | When `true`, the system font directories are ignored and only `fonts.extra_dirs`, `fonts.extra_files` and the operator's configured font paths are used. |
| `on_missing_font` | string | `warn` | `warn` — render what can be rendered and report the rest. `error` — refuse to render with a `font_missing` error naming every unavailable family. |
| `languages` | string[] | `["en"]` | Resolves the `systemLanguage` conditional attribute. |
| `text_rendering` | string | `optimize_legibility` | `optimize_speed`, `optimize_legibility`, `geometric_precision`. |

#### 4.8.1 Where fonts come from

At start-up the server loads, in this order:

1. The platform's system font directories, unless `skip_system_fonts` is set.
   On Linux that is the fontconfig search path — `/usr/share/fonts`,
   `/usr/local/share/fonts`, `~/.fonts`, `~/.local/share/fonts`. On Windows it
   is `%WINDIR%\Fonts` and the per-user font directory under
   `%LOCALAPPDATA%\Microsoft\Windows\Fonts`.
2. Every path in the operator's `font_dirs` and `font_files` configuration
   (§7.4).
3. Per call, `fonts.extra_dirs` and `fonts.extra_files`.

Later entries win over earlier ones when two faces carry the same family, style
and weight, so a caller can override a system font by supplying its own file.
`fonts.extra_dirs` and `fonts.extra_files` are subject to the input allowlist
(§7.2).

Generic families resolve differently per platform and are reported by
`get_capabilities`. The operator pins them in configuration when a fixed result
matters.

#### 4.8.2 A missing font loses the text, and says so

When a text element names a `font-family` that the database cannot supply, the
renderer draws **nothing** for that text. It does not fall back to another
family: the text is simply absent from the image. The result therefore carries a
`font_missing` warning naming the family and stating that consequence.

This is the single most consequential gap in the fidelity contract, because a
document whose text has vanished still looks like a valid image.

The default family — the one used by text that names no family of its own — is a
separate case and does substitute. When the configured `default_family` is not in
the database, an available family is used in its place and the result carries a
`font_substituted` warning naming both.

A caller that cannot tolerate either sets `on_missing_font: "error"`, or calls
`probe_svg` first and reads `fonts_requested[].resolved`.

#### 4.8.3 Reproducible text across Linux and Windows

Byte-identical output on both platforms requires that both see the same fonts.
The recipe is:

```json
{
  "fonts": {
    "skip_system_fonts": true,
    "extra_dirs": ["./assets/fonts"]
  }
}
```

With `skip_system_fonts: true` and an explicit font directory, the render no
longer depends on what the host happens to have installed, and the determinism
guarantee of §8 holds across platforms. Without it, text rendering depends on
the host, and a document whose fonts resolve on one machine may substitute on
another — visibly, with a warning, but differently.

`text_to_paths` in `optimize_svg` (§5.5) is the other route: an SVG whose text
has been converted to outlines carries no font dependency at all.

### 4.9 Rendering hints

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `shape_rendering` | string | `geometric_precision` | `optimize_speed`, `crisp_edges`, `geometric_precision`. Default for elements whose `shape-rendering` is `auto`. |
| `image_rendering` | string | `optimize_quality` | `optimize_quality`, `optimize_speed`. Default for raster images embedded in the SVG. |
| `stylesheet` | string or null | `null` | A CSS stylesheet injected while resolving the document, to restyle it without editing it. |

### 4.10 Sub-element export (O12)

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `export_id` | string or null | `null` | Renders only the element carrying this `id`, with its children. An id that is not present is an `element_not_found` error. |
| `export_area` | string | `page` | `page` — the canvas keeps the document size. `object` — the canvas is the bounding box of the exported element; only meaningful with `export_id`. `drawing` — the canvas is the tight bounding box of everything drawn. |

### 4.11 Fidelity control (M5)

| Parameter | Type | Default | Semantics |
| --- | --- | --- | --- |
| `on_unsupported` | string | `warn` | `warn` — render and report every unsupported construct as a warning. `error` — refuse to render as soon as one unsupported construct is found, with an `unsupported_feature` error listing them all. |

A caller that needs a guarantee rather than a best effort sets
`on_unsupported: "error"` and gets a rendering that is either faithful or
absent. §9 defines what counts as unsupported.

## 5. Tools

### 5.1 `render_svg`

Renders one SVG to one raster image. This is the tool that discharges M1–M5.

**Input parameters**

| Parameter | Type | Required | Default | Reference |
| --- | --- | --- | --- | --- |
| `svg_path` | string | one of three | — | §4.1 |
| `svg_source` | string | one of three | — | §4.1 |
| `svg_url` | string | one of three | — | §4.1 |
| `resources_dir` | string | no | directory of `svg_path` | §4.1 |
| `output_path` | string | conditional | — | §4.4 |
| `overwrite` | boolean | no | `true` | §4.4 |
| `create_dirs` | boolean | no | `false` | §4.4 |
| `return_mode` | string | no | `file` | §4.4 |
| `max_inline_bytes` | integer | no | `5242880` | §4.4 |
| `format` | string | no | from extension, else `png` | §4.5 |
| `width` | integer | no | — | §4.2 |
| `height` | integer | no | — | §4.2 |
| `scale` | number | no | — | §4.2 |
| `fit` | string | no | `contain` | §4.3 |
| `align` | string | no | `center` | §4.3 |
| `background` | string or null | no | `null` | §4.6 |
| `padding_color` | string or null | no | `null` | §4.6 |
| `transparent` | boolean or null | no | `null` | §4.6 |
| `jpeg_quality` | integer | no | `90` | §4.7 |
| `png_compression` | string | no | `default` | §4.7 |
| `png_optimize` | boolean | no | `false` | §4.7 |
| `dpi` | number | no | `96` | §4.7 |
| `density_metadata` | number or null | no | `null` | §4.7 |
| `fonts` | object | no | `{}` | §4.8 |
| `on_missing_font` | string | no | `warn` | §4.8 |
| `languages` | string[] | no | `["en"]` | §4.8 |
| `text_rendering` | string | no | `optimize_legibility` | §4.8 |
| `shape_rendering` | string | no | `geometric_precision` | §4.9 |
| `image_rendering` | string | no | `optimize_quality` | §4.9 |
| `stylesheet` | string or null | no | `null` | §4.9 |
| `export_id` | string or null | no | `null` | §4.10 |
| `export_area` | string | no | `page` | §4.10 |
| `on_unsupported` | string | no | `warn` | §4.11 |

**Output**

```json
{
  "output_path": "/home/phillip/icons/logo.png",
  "format": "png",
  "width": 512,
  "height": 512,
  "bytes": 48213,
  "has_alpha": true,
  "source": {
    "width": 100.0,
    "height": 80.0,
    "view_box": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 80.0 },
    "size_origin": "attributes"
  },
  "fit": "contain",
  "scale_applied": { "x": 5.12, "y": 5.12 },
  "offset_applied": { "x": 0.0, "y": 51.2 },
  "background": null,
  "padding_px": { "top": 51, "right": 0, "bottom": 51, "left": 0 },
  "render_ms": 37,
  "encode_ms": 12,
  "warnings": [
    {
      "code": "aspect_adjusted",
      "message": "Source aspect ratio 1.25 differs from requested 1.0; applied fit=contain.",
      "detail": {
        "strategy": "contain",
        "source_ratio": 1.25,
        "requested_ratio": 1.0,
        "padding_px": { "top": 51, "right": 0, "bottom": 51, "left": 0 },
        "padding_fill": "transparent"
      }
    }
  ],
  "inline_image": null
}
```

`source.size_origin` is one of `attributes`, `view_box`, `default_size`, and
tells the caller where the intrinsic size came from. `padding_px` is present
whenever `fit` is `contain`; it is all zeros when the aspect ratios matched.

When `return_mode` is `image` or `both`, `inline_image` holds
`{ "mime_type": "image/png", "bytes": 48213 }` as metadata and the actual data
travels as an MCP `image` content block alongside the structured result. When
`return_mode` is `image`, `output_path` in the result is `null`.

`warnings` is always present and is an empty array when there is nothing to
report. A caller can treat a non-empty `warnings` array as "the image may not be
what the document says".

### 5.2 `render_svg_batch` (O10)

Renders one SVG to several outputs in a single call. The document is parsed once
and rendered once per output, which is what makes an icon set cheap.

**Input parameters**

| Parameter | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `svg_path` / `svg_source` / `svg_url` | string | one of three | — | §4.1 |
| `resources_dir` | string | no | as §4.1 | §4.1 |
| `defaults` | object | no | `{}` | Any `render_svg` parameter except the input-source and `output_path` ones. Applies to every entry. |
| `outputs` | object[] | **yes** | — | At least one entry, at most 64. Each entry may carry any `render_svg` parameter except the input-source ones, and overrides `defaults`. `output_path` is required in each entry unless `return_mode` is `image`. |
| `stop_on_error` | boolean | no | `false` | `false` — every entry is attempted and the failures are reported per entry. `true` — the call aborts at the first failing entry; entries already written stay written. |

The document-level parameters — `dpi`, `fonts`, `languages`, `stylesheet`,
`resources_dir`, `on_unsupported` — are taken from `defaults` only. Setting them
per entry is an `invalid_input` error, because they change the parsed document
and the point of the batch is to parse once.

**Output**

```json
{
  "parsed_once": true,
  "source": { "width": 64.0, "height": 64.0, "view_box": null, "size_origin": "attributes" },
  "succeeded": 5,
  "failed": 1,
  "results": [
    { "index": 0, "status": "ok", "output_path": "/tmp/icon-16.png", "format": "png",
      "width": 16, "height": 16, "bytes": 412, "warnings": [] },
    { "index": 1, "status": "error", "output_path": "/root/icon-32.png",
      "error": { "code": "output_unwritable", "message": "Permission denied: /root/icon-32.png" } }
  ],
  "warnings": []
}
```

The top-level `warnings` array carries document-level warnings — font
substitutions, unsupported features — which are raised once rather than per
entry. Entry-level `warnings` carry what belongs to that entry, such as
`aspect_adjusted`.

The call itself succeeds — the tool returns a normal result — even when
individual entries fail; the caller reads `failed` and the per-entry `status`.
Only a document-level failure (unparseable SVG, denied input path) makes the
whole call an error.

### 5.3 `render_icon` (O7)

Builds a multi-resolution icon from one SVG.

**Input parameters**

| Parameter | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `svg_path` / `svg_source` / `svg_url` | string | one of three | — | §4.1 |
| `output_path` | string | **yes** | — | The `.ico` or `.icns` file, or — for `container: "png_set"` — the directory that receives the PNG files. |
| `container` | string | no | from extension, else `ico` | `ico`, `icns`, `png_set`. |
| `sizes` | integer[] | no | per container, see below | The edge lengths to render. Icons are square; a non-square source is mapped according to `fit`. |
| `png_name_pattern` | string | no | `icon-{size}.png` | Only for `png_set`. Must contain `{size}`. |
| `fit` | string | no | `contain` | §4.3 |
| `background` | string or null | no | `null` | §4.6 |
| `padding_color` | string or null | no | `null` | §4.6 |
| `overwrite`, `create_dirs`, `png_compression`, `png_optimize`, `dpi`, `fonts`, `on_missing_font`, `languages`, `stylesheet`, `on_unsupported`, `resources_dir` | | no | as §4 | |

`ico` and `png_set` are the containers this server is built for. `icns` is a
nice-to-have: the target platforms are Linux and Windows, and no supported
platform consumes an ICNS file. It is specified here so that an implementation
that wants it has the contract, and it may be left out of a first release
without making that release non-conforming.

Defaults for `sizes`:

* `ico` — `[16, 24, 32, 48, 64, 128, 256]`. Every entry must be ≤ 256; larger is
  an `invalid_input` error, because the ICO format cannot express it.
* `icns` — `[16, 32, 64, 128, 256, 512, 1024]`. Only the sizes the ICNS format
  defines are accepted: 16, 32, 48, 64, 128, 256, 512, 1024. Retina variants are
  derived automatically — a 32 px image is also written as the `2x` variant of
  the 16 px slot, and so on.
* `png_set` — `[16, 32, 48, 64, 128, 256, 512]`.

Each size is rendered from the vector source at its own resolution. Sizes are
never produced by downscaling a larger raster; that is the whole point of
rendering icons from an SVG.

**Output**

```json
{
  "output_path": "/home/phillip/app.ico",
  "container": "ico",
  "entries": [
    { "size": 16, "bytes": 412 },
    { "size": 32, "bytes": 1104 }
  ],
  "files": ["/home/phillip/app.ico"],
  "bytes": 34012,
  "warnings": []
}
```

For `png_set`, `files` lists every written PNG and `output_path` is the
directory.

### 5.4 `probe_svg`

Inspects an SVG without rendering it. This is the tool a caller uses before
rendering, to find out what it is dealing with — and it is the tool that makes
the M5 fidelity contract inspectable rather than merely promised.

**Input parameters**

| Parameter | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `svg_path` / `svg_source` / `svg_url` | string | one of three | — | §4.1 |
| `resources_dir` | string | no | as §4.1 | §4.1 |
| `dpi` | number | no | `96` | Affects the reported intrinsic size. |
| `fonts` | object | no | `{}` | §4.8 — determines which font requests are reported as resolvable. |
| `languages` | string[] | no | `["en"]` | §4.8 |

**Output**

```json
{
  "size": { "width": 100.0, "height": 80.0 },
  "size_origin": "attributes",
  "view_box": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 80.0 },
  "preserve_aspect_ratio": "xMidYMid meet",
  "aspect_ratio": 1.25,
  "recommended_sizes": [
    { "width": 100, "height": 80, "label": "intrinsic" },
    { "width": 200, "height": 160, "label": "2x" }
  ],
  "content": {
    "has_text": true,
    "has_filters": true,
    "has_masks": true,
    "has_clip_paths": false,
    "has_patterns": true,
    "has_gradients": true,
    "has_raster_images": false,
    "node_count": 412
  },
  "filter_primitives": ["feGaussianBlur", "feTurbulence", "feDisplacementMap"],
  "blend_modes": ["multiply"],
  "fonts_requested": [
    { "family": "Inter", "weight": 400, "style": "normal", "resolved": true, "resolved_to": "Inter" },
    { "family": "Comic Neue", "weight": 700, "style": "normal", "resolved": false, "resolved_to": "Times New Roman" }
  ],
  "external_references": [
    { "href": "logo.png", "kind": "image", "resolved": true, "path": "/home/phillip/art/logo.png" }
  ],
  "unsupported": [
    { "code": "animation", "element": "animateTransform", "count": 3,
      "message": "SMIL animation is not rendered; the element's initial state is drawn." }
  ],
  "warnings": []
}
```

`unsupported` is the authoritative per-document answer to "will this render
faithfully". An empty `unsupported` array together with every
`fonts_requested[].resolved` being `true` means the document renders faithfully.

### 5.5 `optimize_svg` (O8)

Normalises and shrinks an SVG document.

**Input parameters**

| Parameter | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `svg_path` / `svg_source` / `svg_url` | string | one of three | — | §4.1 |
| `output_path` | string | conditional | — | Required unless `return_mode` is `source`. |
| `return_mode` | string | no | `file` | `file`, `source` — return the optimised document as a string, `both`. |
| `mode` | string | no | `normalize` | `normalize` — resolve the document through the renderer's own simplification pipeline and write it back out. `minify` — keep the document structure and only shorten it. |
| `precision` | integer | no | `8` | 1–12. Decimal places for coordinates. |
| `text_to_paths` | boolean | no | `false` | Convert text to outlines. Removes the font dependency and makes rendering reproducible anywhere; the outlines are not selectable or editable text. |
| `overwrite`, `create_dirs`, `dpi`, `fonts`, `languages`, `resources_dir` | | no | as §4 | |

`mode: "normalize"` is the strong one: it resolves CSS, inheritance, `use`
references, nested transforms and unit conversions, and drops everything the
renderer ignores. The result renders identically and is usually much smaller,
but it is not the document the author wrote — grouping, ids and editing
structure are gone. `mode: "minify"` is the conservative one: whitespace,
redundant attribute and numeric precision only, leaving the document editable.

The difference is stated here because it is the one place where this server's
optimisation genuinely differs from SVGO's, and a caller that expects SVGO's
behaviour would otherwise be surprised.

**Output**

```json
{
  "output_path": "/home/phillip/logo.min.svg",
  "mode": "normalize",
  "bytes_in": 184320,
  "bytes_out": 41022,
  "ratio": 0.223,
  "source": null,
  "warnings": [
    { "code": "structure_lost", "message": "normalize mode discarded 84 group elements and 112 ids." }
  ]
}
```

### 5.6 `convert_image` (O9)

Converts a raster image between formats, and between file and Base64. It exists
so that this server can replace the general raster tooling of
`image-processing-mcp` without a second server.

**Input parameters**

| Parameter | Type | Required | Default | Semantics |
| --- | --- | --- | --- | --- |
| `input_path` | string | one of two | — | Path to the source raster image. |
| `input_base64` | string | one of two | — | Base64-encoded source image. A `data:` URI prefix is accepted and stripped. |
| `input_format` | string | no | sniffed | Overrides format detection for `input_base64`. |
| `output_path` | string | conditional | — | Required unless `return_mode` is `base64`. |
| `return_mode` | string | no | `file` | `file`, `base64`, `both`, `image` — the last returns MCP image content. |
| `format` | string | no | from extension, else `png` | §4.5 |
| `width` | integer | no | — | Resize target. Same size model as §4.2, with the source raster's size as the intrinsic size. |
| `height` | integer | no | — | |
| `scale` | number | no | — | |
| `fit` | string | no | `contain` | §4.3 |
| `filter` | string | no | `lanczos3` | Resampling filter: `nearest`, `triangle`, `catmull_rom`, `gaussian`, `lanczos3`. |
| `background`, `padding_color`, `transparent`, `jpeg_quality`, `png_compression`, `png_optimize`, `density_metadata`, `overwrite`, `create_dirs`, `max_inline_bytes` | | no | as §4 | |

Resampling a raster is not the same operation as rendering a vector. When the
source is an SVG, `render_svg` is the correct tool and `convert_image` will
refuse an SVG input with an `invalid_input` error naming `render_svg`.

**Output**

```json
{
  "output_path": "/home/phillip/photo.webp",
  "format": "webp",
  "width": 1200,
  "height": 800,
  "bytes": 210488,
  "source": { "format": "jpeg", "width": 4000, "height": 2667 },
  "base64": null,
  "warnings": []
}
```

### 5.7 `get_capabilities`

Reports what this build can actually do. Takes no parameters.

**Output**

```json
{
  "server_version": "0.1.0",
  "renderer": { "name": "resvg", "version": "0.48.1" },
  "encoder": { "name": "image", "version": "0.25.10" },
  "transports": ["stdio"],
  "platform": "linux",
  "architecture": "x86_64",
  "formats": {
    "encode": ["png", "jpeg", "bmp", "gif", "tiff", "webp", "ico", "tga", "qoi", "pnm", "farbfeld", "openexr", "hdr"],
    "encode_containers": ["ico", "png_set", "icns"],
    "notes": {
      "webp": "lossless encoding only",
      "avif": "not built into this binary",
      "gif": "at most 256 colours, alpha reduced to fully transparent or fully opaque",
      "tiff": "written with the encoder's default compression",
      "ico": "each image at most 256×256"
    }
  },
  "svg_support": {
    "profile": "static SVG 1.1, partial SVG 2",
    "filter_primitives": ["feBlend", "feColorMatrix", "feComponentTransfer", "feComposite",
      "feConvolveMatrix", "feDiffuseLighting", "feDisplacementMap", "feDropShadow", "feFlood",
      "feGaussianBlur", "feImage", "feMerge", "feMorphology", "feOffset", "feSpecularLighting",
      "feTile", "feTurbulence"],
    "known_gaps": [
      { "code": "animation", "description": "SMIL animation is not rendered; the initial state is drawn." },
      { "code": "scripting", "description": "script elements and event attributes are ignored." },
      { "code": "interactive", "description": "a renders its children; view and cursor have no effect." },
      { "code": "css_advanced", "description": "Only a subset of CSS selectors is resolved; see SPEC.md §9.2." },
      { "code": "svg_tiny_1_2", "description": "SVG Tiny 1.2 specific features are not supported." },
      { "code": "foreign_object", "description": "foreignObject content is not rendered." },
      { "code": "filter_unsupported", "description": "An unresolvable filter drops its element, per the SVG specification." },
      { "code": "filter_displacement_scale", "description": "feDisplacementMap's scale is applied twice by this renderer." },
      { "code": "font_missing", "description": "Text whose font family is absent is not rendered at all." },
      { "code": "font_substituted", "description": "Text that names no family is drawn in an available default." },
      { "code": "unresolved_reference", "description": "A referenced file or URL that cannot be resolved drops its element." },
      { "code": "parser_diagnostic", "description": "Anything else the parser or renderer reported, passed through verbatim." }
    ]
  },
  "fonts": {
    "system_fonts_loaded": true,
    "face_count": 312,
    "font_dirs": ["/usr/share/fonts", "/home/phillip/.local/share/fonts"],
    "generic_families": { "serif": "DejaVu Serif", "sans_serif": "DejaVu Sans", "monospace": "DejaVu Sans Mono" },
    "families": ["DejaVu Sans", "DejaVu Serif", "Inter", "..."]
  },
  "config": {
    "allowed_input_dirs": [],
    "allowed_output_dirs": [],
    "remote_input": false,
    "remote_svg_references": false,
    "default_fit": "contain",
    "max_pixels": 100000000,
    "max_inline_bytes": 5242880,
    "render_timeout_ms": 30000,
    "default_size": "100x100"
  }
}
```

This tool exists because M5 forbids silent failure, and a caller cannot avoid a
gap it does not know about. An agent that needs a guarantee calls
`get_capabilities` once and `probe_svg` per document.

## 6. Error model

Errors are returned as MCP tool errors — a failed result with a text block and a
structured payload — not as JSON-RPC protocol errors. A JSON-RPC error is
reserved for malformed requests and unknown tools, because clients tend to hide
its message from the user, and this server's error messages are meant to be
read.

Payload shape:

```json
{
  "error": {
    "code": "path_not_allowed",
    "message": "Output path /etc/logo.png is outside the configured allowlist.",
    "detail": {
      "path": "/etc/logo.png",
      "allowed_output_dirs": ["/home/phillip/render", "/srv/assets"]
    },
    "hint": "Choose a path under one of the allowed directories, or start the server without IMG_SVG_MCP_ALLOWED_OUTPUT_DIRS."
  }
}
```

`code` is stable and machine-readable. `message` is one sentence for a human.
`detail` carries the specifics. `hint`, when present, says what to do
differently.

| Code | Raised when |
| --- | --- |
| `invalid_input` | A parameter is missing, contradictory or out of range. Includes zero or several input sources, `scale` together with `width`/`height`, a `format` that contradicts the extension. |
| `input_not_found` | `svg_path` or `input_path` does not exist. |
| `input_unreadable` | The input exists but cannot be read. |
| `path_not_allowed` | An input or output path lies outside a configured allowlist. **Never** a redirect — see M4. |
| `remote_disabled` | `svg_url` was used, or the SVG references a remote resource, while remote access is disabled. |
| `remote_failed` | A permitted remote fetch failed; `detail` carries the status code or the transport error. |
| `parse_failed` | The SVG is not well-formed XML, or has no usable `<svg>` root. `detail` carries line and column where the parser can supply them. |
| `element_not_found` | `export_id` names an id that is not in the document. |
| `empty_render` | The resolved drawing has zero area — an empty document, or an `export_area` that collapsed. |
| `aspect_mismatch` | `fit` is `error` and the requested aspect ratio differs from the source's. `detail` carries both ratios and the sizes that would fit. |
| `size_limit_exceeded` | `width`, `height` or `width * height` exceeds a limit. Never a silent clamp. |
| `unsupported_feature` | `on_unsupported` is `error` and the document uses something that will not render. `detail` lists every finding. |
| `font_missing` | `on_missing_font` is `error` and a requested font family is not in the database. `detail` lists every unresolvable family with its weight and style. |
| `unsupported_format` | The requested format cannot be encoded by this build, or cannot carry what was asked of it — `transparent: true` for JPEG, for instance. |
| `output_exists` | `overwrite` is `false` and the target exists. |
| `output_unwritable` | The target cannot be written: missing parent with `create_dirs: false`, permissions, a full filesystem. |
| `encode_failed` | The encoder rejected the image — for example an ICO entry above 256 px. |
| `inline_too_large` | `return_mode: "image"` and the result exceeds `max_inline_bytes`. |
| `internal_error` | A bug. Always reported with the tool name and enough context to file an issue. |

Every error is also written to `stderr` with its code, so the operator can see
in the log what the caller saw.

**Partial work is never left behind.** Because writes are atomic (§4.4), a
failing call leaves no file. The one exception is `render_svg_batch` with
`stop_on_error: true`, where the entries already completed stay on disk; the
error payload lists them under `detail.completed`.

## 7. Configuration

Configuration belongs to the operator, not to the caller. It is read at start-up
from environment variables, optionally overridden by a TOML file named by
`IMG_SVG_MCP_CONFIG`. The caller can never widen it — a per-call parameter can
only be more restrictive than the configuration, never less.

### 7.1 General

| Variable | TOML key | Default | Meaning |
| --- | --- | --- | --- |
| `IMG_SVG_MCP_CONFIG` | — | unset | Path to the TOML configuration file. |
| `IMG_SVG_MCP_LOG` | `log_level` | `info` | `error`, `warn`, `info`, `debug`, `trace`. Goes to `stderr`. |
| `IMG_SVG_MCP_MAX_PIXELS` | `max_pixels` | `100000000` | Upper bound on `width * height`. |
| `IMG_SVG_MCP_MAX_INLINE_BYTES` | `max_inline_bytes` | `5242880` | Upper bound for inline image content. |
| `IMG_SVG_MCP_DEFAULT_SIZE` | `default_size` | `100x100` | Fallback intrinsic size, §4.2 rule 5. |
| `IMG_SVG_MCP_RENDER_TIMEOUT_MS` | `render_timeout_ms` | `30000` | The budget for one render. A tool that produces several images in one call — `render_svg_batch`, `render_icon` — gets this budget multiplied by the number of images it was asked for. A call that exceeds it returns an error and the server stays responsive. **PENDING (Q-C):** whether a per-call `timeout_ms` parameter joins it. |

### 7.2 Path policy (M4)

| Variable | TOML key | Default | Meaning |
| --- | --- | --- | --- |
| `IMG_SVG_MCP_ALLOWED_INPUT_DIRS` | `allowed_input_dirs` | empty | Colon-separated directories. Empty means no restriction. |
| `IMG_SVG_MCP_ALLOWED_OUTPUT_DIRS` | `allowed_output_dirs` | empty | Likewise for output paths. |
| `IMG_SVG_MCP_FOLLOW_SYMLINKS` | `follow_symlinks` | `false` | When an allowlist is configured, paths are canonicalised before the check and a symlink leading out of an allowed directory is rejected. |

An empty allowlist means the caller may read and write anywhere the process can.
That is the default, and it is deliberate: M4 says the caller chooses the path.
An operator who wants a boundary configures one, and then every rejection is an
explicit `path_not_allowed` error naming the allowed directories. There is no
configuration in which a path is silently replaced by another one.

### 7.3 Remote access

| Variable | TOML key | Default | Meaning |
| --- | --- | --- | --- |
| `IMG_SVG_MCP_REMOTE_INPUT` | `remote_input` | `false` | Allows `svg_url`. |
| `IMG_SVG_MCP_REMOTE_SVG_REFERENCES` | `remote_svg_references` | `false` | Allows the SVG itself to pull in remote images and fonts over the network. |
| `IMG_SVG_MCP_REMOTE_ALLOWLIST` | `remote_allowlist` | empty | Host patterns. Empty with remote access enabled means any host. |
| `IMG_SVG_MCP_REMOTE_MAX_BYTES` | `remote_max_bytes` | `26214400` | Size limit per fetched resource. |

Both remote switches default to off. An SVG is an active document as far as
resource loading is concerned, and a server that fetches whatever a rendered
document points at is a request-forgery tool. Turning them on is the operator's
decision.

### 7.4 Fonts

| Variable | TOML key | Default | Meaning |
| --- | --- | --- | --- |
| `IMG_SVG_MCP_FONT_DIRS` | `font_dirs` | empty | Extra font directories, loaded at start-up. |
| `IMG_SVG_MCP_FONT_FILES` | `font_files` | empty | Extra font files. |
| `IMG_SVG_MCP_SKIP_SYSTEM_FONTS` | `skip_system_fonts` | `false` | Ignore system fonts entirely. **PENDING (Q-B):** whether `false` stays the default. §4.8.1 lists the directories this covers on Linux and on Windows. |
| `IMG_SVG_MCP_DEFAULT_FAMILY` | `default_family` | `Times New Roman` | |
| — | `serif_family`, `sans_serif_family`, `cursive_family`, `fantasy_family`, `monospace_family` | platform default | Generic family resolution. The platform defaults differ between Linux and Windows; pinning them here makes the result the same on both. |

An operator who wants identical output on Linux and Windows sets
`skip_system_fonts = true`, lists the project's own fonts under `font_dirs`, and
pins the five generic families. §4.8.3 shows the equivalent per call.

### 7.5 Transport

The server has no transport configuration. It speaks MCP over stdio and is
started as a command by its client (§3).

## 8. Behavioural guarantees

1. **Determinism.** The same input, the same parameters and the same font
   database produce byte-identical output on every supported platform. This
   follows from the renderer, which uses no system graphics or text libraries.
   It is what makes the reference-image tests in [`TESTPLAN.md`](TESTPLAN.md)
   possible at all. The font database is the one part that is not identical
   across Linux and Windows by itself; §4.8.3 says how to make it so.
2. **No silent substitution.** No path, size, format, colour or font is ever
   replaced by something else without a warning in the result.
3. **No partial files.** Writes are atomic.
4. **Bounded work.** Every render is bounded by `max_pixels` and
   `render_timeout_ms`. A malicious or merely pathological SVG cannot hang the
   server.
5. **Read-only towards the input.** No tool ever modifies its input file.
   `optimize_svg` writing back over its own input is permitted but requires
   `output_path` to be stated explicitly; it is never the default.

## 9. Fidelity contract (M5)

### 9.1 What is guaranteed

The renderer implements static SVG 1.1 in full, plus parts of SVG 2, and is
verified against a public regression suite of roughly 1600 SVG-to-PNG tests.
Within that profile the following are rendered, not approximated:

* **Filter primitives** — `feBlend`, `feColorMatrix`, `feComponentTransfer`,
  `feComposite`, `feConvolveMatrix`, `feDiffuseLighting`, `feDisplacementMap`,
  `feDropShadow`, `feFlood`, `feGaussianBlur`, `feImage`, `feMerge`,
  `feMorphology`, `feOffset`, `feSpecularLighting`, `feTile`, `feTurbulence`,
  including filter chains, `filterUnits`, `primitiveUnits`, filter regions and
  `color-interpolation-filters` in both sRGB and linearRGB.
* **Paint** — linear and radial gradients with every spread method, patterns
  with their own content and transforms, solid colours, `fill-rule`,
  `stroke-dasharray`, `stroke-linejoin`, markers.
* **Compositing** — group and element opacity, `clipPath` including nested and
  `clip-rule`, `mask` including luminance and alpha masking, `isolation`, and
  the CSS blend modes.
* **Text** — text on a path, `tspan`, `dx`/`dy`/`rotate`, `textLength`,
  `letter-spacing`, `word-spacing`, writing modes, bidirectional text, and font
  features — provided the font is available (§4.8).
* **Structure** — `use`, `symbol`, `switch` with `systemLanguage`, nested
  `svg`, `preserveAspectRatio`, embedded raster images, and gzip-compressed
  `.svgz` input.

### 9.2 Known gaps

These are the gaps. They are reported by `get_capabilities`, detected per
document by `probe_svg`, and surfaced at render time as warnings — or, with
`on_unsupported: "error"`, as a refusal.

| Code | Gap | Behaviour |
| --- | --- | --- |
| `animation` | SMIL animation: `animate`, `animateTransform`, `animateMotion`, `set`. The renderer has no animation support and none is planned. | The element's initial state is drawn. Warning per animated element. See §10 for the stretch goal. |
| `scripting` | `script` elements and event attributes. | Ignored. Warning. |
| `interactive` | `a`, `view`, `cursor`. | `a` renders its children; the rest is ignored. Warning. |
| `css_advanced` | CSS support is limited to a subset of selectors. Complex selectors, `@media`, custom properties and cascade layers are not resolved. | The affected declaration does not apply. Warning naming the selector. |
| `svg_tiny_1_2` | SVG Tiny 1.2 specific elements. | Ignored. Warning. |
| `foreign_object` | `foreignObject` — arbitrary HTML inside an SVG needs a browser engine. | Not rendered. Warning. |
| `filter_unsupported` | A filter primitive outside the list in §9.1, or an unresolvable filter input. | Per the SVG specification, an element with an unresolvable filter is not rendered at all. Warning, and the warning says the element was dropped rather than drawn unfiltered. |
| `font_missing` | A text element names a font family that is not in the database. The installed families differ between Linux and Windows, so this is the gap most likely to appear on one platform and not the other. | **The text is not drawn at all**; there is no fallback to another family. `font_missing` warning naming the family. With `on_missing_font: "error"` the render is refused instead. §4.8.3 says how to take the host out of the equation. |
| `font_substituted` | The configured default family — used by text that names none of its own — is not in the database. | An available family is used in its place. Warning naming both. |
| `filter_displacement_scale` | `feDisplacementMap` in this renderer multiplies the displacement by the primitive's `scale` twice, so a document asking for `scale="38"` is displaced by about 1444 pixels and its content leaves the filter region entirely. | The image is rendered as the renderer produces it, and the warning names the defect, the effective displacement and the square-root value that produces the displacement the document asked for. §9.4 has the detail. |
| `unresolved_reference` | A referenced file or URL could not be resolved, or a document references a remote resource while remote references are disabled. | The referencing element is not drawn. Warning. |
| `parser_diagnostic` | The parser or the renderer reported something this catalogue does not classify further. | The message is passed through verbatim so that nothing the renderer says is swallowed. |

### 9.3 How the contract is enforced

1. The parser's **and the renderer's** diagnostics are captured per call rather
   than only logged, and every diagnostic becomes a structured warning in the
   result. Both stages matter: the parser reports what it could not resolve, the
   renderer reports what it could not draw, and a capture around only the first
   would miss an undecodable embedded image or a dropped filter.
2. Before parsing, the document is scanned for the constructs in §9.2 that the
   parser discards without a diagnostic — `animate*`, `set`, `script`,
   `foreignObject`, `view`, `cursor` — so that they are reported even though the
   parser is silent about them.
3. After parsing, the resolved tree is walked to collect the fonts actually used
   and compare them against what the document asked for, which is what produces
   `font_substituted`.
4. `on_unsupported: "error"` turns the collected findings into a refusal before
   anything is written.

Steps 1 and 2 together are what makes the promise in M5 keepable. A gap that is
neither in §9.2 nor reported at runtime is a bug of the highest severity in this
project, and [`TESTPLAN.md`](TESTPLAN.md) describes the tests that guard it —
including one that fails the build when a catalogue entry has no document behind
it.

### 9.4 The `feDisplacementMap` defect

The renderer computes a pixel's displacement as `channel × scale × scale`
instead of `channel × scale`. The consequence is quadratic: `scale="5"` behaves
like 25, `scale="10"` like 100, and `scale="38"` like 1444, at which point every
pixel is sampled from outside the filter region and the filtered element renders
empty.

This server does not rewrite the document to compensate. A silent rewrite would
produce a correct image today and a doubly-wrong one the day the renderer is
fixed, and quietly editing the caller's document is exactly the behaviour this
project exists to avoid. Instead:

* every `feDisplacementMap` with `|scale| > 1` raises `filter_displacement_scale`;
* the warning states the effective displacement and gives the square root of the
  requested value, which is what produces the displacement the document asks for
  as long as the defect is present;
* `on_unsupported: "error"` refuses the render outright.

A caller that wants the documented result today writes `scale="6.164"` where the
document means 38. The demo corpus carries `filter-displacement.svg` as the
standing witness for this, and a test asserts the warning is raised.

## 10. Stretch goal: animated SVG (O13)

Recorded as an idea, not specified for implementation.

Rendering SMIL animation to GIF or APNG would need a timeline the renderer does
not have: it renders a static tree, and animation support is explicitly out of
its scope. Three routes exist, in increasing order of cost:

1. **Pre-process the timeline.** Parse the SMIL declarations, compute the
   attribute values at each frame time, rewrite the document per frame, render
   each, and assemble. Feasible for simple `animate` and `animateTransform`
   declarations; hard for `animateMotion`, `begin`/`end` event timing and
   spline pacing. Roughly the work of the rest of this server.
2. **Accept a frame list from the caller** — a tool that renders N documents,
   or one document with N stylesheets, into an animation container. Cheap,
   honest, and it solves the case where the caller can generate the frames.
3. **Drive a browser engine.** Correct for the full animation model, and it
   discards the single-static-binary property that motivates the whole stack.

Route 2 would be the first step if this is ever wanted. Nothing in the current
tool surface forecloses any of the three.

## 11. Pending decisions

Three questions that shaped this specification are settled and are written into
the text above as ordinary rules: `contain` is the default aspect-ratio strategy
(§4.3) with the padding semantics of §4.6; the binary targets Linux and Windows
on x86-64, with macOS taken only where it costs nothing
([`ARCHITECTURE.md`](ARCHITECTURE.md)); and stdio is the only transport (§3).

What is left open:

| ID | Subject | Provisional default | Where |
| --- | --- | --- | --- |
| **Q-A** | Default for `overwrite` | `true` | §4.4 |
| **Q-B** | Whether system fonts are loaded by default | yes | §4.8, §7.4 |
| **Q-C** | Whether `render_svg` gets a `timeout_ms` parameter of its own | no, operator configuration only | §7.1 |
| **Q-D** | Licence for the repository | not chosen | — |

All four are carried, with a recommendation each, in
[`OPEN_QUESTIONS.md`](OPEN_QUESTIONS.md).
