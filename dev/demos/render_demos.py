#!/usr/bin/env python3
"""Render every demo document through the MCP server and record what it reported.

The AI agent, or the user, runs this to exercise the server the way a client
does: over stdio, one JSON-RPC call per document, with no shortcut into the
library. It writes the PNGs into ``out/`` and the per-document report into
``report.json``.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
from typing import Any

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
SVG_DIR = HERE / "svg"
OUT_DIR = HERE / "out"
BINARY = REPO / "target" / "release" / "img-gen-via-svg-mcp"

#: Documents rendered at this canvas size, twice their 240-unit intrinsic size.
CANVAS = 480


class McpClient:
    """A minimal MCP client speaking JSON-RPC over the server's stdio pipes."""

    def __init__(self, binary: pathlib.Path) -> None:
        self._process = subprocess.Popen(
            [str(binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self._next_id = 0
        self._request(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "demo-renderer", "version": "1"},
            },
        )
        self._notify("notifications/initialized")

    def _send(self, payload: dict[str, Any]) -> None:
        assert self._process.stdin is not None
        self._process.stdin.write(json.dumps(payload) + "\n")
        self._process.stdin.flush()

    def _notify(self, method: str, params: dict[str, Any] | None = None) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    def _request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        self._next_id += 1
        request_id = self._next_id
        self._send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        assert self._process.stdout is not None
        while True:
            line = self._process.stdout.readline()
            if not line:
                raise RuntimeError("the server closed its output stream")
            message = json.loads(line)
            if message.get("id") == request_id:
                if "error" in message:
                    raise RuntimeError(f"{method} failed: {message['error']}")
                return message["result"]

    def call(self, tool: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Calls one tool and returns its structured result."""
        result = self._request("tools/call", {"name": tool, "arguments": arguments})
        structured = result.get("structuredContent") or {}
        structured["_is_error"] = bool(result.get("isError"))
        structured["_text"] = next(
            (block["text"] for block in result.get("content", []) if block["type"] == "text"),
            "",
        )
        return structured

    def close(self) -> None:
        """Shuts the server down and returns."""
        assert self._process.stdin is not None
        self._process.stdin.close()
        try:
            self._process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self._process.kill()


def main() -> int:
    if not BINARY.exists():
        print(f"build the server first: cargo build --release ({BINARY} is missing)")
        return 1

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    client = McpClient(BINARY)
    report: dict[str, Any] = {"capabilities": client.call("get_capabilities", {}), "documents": []}

    for svg in sorted(SVG_DIR.glob("*.svg")):
        png = OUT_DIR / f"{svg.stem}.png"
        probe = client.call("probe_svg", {"svg_path": str(svg)})
        render = client.call(
            "render_svg",
            {
                "svg_path": str(svg),
                "output_path": str(png),
                "width": CANVAS,
                "height": CANVAS,
                "png_optimize": True,
            },
        )
        entry = {
            "name": svg.stem,
            "svg": str(svg.relative_to(REPO)),
            "png": str(png.relative_to(REPO)) if not render.get("_is_error") else None,
            "intrinsic": probe.get("size"),
            "unsupported": probe.get("unsupported", []),
            "fonts_requested": probe.get("fonts_requested", []),
            "filter_primitives": probe.get("filter_primitives", []),
            "render_warnings": render.get("warnings", []),
            "render_error": render.get("error"),
            "bytes": render.get("bytes"),
            "render_ms": render.get("render_ms"),
            "padding_px": render.get("padding_px"),
        }
        report["documents"].append(entry)
        state = "ERROR" if render.get("_is_error") else f"{render.get('bytes')} bytes"
        print(f"{svg.stem:28} {state:>14}  {len(entry['render_warnings'])} warning(s)")

    client.close()
    (HERE / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"\n{len(report['documents'])} documents rendered into {OUT_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
