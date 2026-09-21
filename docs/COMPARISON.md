# Comparison with existing SVG MCP servers

This server exists to replace six SVG-to-raster MCP servers with one. The table
below records what each of them offers, which of this server's tools takes over
that capability, and where each of them falls short of the mandatory
requirements in [`SPEC.md`](SPEC.md) §2.1.

## The servers

| Short name used below | Package / project | What it is |
| --- | --- | --- |
| **surferdot** | `mcp-svg-converter` (npm, 1.0.6) | Node server converting SVG to PNG and JPG |
| **svg-mcp** | `@svg-mcp/svg-mcp` (npm, 1.1.1) | Rust server for SVG-to-image conversion |
| **img-proc** | `image-processing-mcp` | General raster image toolbox with SVG input among its formats |
| **ppbong** | `svg-converter-mcp` (npm, 1.0.0) | Lightweight SVG to PNG/JPEG converter, icon-set oriented |
| **magick** | `magick-convert` | Thin wrapper around the ImageMagick `convert`/`magick` binary |
| **im-mcp** | `mcp-imagemagick` | ImageMagick-backed image server |

The capability columns reflect the project owner's survey of these servers. A
package registry lookup on 2026-09-21 resolved surferdot, svg-mcp and ppbong on
npm; img-proc, magick and im-mcp are not published under those names on npm or
PyPI and are distributed by other means. Where a cell below is marked `?`, the
capability could not be confirmed from a published package and the entry rests
on the survey alone.

## Mandatory requirements

Legend: ● full, ◐ partial or conditional, ○ absent, `?` unconfirmed.

| Requirement ([`SPEC.md`](SPEC.md) §2.1) | surferdot | svg-mcp | img-proc | ppbong | magick | im-mcp | **this server** |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **M1** SVG as a file path | ● | ● | ● | ● | ● | ● | **●** |
| **M2** Absolute target size in pixels | ○ scale factor only | ● | ◐ `?` | ◐ fixed icon sizes | ● | ● | **●** |
| **M3** Free target format incl. PNG, JPEG, BMP | ○ PNG, JPG | ◐ PNG, JPEG, WebP | ◐ `?` no BMP | ○ PNG, JPEG | ● | ● | **●** 15 formats |
| **M4** Free output path, no silent redirect | ○ allowlist with silent redirect | ○ temp directory only | ◐ `?` | ◐ `?` | ● | ● | **●** optional allowlist, explicit error |
| **M5** Full SVG feature fidelity, gaps declared | ◐ browser-class renderer, gaps undeclared | ◐ gaps undeclared | ◐ `?` | ◐ `?` | ◐ depends on the host's SVG delegate | ◐ depends on the host's SVG delegate | **●** resvg, gaps enumerated and reported per call |

**Where each one fails.**

* **surferdot** fails M2 outright: size is expressed as a scale factor, so a
  caller that needs a 512 × 512 icon has to compute a factor from the
  document's intrinsic size first. It also fails M4 in the way that is hardest
  to debug — a path outside its allowlist is not refused, it is quietly
  rewritten, and the caller finds the file somewhere else.
* **svg-mcp** fails M4: output goes to a temporary directory and the caller
  cannot choose the destination.
* **img-proc** is a general raster toolbox. SVG is one input format among many
  and render fidelity is not a stated goal, so M5 is not addressed.
* **ppbong** covers PNG and JPEG only and is shaped around icon sets rather than
  around arbitrary target sizes, so it fails M3 and only partly meets M2.
* **magick** and **im-mcp** both delegate SVG rendering to whatever the host's
  ImageMagick installation finds — often its internal MSVG renderer, sometimes
  librsvg, occasionally nothing. The same call produces different images on
  different machines, which is the failure mode M5 exists to prevent. Both also
  require an ImageMagick installation, which this server's single static binary
  does not.

## Optional capabilities

| Capability | surferdot | svg-mcp | img-proc | ppbong | magick | im-mcp | **this server** | Replaced by |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | --- |
| SVG source as a string | ● | ● | ○ | ○ | ○ | ○ | **●** | `render_svg` (`svg_source`) |
| SVG from a URL | ○ | ○ | ○ | ○ | ○ | ○ | **●** opt-in | `render_svg` (`svg_url`) |
| Scale factor | ● | ○ | ◐ `?` | ○ | ● | ● | **●** | `render_svg` (`scale`) |
| Size derived from `viewBox` | ● | ● | ◐ `?` | ● | ● | ● | **●** | `render_svg` (omit size) |
| Background colour | ● | ◐ `?` | ● | ○ | ● | ● | **●** | `render_svg` (`background`) |
| Force/disable transparency | ● | ○ | ● | ○ | ● | ● | **●** | `render_svg` (`transparent`) |
| Padding colour distinct from backdrop | ○ | ○ | ○ | ○ | ◐ manual | ◐ manual | **●** | `render_svg` (`padding_color`) |
| JPEG quality | ● | ◐ `?` | ● | ○ | ● | ● | **●** | `render_svg` (`jpeg_quality`) |
| PNG compression level | ○ | ○ | ● | ○ | ● | ● | **●** | `render_svg` (`png_compression`, `png_optimize`) |
| DPI / density | ○ | ○ | ● | ○ | ● | ● | **●** | `render_svg` (`dpi`, `density_metadata`) |
| Base64 / inline image result | ○ | ● | ● | ○ | ○ | ◐ `?` | **●** | `render_svg` (`return_mode`), `convert_image` |
| Multi-resolution ICO | ○ | ○ | ○ | ● | ● | ● | **●** | `render_icon` |
| ICNS | ○ | ○ | ○ | ● | ● | ● | **◐** specified, low priority | `render_icon` |
| SVGO-style optimisation | ○ | ○ | ● | ○ | ○ | ○ | **●** | `optimize_svg` |
| Base64 ↔ image conversion | ○ | ○ | ● | ○ | ○ | ○ | **●** | `convert_image` |
| Raster format conversion | ○ | ○ | ● | ○ | ● | ● | **●** | `convert_image` |
| Raster resize | ○ | ○ | ● | ○ | ● | ● | **●** | `convert_image` |
| Batch: many sizes/formats per call | ○ | ○ | ◐ `?` | ● icon sets | ◐ shell-level | ◐ shell-level | **●** | `render_svg_batch` |
| Aspect-ratio strategy as a parameter | ○ | ○ | ◐ `?` | ○ | ● `-resize` geometry flags | ● | **●** | `render_svg` (`fit`, `align`) |
| Sub-element export by `id` | ○ | ○ | ○ | ○ | ○ | ○ | **●** | `render_svg` (`export_id`) |
| Custom fonts / font directories | ○ | ◐ `?` | ○ | ○ | ◐ host fontconfig | ◐ host fontconfig | **●** | `render_svg` (`fonts`) |
| Inspect an SVG without rendering | ○ | ○ | ○ | ○ | ◐ `identify` | ◐ `identify` | **●** | `probe_svg` |
| Declared capability and gap report | ○ | ○ | ○ | ○ | ○ | ○ | **●** | `get_capabilities` |
| Per-call warnings on lost fidelity | ○ | ○ | ○ | ○ | ○ | ○ | **●** | every render tool |
| Animated SVG → GIF/APNG | ○ | ○ | ○ | ○ | ◐ SMIL unsupported in practice | ◐ | **○** stretch goal | [`SPEC.md`](SPEC.md) §10 |
| No external runtime dependency | ○ Node | ○ Node wrapper | ○ | ○ Node | ○ ImageMagick | ○ ImageMagick | **●** single static binary | — |

## Coverage summary

| Foreign server | Every capability of it covered here? | Remainder |
| --- | --- | --- |
| surferdot | yes | — |
| svg-mcp | yes | — |
| img-proc | yes, for its SVG and format-conversion surface | Raster editing beyond resize and format conversion — cropping, rotation, compositing — is out of scope. `convert_image` is not a general image editor. |
| ppbong | yes | ICNS is specified but low priority ([`SPEC.md`](SPEC.md) §5.3) |
| magick | for SVG rendering, yes | ImageMagick's wider raster feature set is out of scope by design |
| im-mcp | for SVG rendering, yes | as above |

The two ImageMagick wrappers are the only ones this server does not fully
subsume, and deliberately so: they are general image-manipulation front ends
that happen to read SVG. This server replaces the SVG-rendering half of what
they are used for, correctly and reproducibly, and does not attempt to be a
general raster editor.

## What no existing server does

Three things in this specification have no counterpart in any of the six:

1. **A declared fidelity contract.** [`SPEC.md`](SPEC.md) §9 enumerates what is
   rendered and what is not, `get_capabilities` reports it, `probe_svg` checks a
   document against it, and every render result carries the warnings that apply.
   None of the six tells a caller that a font was substituted, a filter dropped
   or an animation ignored.
2. **Exact output dimensions with a stated aspect-ratio policy.** The others
   either scale by a factor, or resize with an unstated rule about what happens
   to a mismatched aspect ratio. Here the canvas is exactly the requested size,
   `fit` says how the drawing is mapped onto it, and §4.6 says precisely what
   fills the remainder in every format — including the formats that have no
   alpha channel.
3. **Paths that are obeyed or refused.** No temporary directory, no silent
   redirect. An allowlist exists only if the operator configures one, and then
   it produces an error naming the allowed directories.
