---
description: Make the img-gen-via-svg server's binary available on this machine
---

The `img-gen-via-svg` MCP server is a compiled Rust binary. Claude Code clones a
plugin's repository but never builds it, so on a fresh installation the binary
has to be fetched or compiled once. This command does that.

Run the plugin's launcher in install mode:

```bash
uv run --quiet --no-project --python ">=3.9" "${CLAUDE_PLUGIN_ROOT}/bin/launch.py" --install
```

It tries, in this order: a binary that is already there, the release artefact for
this platform, and finally a build from the sources the plugin was cloned with.

Then report to the user:

- **If it printed a path** — the server is ready. Tell the user to restart the
  Claude Code session, because an MCP server is started once per session.
- **If it failed for want of a release and there is no `cargo` on PATH** — the
  machine has no Rust toolchain. Say so, and give them the two ways out:
  install Rust from <https://rustup.rs> and run this command again, or set
  `IMG_SVG_MCP_BIN` to a binary they already have.
- **If the build itself failed** — show the compiler output. Do not retry
  blindly; a build error is a real error.

To compile rather than download, even when a release exists, add `--build`. To
replace a binary that is already installed, add `--force`.
