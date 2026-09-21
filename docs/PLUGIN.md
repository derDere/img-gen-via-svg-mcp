# The Claude Code plugin

This repository is both the server and a Claude Code plugin. Installing the
plugin is what puts the server's tools in front of Claude.

```
/plugin marketplace add derDere/MyClaudeMarked
/plugin install img-gen-via-svg@derdere
```

| Thing | Value |
| --- | --- |
| Plugin name | `img-gen-via-svg` |
| Marketplace | `derdere`, catalogued in `derDere/MyClaudeMarked` |
| MCP server key | `img-gen-via-svg`, so its tools appear as `mcp__img-gen-via-svg__render_svg` and so on |
| Components | One MCP server with seven tools, and one slash command, `/img-gen-via-svg:setup` |

## The files that make it a plugin

| Path | Purpose |
| --- | --- |
| `.claude-plugin/plugin.json` | The manifest: name, description, version, author |
| `.mcp.json` | How Claude Code starts the server |
| `bin/launch.py` | Finds the server binary, fetches or builds it when there is none, and hands the process over to it |
| `commands/setup.md` | `/img-gen-via-svg:setup`, which makes a binary available on demand |

## The problem this had to solve

Claude Code clones a plugin's repository when a user installs it. It does not
build anything. This server is a compiled Rust binary, so a naive `.mcp.json`
pointing at `target/release/img-gen-via-svg-mcp` describes a file that exists on
exactly one machine — the one it was built on — and the plugin is broken for
everyone else.

Four ways out were available.

| Option | What it costs the user | Why it was not taken alone |
| --- | --- | --- |
| Commit the binary to the repository | Nothing at install time | 18 MB per platform in the history of a repository every user clones, and Git never forgets it. The marketplace guide rules this out explicitly. |
| Require `cargo install` before installing the plugin | A Rust toolchain and a manual step before the plugin works at all | A plugin that fails until the user has read the README is a plugin that fails. |
| Build at first launch | Two minutes on first start | An MCP server that takes two minutes to start is killed by the startup timeout, and the user sees a failed server rather than a build. |
| Download a prebuilt release artefact at first launch | A few seconds on first start, and a network connection | Nothing to hold against it — except that it needs published releases, which is a thing this repository does not have yet. |

**The decision: a launcher that resolves a binary, in a fixed order, and
downloads a release artefact when it finds none.** Building is available but
never automatic, because a compile does not fit inside a server launch.

That gives a working plugin under every combination: a machine with a release to
download, a machine with a Rust toolchain and no network, and a machine where
the user already has the binary.

## How the binary is resolved

`bin/launch.py` runs through these in order and uses the first that works:

1. `$IMG_SVG_MCP_BIN`, when the user has pointed it at a binary.
2. `<plugin data>/bin/img-gen-via-svg-mcp`, where a previous download or build
   left one. Claude Code supplies the plugin data directory and it survives
   plugin updates.
3. `img-gen-via-svg-mcp` on `PATH` — which is what `cargo install` produces.
4. `target/release/` and then `target/debug/` under the plugin directory, which
   is where a developer working in a clone of this repository has one.
5. Failing all of those, the release artefact for the running platform is
   downloaded from this repository's GitHub releases and cached under (2).

When even that finds nothing, the launcher writes a message naming the three
ways out and exits non-zero, so `/mcp` in the session shows why the server is
not there rather than showing nothing.

`/img-gen-via-svg:setup` runs the same resolution with `--install`, which adds a
build from source as a last resort. It is a slash command rather than part of
the launch because it may take minutes.

### Environment

| Variable | Effect |
| --- | --- |
| `IMG_SVG_MCP_BIN` | Use this binary and look no further. |
| `IMG_SVG_MCP_NO_DOWNLOAD` | Set to `1` to never reach the network at launch. |

The server's own configuration — the path allowlist, the pixel limits, the font
directories — is in [`SPEC.md`](SPEC.md) §7 and is set in the `env` block of the
client's own configuration.

## What a user on an empty machine has to do

**Once a release is published:** nothing beyond the two `/plugin` commands. The
first session downloads the artefact for the platform, caches it, and starts the
server. Later sessions start it directly.

**Until then**, the download finds nothing and the user needs a Rust toolchain:

```
/plugin marketplace add derDere/MyClaudeMarked
/plugin install img-gen-via-svg@derdere
/img-gen-via-svg:setup          # builds from the plugin's own sources, a few minutes
```

and then restarts the session, because an MCP server is started once per
session. Without Rust, `cargo install --git https://github.com/derDere/img-gen-via-svg-mcp`
is the same dead end and the honest answer is that the plugin does not work yet
on that machine.

## Publishing a release

The plugin's automatic path needs one thing that does not exist yet: a GitHub
release tagged `v<version>` carrying an artefact per platform. The launcher
constructs the download URL from the version in `.claude-plugin/plugin.json` and
the running platform, and expects the names the release workflow in
[`ARCHITECTURE.md`](ARCHITECTURE.md) §5.1 produces:

```
img-gen-via-svg-mcp-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
img-gen-via-svg-mcp-v0.1.0-x86_64-unknown-linux-musl.tar.gz
img-gen-via-svg-mcp-v0.1.0-x86_64-pc-windows-msvc.zip
```

Each archive holds the binary anywhere inside it; the launcher searches the
extracted tree by name rather than assuming a layout.

Two steps produce them:

1. Copy the release workflow from [`ARCHITECTURE.md`](ARCHITECTURE.md) §5.1 into
   `.github/workflows/release.yml`. It is not committed here because adding a
   workflow file needs a token scope the automated account does not carry.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.

Three versions have to agree, and nothing checks this automatically: the
`version` in `.claude-plugin/plugin.json`, the `version` in `Cargo.toml`, and
the release tag. The launcher reads the first of those, so a plugin version
without a matching release falls back to the build path.
