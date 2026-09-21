# Open questions

Everything here is a decision for the project owner. Each item states the
question, what is at stake, the recommendation, and what the specification does
until the decision is taken. Nothing here blocks implementation: every item has
a provisional default written into [`SPEC.md`](SPEC.md).

## Decisions already taken

These three shaped the specification and are written into it as ordinary rules,
not as questions:

| Decision | Effect |
| --- | --- |
| **Aspect-ratio default: fit with a transparent border** | `fit: "contain"` is the default. The drawing is scaled uniformly to fit entirely inside the requested canvas, nothing is distorted and nothing is cropped, and the remainder is transparent. `stretch`, `cover` and `error` remain available as parameter values. [`SPEC.md`](SPEC.md) §4.3, and §4.6 for what fills the border in each format. |
| **Target platforms: Linux and Windows** | Release builds for `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl` and `x86_64-pc-windows-msvc`; macOS arm64 is built opportunistically and may fail without failing the release. ICNS is therefore a nice-to-have rather than a platform requirement. [`ARCHITECTURE.md`](ARCHITECTURE.md) §5. |
| **Transport: stdio only** | No HTTP, no SSE. The server is a command entry in `mcp.yaml` or `claude_desktop_config.json`. [`SPEC.md`](SPEC.md) §3. |

## Q-A — Should `overwrite` default to `true` or `false`?

**At stake.** Whether a render silently replaces an existing file.

`true` is convenient: an agent iterating on an icon renders to the same path a
dozen times, and `false` would make every attempt after the first fail with
`output_exists` until the agent learns to pass the flag. `false` is safer: no
call ever destroys a file the caller did not mean to touch.

**Recommendation: `true`.** The overwhelmingly common case is deliberate
re-rendering, the caller chose the path explicitly, and a server that refuses by
default trains agents to pass `overwrite: true` on every call, which removes the
protection anyway. An operator who wants the protection can get a stronger one
from a write-only output directory.

**Currently built:** `true`, marked PENDING in [`SPEC.md`](SPEC.md) §4.4.
Changing it is a one-line change in `mcp/tools/render_svg.rs` and its siblings.

## Q-B — Are system fonts loaded by default?

**At stake.** Whether the same document renders the same way on two machines.

Loading system fonts by default means an SVG that names "Arial" renders with
Arial on Windows and with a substitute on Linux — visibly different, though the
result does say so in a `font_substituted` warning. Not loading them by default
means text renders in the single fallback family until the operator configures
fonts, which is predictable but almost never what the caller wanted.

**Recommendation: load them (`skip_system_fonts = false`).** A server that
ignores the host's fonts by default would surprise every caller rendering a
normal document, and the fidelity machinery already makes the difference
visible: every substitution is reported, `probe_svg` shows it before rendering,
and `on_missing_font: "error"` turns it into a refusal. Reproducibility across
platforms is available on demand via [`SPEC.md`](SPEC.md) §4.8.3 and does not
need to be the default.

**Currently built:** loaded, marked PENDING in [`SPEC.md`](SPEC.md) §7.4. The
consequence is sharper than the specification first assumed: a font family that
is missing does not change typeface, it makes the text disappear
([`SPEC.md`](SPEC.md) §9.2), which is an argument for keeping the host's fonts
available by default.

## Q-C — Does `render_svg` get a `timeout_ms` parameter of its own?

**At stake.** Whether a caller can raise or lower the render timeout per call.

The operator's `render_timeout_ms` (default 30 s) bounds every render. A
per-call parameter would let a caller give a heavy filter chain more time, or
fail fast on a batch.

**Recommendation: no.** A timeout is a resource policy and resource policy
belongs to the operator. A caller that can raise its own timeout can hold the
single-process, single-client stdio server for as long as it likes. If a
per-call parameter is wanted later, it should only ever be allowed to *lower*
the operator's value, which is the same rule the path allowlist follows.

**Currently built:** operator configuration only, marked PENDING in
[`SPEC.md`](SPEC.md) §7.1. A tool that produces several images gets the budget
multiplied by the number of images, so a batch is not cut off by a budget meant
for one render.

## Q-D — Which licence?

**At stake.** Whether and how others can use this.

The repository is public and is meant to replace six existing servers, so it
needs a licence before the first release; without one, nobody may legally use
it. The rendering and encoding dependencies are MIT/Apache-2.0 dual-licensed,
which permits anything.

**Recommendation: MIT OR Apache-2.0**, the Rust ecosystem's convention. It
matches the dependencies, imposes nothing on users, and is what anyone
evaluating a Rust crate expects to find.

**Currently built:** no licence chosen, and no `LICENSE` file in the repository.
The release workflow copies one into the artefact if it appears. This is the one
open question that has to be answered before a first release: without a licence
nobody may legally use this.

## Smaller points

These are lower-stakes and have a defensible default; they are listed because
the project owner may have a preference.

| # | Question | Default in the specification | Note |
| --- | --- | --- | --- |
| S1 | Does ICNS ship in the first release? | Built, behind the `icns` Cargo feature, on by default | No target platform consumes ICNS, but it cost one small module and one dependency. |
| S2 | How is the binary distributed? | GitHub release artefacts only | Alternatives, not mutually exclusive: publish on crates.io so `cargo install` works; an npm wrapper package, which is how most MCP servers are installed today and would matter for adoption by the users of the six servers this replaces. |
| S3 | Is `convert_image` in scope? | Built: format conversion, Base64 and resize | It exists to absorb `image-processing-mcp`. If that is not a goal, dropping it removes a decoder surface and a whole class of input handling. |
| S4 | Is 64 the right cap on `render_svg_batch` entries? | 64 | Large enough for any icon set. It is a guard against a single call occupying the server for minutes, not a meaningful limit. |
| S5 | Tool names | `render_svg`, `render_svg_batch`, `render_icon`, `probe_svg`, `optimize_svg`, `convert_image`, `get_capabilities` | Unprefixed. Clients show the server name alongside, so a prefix mostly adds noise — but a client with several image servers configured would show three `convert_image` entries. |
| S6 | Does the server expose MCP resources or prompts? | No, tools only | A resource listing the corpus of supported features, or a prompt template for common icon-set generation, would be small additions. Neither is required by anything in [`SPEC.md`](SPEC.md). |

## Reported upstream

Not a question, but the natural next action and nobody's job by default.

The `feDisplacementMap` defect in resvg 0.48.1 ([`SPEC.md`](SPEC.md) §9.4) is
reproducible in four lines and has a one-line fix: `displacement_map.rs`
multiplies by `fe.scale()` a second time although `sx` and `sy` already carry it.
It has not been reported to
[`linebender/resvg`](https://github.com/linebender/resvg/issues) from here.
Filing it, and dropping the gap from the catalogue once a release carries the
fix, is worth doing — both for this project and for everyone else using resvg.
