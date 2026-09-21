# img-gen-via-svg-mcp

An MCP server that renders SVG to raster images — with absolute pixel sizes, a
free output path, and full SVG feature fidelity.

A single static binary, no runtime dependencies, stdio only.

## Why this server exists

The project owner surveyed the SVG-to-raster MCP servers that were available and
found that none of them satisfies all of the requirements below at the same
time:

| Server | Where it falls short |
| --- | --- |
| `mcp-svg-converter` (surferdot) | No absolute target size — scaling is expressed as a factor; output directories are whitelisted and a rejected path is silently redirected instead of refused |
| `@svg-mcp/svg-mcp` | Writes into a temporary directory; the caller cannot choose where the file lands |
| `image-processing-mcp` | General raster toolbox; SVG rendering is not its focus and SVG filter fidelity is not a stated goal |
| `svg-converter-mcp` (ppbong) | PNG/JPEG only, driven by icon-set use cases rather than by render fidelity |
| `magick-convert` | ImageMagick wrapper; SVG rendering quality depends on whichever delegate ImageMagick happens to find on the host |
| `mcp-imagemagick` | Same dependency problem, plus an ImageMagick installation as a hard runtime requirement |

[`docs/COMPARISON.md`](docs/COMPARISON.md) carries the full feature matrix and
records which tool of this server replaces which foreign feature.

This server is meant to replace all six. Every capability any of them offers is
planned here as an optional feature, on top of the five requirements that are
mandatory.

## What it does

* Takes an SVG as a **file path** (a source string or a URL works too).
* Renders at an **absolute pixel size** — `width` and `height` in pixels, not a
  scale factor. A scale factor is available as an alternative.
* Fits the drawing into that size without distorting or cropping it, and says
  exactly what fills the remaining border — transparent by default, a colour on
  request, and white on the formats that have no alpha channel.
* Writes **wherever the caller says**. A directory allowlist exists, but only
  when the operator configures one, and a rejected path produces an error
  rather than a silent redirect.
* Encodes to **PNG, JPEG, BMP, GIF, TIFF, WebP, ICO, TGA, QOI, AVIF, PNM,
  Farbfeld, OpenEXR and HDR**, plus ICNS for multi-resolution icons.
* Renders the **whole of static SVG 1.1** — filters (blur, turbulence/grain,
  displacement, morphology, convolution, lighting, drop shadow, compositing),
  gradients, masks, clip paths, patterns, opacity, blend modes and text.
  Whatever is not supported is named explicitly in the specification and
  reported at runtime; this server never renders a missing feature silently.

## The tools

| Tool | Purpose |
| --- | --- |
| `render_svg` | Render one SVG to one raster image. The core tool. |
| `render_svg_batch` | Render one SVG to many sizes and formats in a single call. |
| `render_icon` | Build a multi-resolution `.ico`, `.icns` or PNG icon set. |
| `probe_svg` | Report an SVG's intrinsic size, viewBox, feature inventory, font requirements and anything unsupported — without rendering. |
| `optimize_svg` | Normalise and shrink an SVG. |
| `convert_image` | Convert a raster image between formats and between files and Base64. |
| `get_capabilities` | Report supported formats, the SVG feature matrix, known gaps, loaded fonts and the active configuration. |

Full parameter schemas, defaults and error behaviour are in
[`docs/SPEC.md`](docs/SPEC.md).

## Building and installing

Rust 1.88 or newer:

```
cargo build --release
```

The binary lands at `target/release/img-gen-via-svg-mcp`. Register it as a
command in your client's configuration — an `mcp.yaml` entry, or
`claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "img-gen-via-svg": {
      "command": "/usr/local/bin/img-gen-via-svg-mcp",
      "args": [],
      "env": { "IMG_SVG_MCP_ALLOWED_OUTPUT_DIRS": "/home/you/render" }
    }
  }
}
```

That environment variable is optional. Without it the caller may write anywhere
the process can, which is the point: the caller chooses the path.
[`docs/SPEC.md`](docs/SPEC.md) §7 lists every setting.

Cargo features: `icns`, `png-optimize` and `remote` are on by default; `avif`
is off, because its encoder costs minutes of build time for a format nothing
here requires.

## An example

```json
{
  "svg_path": "/home/you/art/logo.svg",
  "output_path": "/home/you/render/logo.png",
  "width": 512,
  "height": 512
}
```

The result says what happened, and what it cost:

```json
{
  "output_path": "/home/you/render/logo.png",
  "format": "png", "width": 512, "height": 512, "bytes": 48213,
  "has_alpha": true,
  "source": { "width": 100.0, "height": 80.0, "size_origin": "attributes" },
  "fit": "contain",
  "padding_px": { "top": 51, "right": 0, "bottom": 51, "left": 0 },
  "warnings": [
    { "code": "aspect_adjusted",
      "message": "The document's aspect ratio 1.2500 differs from the requested 1.0000; fit contain was applied." }
  ]
}
```

**Read the `warnings` array.** It is empty when the image is exactly what the
document says. When it is not, something was substituted, dropped or adjusted,
and the entry names which. That is the difference between this server and the
six it replaces.

## Seeing it work

`dev/demos` holds 31 documents covering the filter primitives, the paint and
compositing model, text, structure, and every gap this renderer has. Rendering
them and building a page that puts each next to the browser's own rendering:

```
cargo build --release
python dev/demos/render_demos.py
python dev/demos/build_compare.py
python -m http.server 8731 --directory dev/demos   # then open /compare.html
```

## Implementation stack

Rust, producing a single static binary with no runtime dependencies, for
x86-64 Linux and x86-64 Windows:
[`resvg`/`usvg`/`tiny-skia`](https://github.com/linebender/resvg) for rendering,
the [`image`](https://github.com/image-rs/image) crate for encoding, and
[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) — the official Rust
MCP SDK — for the protocol. The transport is stdio and only stdio, so the
server installs as a command entry in an `mcp.yaml` or a
`claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "img-gen-via-svg": {
      "command": "/usr/local/bin/img-gen-via-svg-mcp",
      "args": []
    }
  }
}
```

[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) explains the choices.

## As a Claude Code plugin

This repository is also a Claude Code plugin, catalogued in the `derdere`
marketplace:

```
/plugin marketplace add derDere/MyClaudeMarked
/plugin install img-gen-via-svg@derdere
```

The plugin carries no binary. On first start its launcher looks for one that is
already on the machine and otherwise downloads the release artefact for the
platform; `/img-gen-via-svg:setup` builds one from these sources when neither
applies. [`docs/PLUGIN.md`](docs/PLUGIN.md) explains the arrangement and what a
user on an empty machine has to do.

## Documentation

| Document | Contents |
| --- | --- |
| [`docs/SPEC.md`](docs/SPEC.md) | The specification: requirements, every tool with its full parameter schema, the error model, configuration, the fidelity contract |
| [`docs/COMPARISON.md`](docs/COMPARISON.md) | Feature matrix against the six surveyed servers |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Stack decision and its rationale, module layout, pinned dependency versions |
| [`docs/TESTPLAN.md`](docs/TESTPLAN.md) | How feature fidelity is proven: the corpus, the assertions, the no-silent-gap tests, and what is not covered yet |
| [`docs/PLUGIN.md`](docs/PLUGIN.md) | The Claude Code plugin: its files, how the binary reaches a user's machine, and how to publish a release |
| [`docs/OPEN_QUESTIONS.md`](docs/OPEN_QUESTIONS.md) | Decisions still to be made, each with a recommendation |

## Tests

```
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

102 of them: unit tests per module, tool-level tests against the demo corpus,
and a protocol test that drives the built binary over real stdio.
