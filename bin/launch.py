#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""Find the img-gen-via-svg-mcp binary and run it, or say precisely what is missing.

Claude Code clones a plugin's repository; it does not build anything. This
server is a compiled Rust binary, so something has to bridge the gap between
"the sources are on disk" and "a binary is running". That is this script.

It resolves a binary in a fixed order — an explicit override, a previous
download, the user's PATH, a local build — and, when none of those turns one up,
downloads the release artefact for the running platform. Only `--install` will
build from source, because a two-minute compile inside an MCP server launch
would be killed by the startup timeout long before it finished.

Run with no arguments it becomes the server: the process is replaced, so the
MCP client talks to the server over the same pipes.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Iterator, List, Optional, Sequence

#: The repository that carries both the server and the release artefacts.
REPOSITORY = "derDere/img-gen-via-svg-mcp"

#: The name the compiled server is published and installed under.
BINARY_STEM = "img-gen-via-svg-mcp"

#: How long a release download may take before it is given up on, in seconds.
DOWNLOAD_TIMEOUT = 120


def log(message: str) -> None:
    """Writes a line to stderr, which is where an MCP server may speak."""
    print(f"[img-gen-via-svg] {message}", file=sys.stderr)


def plugin_root() -> Path:
    """The directory this plugin was cloned into."""
    from_env = os.environ.get("IMG_SVG_MCP_PLUGIN_ROOT") or os.environ.get("CLAUDE_PLUGIN_ROOT")
    if from_env:
        return Path(from_env)
    return Path(__file__).resolve().parent.parent


def plugin_data() -> Path:
    """The per-plugin directory that survives plugin updates.

    Claude Code supplies one. Outside Claude Code — a developer running this
    script by hand — a cache directory stands in for it.
    """
    from_env = os.environ.get("IMG_SVG_MCP_PLUGIN_DATA") or os.environ.get("CLAUDE_PLUGIN_DATA")
    if from_env:
        return Path(from_env)
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
    return base / BINARY_STEM


def binary_name() -> str:
    """The file name of the compiled server on this platform."""
    return f"{BINARY_STEM}.exe" if sys.platform == "win32" else BINARY_STEM


def target_triples() -> List[str]:
    """The Rust target triples whose artefacts run on this machine, best first."""
    machine = (os.uname().machine if hasattr(os, "uname") else os.environ.get("PROCESSOR_ARCHITECTURE", "")).lower()
    is_x86_64 = machine in {"x86_64", "amd64"}
    is_arm64 = machine in {"arm64", "aarch64"}

    if sys.platform.startswith("linux") and is_x86_64:
        # The musl build runs on a glibc system too, so it is the fallback.
        return ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]
    if sys.platform == "win32" and is_x86_64:
        return ["x86_64-pc-windows-msvc"]
    if sys.platform == "darwin" and is_arm64:
        return ["aarch64-apple-darwin"]
    if sys.platform == "darwin" and is_x86_64:
        return ["x86_64-apple-darwin"]
    return []


def plugin_version() -> str:
    """The version to fetch a release for, read from the plugin manifest."""
    manifest = plugin_root() / ".claude-plugin" / "plugin.json"
    try:
        return str(json.loads(manifest.read_text(encoding="utf-8"))["version"])
    except (OSError, ValueError, KeyError):
        return "0.1.0"


def is_runnable(path: Optional[Path]) -> bool:
    """Whether this path is a file the operating system will execute."""
    return path is not None and path.is_file() and os.access(str(path), os.X_OK)


def candidates() -> Iterator[Path]:
    """Every place a usable binary may already be, in the order they are tried."""
    override = os.environ.get("IMG_SVG_MCP_BIN")
    if override:
        yield Path(override)

    yield plugin_data() / "bin" / binary_name()

    on_path = shutil.which(BINARY_STEM)
    if on_path:
        yield Path(on_path)

    # A developer working in a clone of this repository has one here already.
    root = plugin_root()
    yield root / "target" / "release" / binary_name()
    yield root / "target" / "debug" / binary_name()


def find_existing() -> Optional[Path]:
    """The first binary that is already present and executable."""
    for candidate in candidates():
        if is_runnable(candidate):
            return candidate
    return None


def artefact_urls(version: str) -> Iterator[str]:
    """The release artefacts that could carry a binary for this machine."""
    tag = f"v{version}"
    suffix = ".zip" if sys.platform == "win32" else ".tar.gz"
    for triple in target_triples():
        name = f"{BINARY_STEM}-{tag}-{triple}{suffix}"
        yield f"https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"


def extract_binary(archive: Path, into: Path) -> Optional[Path]:
    """Pulls the server out of a downloaded archive and returns where it landed."""
    wanted = binary_name()
    with tempfile.TemporaryDirectory() as work:
        workdir = Path(work)
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as bundle:
                bundle.extractall(workdir)
        else:
            with tarfile.open(archive) as bundle:
                # Anything outside the extraction directory is a malformed
                # archive, and unpacking it anyway is how tar bombs work.
                members = [m for m in bundle.getmembers() if not m.name.startswith(("/", ".."))]
                bundle.extractall(workdir, members=members)

        found = next((p for p in workdir.rglob(wanted) if p.is_file()), None)
        if found is None:
            return None
        into.mkdir(parents=True, exist_ok=True)
        destination = into / wanted
        shutil.copy2(found, destination)
        destination.chmod(destination.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        return destination


def download(version: str) -> Optional[Path]:
    """Fetches the release artefact for this platform, or reports why it could not."""
    if not target_triples():
        log(f"No release is built for {sys.platform} on this architecture.")
        return None

    for url in artefact_urls(version):
        try:
            log(f"downloading {url.rsplit('/', 1)[-1]}")
            with urllib.request.urlopen(url, timeout=DOWNLOAD_TIMEOUT) as response:
                payload = response.read()
        except urllib.error.HTTPError as error:
            if error.code == 404:
                continue
            log(f"download failed with HTTP {error.code}")
            return None
        except (urllib.error.URLError, TimeoutError) as error:
            log(f"download failed: {error}")
            return None

        with tempfile.TemporaryDirectory() as work:
            archive = Path(work) / url.rsplit("/", 1)[-1]
            archive.write_bytes(payload)
            binary = extract_binary(archive, plugin_data() / "bin")
        if binary is not None:
            log(f"installed {binary}")
            return binary
        log("the archive held no server binary")
        return None

    log(f"no release artefact for this platform under tag v{version}")
    return None


def build_from_source() -> Optional[Path]:
    """Compiles the server from the sources this plugin was cloned with."""
    root = plugin_root()
    if not (root / "Cargo.toml").is_file():
        log(f"no Cargo.toml under {root}, so there is nothing to build")
        return None
    if shutil.which("cargo") is None:
        log("cargo is not on PATH; install Rust from https://rustup.rs and try again")
        return None

    log("building from source, which takes a few minutes the first time")
    result = subprocess.run(
        ["cargo", "build", "--release", "--locked"],
        cwd=str(root),
        stdout=sys.stderr,
        stderr=sys.stderr,
        check=False,
    )
    if result.returncode != 0:
        log(f"cargo build failed with exit code {result.returncode}")
        return None

    built = root / "target" / "release" / binary_name()
    if not is_runnable(built):
        log(f"the build finished but {built} is not there")
        return None

    destination = plugin_data() / "bin"
    destination.mkdir(parents=True, exist_ok=True)
    installed = destination / binary_name()
    shutil.copy2(built, installed)
    log(f"installed {installed}")
    return installed


def missing_binary_message(version: str) -> str:
    """What to tell a user who has no binary and no way to get one automatically."""
    return (
        "No img-gen-via-svg-mcp binary is available.\n"
        "  Any one of these fixes it:\n"
        f"    - run /img-gen-via-svg:setup in Claude Code, which builds it from the plugin's own sources\n"
        f"    - install it yourself:  cargo install --git https://github.com/{REPOSITORY}\n"
        f"    - point the plugin at an existing binary:  IMG_SVG_MCP_BIN=/path/to/{binary_name()}\n"
        f"  A download was attempted from the v{version} release of {REPOSITORY} and found nothing\n"
        "  for this platform; publishing that release makes the first start automatic."
    )


def run(binary: Path, arguments: Sequence[str]) -> int:
    """Hands the process over to the server so that stdio reaches it directly."""
    command = [str(binary), *arguments]
    if sys.platform == "win32":
        return subprocess.run(command, check=False).returncode
    os.execv(str(binary), command)
    return 1  # unreachable: execv replaces this process


def main(argv: Optional[Sequence[str]] = None) -> int:
    """Resolves a binary and either runs it or reports what is missing."""
    parser = argparse.ArgumentParser(
        prog="img-gen-via-svg launcher",
        description="Runs the img-gen-via-svg-mcp server, fetching or building it when needed.",
    )
    parser.add_argument(
        "--install",
        action="store_true",
        help="Make a binary available and report where it is, instead of running the server.",
    )
    parser.add_argument(
        "--build",
        action="store_true",
        help="With --install: compile from source rather than downloading a release.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="With --install: ignore a binary that is already present.",
    )
    known, passthrough = parser.parse_known_args(list(argv) if argv is not None else None)

    version = plugin_version()

    if known.install:
        existing = None if known.force else find_existing()
        if existing is not None:
            print(f"img-gen-via-svg-mcp is already available at {existing}")
            return 0
        binary = build_from_source() if known.build else (download(version) or build_from_source())
        if binary is None:
            print(missing_binary_message(version), file=sys.stderr)
            return 1
        print(f"img-gen-via-svg-mcp is ready at {binary}")
        return 0

    binary = find_existing()
    if binary is None and os.environ.get("IMG_SVG_MCP_NO_DOWNLOAD") not in {"1", "true", "yes"}:
        binary = download(version)
    if binary is None:
        print(missing_binary_message(version), file=sys.stderr)
        return 1

    return run(binary, passthrough)


if __name__ == "__main__":
    sys.exit(main())
