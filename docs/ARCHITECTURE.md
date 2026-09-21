# Architecture

## 1. The stack, and why

| Layer | Choice | Version |
| --- | --- | --- |
| Language | Rust (edition 2024, MSRV 1.88) | — |
| SVG parsing and simplification | `usvg` | 0.48.1 |
| Rasterisation | `resvg` on `tiny-skia` | 0.48.1 / 0.12.0 |
| Encoding | `image` | 0.25.10 |
| MCP protocol | `rmcp`, the official Rust SDK | 3.4.0 |
| Transport | stdio | — |

### 1.1 Rust

The deliverable is a single statically linked binary with no runtime
dependency. A client registers it as a command in `mcp.yaml` or
`claude_desktop_config.json`, and that is the entire installation — no Node
runtime, no Python environment, no ImageMagick, no shared library that has to
match the host's version.

This is also what separates this server from `magick-convert` and
`mcp-imagemagick`: both require an ImageMagick installation and both inherit
whatever SVG delegate that installation happens to have, which makes their
output a property of the host rather than of the input.

### 1.2 resvg, usvg and tiny-skia

resvg is the reason this project can promise
[`SPEC.md`](SPEC.md) §2.1 M5 at all.

* **Filter coverage.** resvg implements the full static SVG 1.1 filter set —
  `feTurbulence`, `feDisplacementMap`, `feConvolveMatrix`, `feMorphology`, the
  two lighting primitives, and the rest. Among renderers that can realistically
  be embedded in a binary, nothing else comes close; the common alternatives
  either skip filters or implement `feGaussianBlur` and stop.
* **A public regression suite.** resvg is verified against roughly 1600
  SVG-to-PNG regression tests, published separately as
  [`linebender/resvg-test-suite`](https://github.com/linebender/resvg-test-suite).
  [`TESTPLAN.md`](TESTPLAN.md) uses it directly. A fidelity claim backed by
  someone else's 1600 tests is worth more than one backed by our own dozen.
* **Reproducibility.** resvg uses no system graphics or text libraries, so the
  same document renders to identical pixels on Linux and on Windows. That
  property is what makes reference-image testing possible, and it is why the
  font database is the only cross-platform variable left
  ([`SPEC.md`](SPEC.md) §4.8.3).
* **Absolute pixel sizes.** Sizing is a transform applied to a pixmap the caller
  allocates, so an exact `width` × `height` target needs no detour through a
  scale factor.

**Two assumptions did not survive checking.** The first is the CLI's sizing.
The resvg *CLI* accepts `--width`
and `--height`, but when both are given it fits the drawing into that box while
preserving the aspect ratio and then shrinks the canvas to the fitted size
(`IntSize::scale_to`). Asking its CLI for 512 × 512 from a 100 × 80 document
yields a 512 × 410 image, not a 512 × 512 one. The *library* has no such
behaviour — it renders into whatever pixmap it is given, with whatever transform
it is given. This server therefore allocates the pixmap at exactly the requested
size and computes the transform itself in `render::fit`, which is also where the
`contain` / `stretch` / `cover` / `error` strategies of
[`SPEC.md`](SPEC.md) §4.3 live. The CLI's behaviour is not a model to copy.

The second concerns `feDisplacementMap`. resvg 0.48.1 multiplies each pixel's
displacement by the primitive's `scale` twice, so a document asking for
`scale="38"` is displaced by roughly 1444 pixels and its content leaves the
filter region entirely — the element renders empty. The defect is in
`crates/resvg/src/filter/displacement_map.rs`, where `sx` already carries the
scale and is multiplied by `fe.scale()` again, and it is reproducible with a
uniform grey displacement map, which must be a no-op and is not.

This is exactly the case the whole project is built around: a feature the
renderer lists as supported, rendered wrong, with nothing said. This server
therefore detects it, declares it in the gap catalogue, and reports it per
document with the value that works around it. It does not rewrite the caller's
document to compensate — that would break the day the defect is fixed, and
editing the input behind the caller's back is the behaviour this project exists
to replace. [`SPEC.md`](SPEC.md) §9.4 has the full treatment.

resvg's other limitations are real and are declared rather than worked around:
no SMIL animation, no scripting, no `foreignObject`, and a limited CSS subset.
One more is worth naming because it surprises people: when a text element's
`font-family` is not in the font database, resvg draws **no text at all** rather
than falling back to another family. [`SPEC.md`](SPEC.md) §9.2 lists them as the
known gaps, and §9.3 defines how they are detected and reported instead of being
rendered wrong in silence.

### 1.3 The `image` crate

This is the second reason the stack is Rust rather than Node. The obvious Node
choice, `sharp`, cannot write BMP at all, and BMP is a mandatory format
([`SPEC.md`](SPEC.md) §2.1 M3). The `image` crate encodes PNG, JPEG, BMP, GIF,
TIFF, WebP, ICO, TGA, QOI, AVIF, PNM, Farbfeld, OpenEXR and HDR, which turns a
mandatory three-format requirement into fourteen for no additional work.

Several limits were confirmed and are specified rather than glossed over:

* **WebP encoding is lossless only.** The crate has no lossy WebP encoder.
  [`SPEC.md`](SPEC.md) §4.5 states it and `jpeg_quality` raises a
  `lossless_only` warning when it is set for WebP.
* **AVIF encoding is behind a Cargo feature** that is off by default, because
  the encoder costs several minutes of build time for a format nothing in the
  requirements asks for. `get_capabilities` reports whether a build has it.
* **TIFF compression and GIF dithering are not selectable.** The crate's
  encoders expose no setting for either, so this server exposes none. Inventing
  a parameter that silently does nothing would be worse than not having one.
* **Neither the PNG nor the JPEG encoder writes a physical resolution.**
  `density_metadata` is therefore applied afterwards, by inserting a `pHYs`
  chunk into the encoded PNG and by patching the JFIF header of the encoded
  JPEG. No other format can carry it, and asking for it elsewhere raises
  `metadata_unsupported`.
* **Farbfeld, OpenEXR and HDR accept only their own pixel types** — 16-bit
  integer, 32-bit float with alpha and 32-bit float without — so `encode::raster`
  converts into them explicitly rather than relying on the dispatcher.

Multi-resolution ICO comes from the crate's `IcoEncoder::encode_images`, which
takes a set of frames — exactly what `render_icon` needs. ICNS has no support
in the crate and needs the separate `icns` crate; since ICNS is a nice-to-have
([`SPEC.md`](SPEC.md) §5.3), that dependency is optional and behind a Cargo
feature.

### 1.4 rmcp

`rmcp` is the official Rust MCP SDK and is actively maintained (3.4.0, September
2026). Its macro layer is a good fit for a tools-only server: `#[tool_router]`
on the `impl` block, `#[tool]` per method, `Parameters<T>` for the input struct,
and `schemars` deriving the JSON Schema from that struct, so the schema in
[`SPEC.md`](SPEC.md) §5 and the schema on the wire cannot drift apart.

`rmcp` also draws the distinction this server's error model depends on: a
`CallToolResult` marked as an error carries a message the client renders for the
user, while a JSON-RPC error is usually hidden. [`SPEC.md`](SPEC.md) §6 follows
that — tool failures are tool-level errors, and the protocol error is reserved
for malformed requests.

Transport is `rmcp`'s stdio transport, and only that.

### 1.5 A note on a later HTTP transport

None is planned and none is built. If one is ever wanted, `rmcp` provides a
streamable-HTTP server transport and the tool implementations would not change:
the tool router is transport-agnostic, so the work is a second branch in `main`
that constructs a different transport, plus authentication and a bind address in
the configuration. Nothing in this design has to be prepared for that now, and
nothing in it is made harder by not preparing.

## 2. Module layout

One responsibility per file, and no file large enough to need scrolling to
understand.

```
src/
  main.rs              Process entry: logging to stderr, configuration, diagnostics, stdio transport.
  lib.rs               The crate root, so everything below the protocol layer is testable on its own.
  config.rs            Operator configuration: environment variables, TOML file, validation.
  error.rs             The error type and the stable error codes.
  warning.rs           The warning type and the per-call collector.
  pipeline.rs          The twelve steps every rendering tool shares, from input to written file.

  svg/
    source.rs          Resolves svg_path / svg_source / svg_url into text; svgz; resources_dir.
    scan.rs            Reads the raw XML for what the parser discards without saying so.
    document.rs        usvg options and parsing, with the parser's diagnostics captured.
    diagnostics.rs     Routes the renderer's `log` output into the call that caused it.
    fidelity.rs        The gap catalogue, the per-document check, the strict modes.
    fonts.rs           The font database: system directories, extra paths, family resolution.
    probe.rs           The document inventory behind probe_svg.
    optimize.rs        normalize and minify for optimize_svg.

  render/
    sizing.rs          The size model of SPEC §4.2.
    fit.rs             contain / stretch / cover / error, alignment, transform and padding.
    canvas.rs          Pixmap allocation, background and padding fills, flattening (SPEC §4.6).
    renderer.rs        The resvg call, export_id and export_area.

  encode/
    format.rs          The format enum, alpha capability, extensions, media types.
    options.rs         Encoder parameters and their validation.
    raster.rs          Encoding a canvas or an image buffer into file bytes.
    ico.rs             Single-image and multi-resolution ICO.
    icns.rs            ICNS, behind the `icns` feature.
    png_opt.rs         Lossless PNG post-optimisation, behind the `png-optimize` feature.
    metadata.rs        The pHYs chunk and the JFIF density patch.

  io/
    paths.rs           Allowlist checks, canonicalisation, symlink policy (SPEC §7.2).
    atomic.rs          Temporary file plus rename.
    remote.rs          HTTP fetching, behind the `remote` feature and the operator's switches.
    raster_in.rs       Raster decoding for convert_image, including Base64 input.

  mcp/
    server.rs          The server type, the tool router, the ServerHandler, the time budget.
    state.rs           The configuration and font database shared by every call.
    params.rs          One parameter struct per tool; the JSON schemas derive from these.
    result.rs          Structured results, text summaries, inline image blocks.
    tools/             One module per tool, each a `run` function over the state and its params.
```

The dependency direction is one-way: `mcp/tools/*` orchestrates `pipeline`,
which orchestrates `svg`, `render`, `encode` and `io`; none of those knows that
MCP exists. `pipeline` is what keeps a batch entry, an icon size and a single
render from drifting apart, because all three walk the same code. Everything
below `mcp` is testable without a protocol client, which is what
[`TESTPLAN.md`](TESTPLAN.md) relies on.

## 3. Rendering pipeline

`render_svg` in order:

1. **Resolve the input** (`svg::source`) — path, string or URL into bytes, with
   the allowlist and the remote policy applied. Decompress `.svgz`.
2. **Scan** (`svg::scan`) — find the constructs usvg discards without a
   diagnostic, before they disappear.
3. **Parse** (`svg::document`) — usvg with the assembled options, capturing the
   parser's diagnostics into the warning collector.
4. **Check fidelity** (`svg::fidelity`) — turn scan findings, parser
   diagnostics and font substitutions into warnings, or into a refusal when
   `on_unsupported` or `on_missing_font` is `error`.
5. **Resolve the size** (`render::sizing`) — the rules of SPEC §4.2, producing
   the canvas size and the source size.
6. **Compute the mapping** (`render::fit`) — the transform and the padding
   rectangle for the chosen strategy and alignment.
7. **Prepare the canvas** (`render::canvas`) — allocate exactly the canvas size,
   apply `background`, then `padding_color`.
8. **Render** (`render::renderer`) — resvg into the pixmap, under the render
   timeout.
9. **Flatten** (`render::canvas`) — only when the format has no alpha or
   `transparent` is `false`.
10. **Encode** (`encode`) — with the format's validated options.
11. **Write** (`io::atomic`) — temporary file plus rename, so a failure leaves
    nothing behind.
12. **Assemble the result** (`mcp::result`) — structured JSON, the collected
    warnings, and the inline image block when asked for.

Steps 2, 3 and 4 are the fidelity contract. Steps 5 to 7 are what make the
output exactly the requested size. Step 11 is what makes a failed call leave the
filesystem as it was.

## 4. Dependencies

| Crate | Version | Purpose | Notes |
| --- | --- | --- | --- |
| `rmcp` | 3.4 | MCP server, stdio transport | features `server`, `macros`, `transport-io` |
| `resvg` | 0.48 | Rasterisation | |
| `usvg` | 0.48 | Parsing and simplification | its `writer` feature is what `optimize_svg` normalises through |
| `tiny-skia` | 0.12 | Pixmap, transforms, compositing | pinned explicitly because `render::canvas` uses it directly |
| `fontdb` | 0.24 | Font database | the version usvg 0.48 expects |
| `image` | 0.25 | Encoding and decoding | `avif` optional and off by default |
| `icns` | 0.5 | ICNS container | optional, `icns` feature, on by default |
| `oxipng` | 10 | Lossless PNG post-optimisation | optional, `png-optimize` feature, on by default |
| `ureq` | 3 | HTTP client for `svg_url` and remote references | optional, `remote` feature; blocking and small, no second async stack |
| `roxmltree` | 0.21 | The pre-parse scan | reads the document as XML without resolving it |
| `flate2` | 1 | `.svgz` decompression | |
| `svgtypes` | 0.16 | CSS colour parsing for `background` and `padding_color` | the same parser resvg uses, so colours behave identically inside and outside the document |
| `log` | 0.4 | The renderer's diagnostic channel | this crate installs the logger that captures it |
| `serde`, `serde_json` | 1 | Parameters and structured results | |
| `schemars` | 1.2 | JSON Schema generation from the parameter structs | the version rmcp 3.x requires |
| `tokio` | 1 | The async runtime rmcp requires | features `rt-multi-thread`, `macros`, `io-std`, `sync`, `time` |
| `tracing`, `tracing-subscriber` | 0.3 | Logging to stderr | |
| `thiserror` | 2 | Error types | |
| `base64` | 0.23 | Inline image content, Base64 input and output | |
| `toml` | 1 | The configuration file | |

The one development dependency is `tempfile`, for the directories the tests
write into. The test corpus is the demo corpus under `dev/demos/svg`, which
keeps the documents that prove the claims and the documents a reader can look at
as one set rather than two.

Versions are minimum compatible versions; `Cargo.lock` is committed, because a
server whose output is claimed to be byte-reproducible cannot float its
rendering dependencies.

## 5. Target platforms and release builds

Linux and Windows are the platforms that have to work. macOS is taken along
where it costs nothing and is not a release requirement.

| Target triple | Status | Built by |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | required | `ubuntu-latest` |
| `x86_64-pc-windows-msvc` | required | `windows-latest` |
| `x86_64-unknown-linux-musl` | optional, fully static | `ubuntu-latest` with the musl target |
| `aarch64-apple-darwin` | opportunistic | `macos-latest`, allowed to fail |

The pure-Rust dependency set means none of these needs a C toolchain or a system
library, so the matrix is a plain `cargo build --release` per target. The musl
target produces a binary with no libc dependency at all, which is the right
artefact for a container image.

### 5.1 Release workflow

The release workflow belongs at `.github/workflows/release.yml`. It is given in
full here because adding a workflow file needs a token scope that the automated
account does not carry, so it is copied into place by hand:

```yaml
name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:

permissions:
  contents: write

jobs:
  build:
    name: ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    continue-on-error: ${{ matrix.optional || false }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - { os: ubuntu-latest,  target: x86_64-unknown-linux-gnu,  bin: img-gen-via-svg-mcp }
          - { os: ubuntu-latest,  target: x86_64-unknown-linux-musl, bin: img-gen-via-svg-mcp }
          - { os: windows-latest, target: x86_64-pc-windows-msvc,    bin: img-gen-via-svg-mcp.exe }
          - { os: macos-latest,   target: aarch64-apple-darwin,      bin: img-gen-via-svg-mcp, optional: true }
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Install musl tooling
        if: endsWith(matrix.target, '-musl')
        run: sudo apt-get update && sudo apt-get install -y musl-tools

      - name: Build
        run: cargo build --release --locked --target ${{ matrix.target }}

      - name: Package
        shell: bash
        run: |
          staging="img-gen-via-svg-mcp-${GITHUB_REF_NAME}-${{ matrix.target }}"
          mkdir "$staging"
          cp "target/${{ matrix.target }}/release/${{ matrix.bin }}" "$staging/"
          cp README.md LICENSE "$staging/" 2>/dev/null || cp README.md "$staging/"
          if [ "${{ runner.os }}" = "Windows" ]; then
            7z a "$staging.zip" "$staging"
          else
            tar czf "$staging.tar.gz" "$staging"
          fi

      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.target }}
          path: |
            *.tar.gz
            *.zip

  release:
    needs: build
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/v')
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            *.tar.gz
            *.zip
          generate_release_notes: true
```

### 5.2 Continuous integration

The integration workflow belongs at `.github/workflows/ci.yml`, likewise copied
into place by hand. It runs on every push and pull request, on `ubuntu-latest`
and `windows-latest`:

```yaml
name: ci

on: [push, pull_request]

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo test --all-features --locked
```

Running the whole suite on both operating systems is the point: the tool tests
assert on pixels and on warning codes, so a font-related divergence between
Linux and Windows shows up as a failure rather than as a difference nobody
looks at ([`TESTPLAN.md`](TESTPLAN.md)).

## 6. Font handling across platforms

Fonts are the one component that is genuinely different on Linux and Windows,
and they are therefore handled explicitly rather than left to whatever the host
provides.

* **At start-up** the server builds one font database and keeps it for the
  process lifetime. It loads the platform's system font directories — the
  fontconfig search path on Linux, `%WINDIR%\Fonts` and the per-user font
  directory on Windows — unless the operator disables that, then the operator's
  configured directories and files on top.
* **Per call** `fonts.extra_dirs` and `fonts.extra_files` extend that database
  for one render. The per-call database is a clone of the start-up one with the
  extra faces added, so a call cannot leak fonts into the next.
* **Generic families** (`serif`, `sans-serif`, `monospace`, `cursive`,
  `fantasy`) resolve to whatever is available on the host and are therefore
  configurable; `get_capabilities` reports what they resolved to.
* **A missing font is never silent.** The resolved tree is walked after parsing,
  the fonts actually used are compared against the families the document asked
  for, and every difference becomes a `font_substituted` warning naming both.
  `on_missing_font: "error"` turns it into a refusal.
* **Reproducible text** across the two platforms means taking the host out of
  the picture: `skip_system_fonts` plus an explicit font directory, or an SVG
  whose text has been converted to outlines by `optimize_svg`. Both routes are
  in [`SPEC.md`](SPEC.md) §4.8.3.

The server does not ship fonts of its own. A build that bundled a font would
make the binary larger for everyone to solve a problem that only some callers
have, and the two routes above solve it better.
