"""Fetch the BAML alpha-channel release binary.

The alpha channel is published as GitHub pre-releases named
`baml-language-0.11.0-alpha.<N>`, each shipping prebuilt CLI binaries per
platform (e.g. `...-x86_64-unknown-linux-gnu.tar.gz`). We download the
asset matching this machine's platform and extract the `baml` binary. No
compilation — this is the actual published alpha artifact.
"""

from __future__ import annotations

import io
import os
import platform
import tarfile

import httpx

REPO_SLUG = os.environ.get("BAML_REPO_SLUG", "BoundaryML/baml")
GITHUB_TOKEN = os.environ.get("GITHUB_TOKEN", "")
# GitHub API base; env-overridable for consistency with the other API clients.
GITHUB_API_BASE = os.environ.get("GITHUB_API_BASE", "https://api.github.com")


def _gh_headers() -> dict[str, str]:
    """Build the GitHub API request headers, adding auth when a token is set.

    Returns:
        The request headers, including a bearer `Authorization` header when
        `GITHUB_TOKEN` is configured.
    """
    h = {"Accept": "application/vnd.github+json"}
    if GITHUB_TOKEN:
        h["Authorization"] = f"Bearer {GITHUB_TOKEN}"
    return h


def _platform_triple() -> str:
    """Return the glibc Linux target triple for this machine's architecture.

    Returns:
        The target triple string (e.g. `x86_64-unknown-linux-gnu`).
    """
    m = platform.machine().lower()
    arch = "aarch64" if m in ("arm64", "aarch64") else "x86_64"
    # glibc (Debian-based images and Fly machines)
    return f"{arch}-unknown-linux-gnu"


def _extract_baml(targz: bytes) -> bytes:
    """Extract the baml binary from a gzipped release tarball.

    Prefers a file named `baml`, then `baml-cli`, then the largest file.

    Args:
        targz: The gzipped tarball bytes.

    Returns:
        The extracted binary's bytes.

    Raises:
        RuntimeError: If no binary is found or it cannot be read.
    """
    with tarfile.open(fileobj=io.BytesIO(targz), mode="r:gz") as tf:
        files = [m for m in tf.getmembers() if m.isfile()]
        named = [m for m in files if os.path.basename(m.name) in ("baml", "baml-cli")]
        # prefer a file literally named `baml`, then `baml-cli`, then largest
        pick = sorted(named, key=lambda m: os.path.basename(m.name) != "baml")
        if not pick:
            pick = sorted(files, key=lambda m: m.size, reverse=True)[:1]
        if not pick:
            raise RuntimeError("no binary found in release tarball")
        fobj = tf.extractfile(pick[0])
        if fobj is None:
            raise RuntimeError("could not read binary from tarball")
        return fobj.read()


async def fetch_baml(tag: str) -> bytes:
    """Download the alpha release `tag` asset for this platform; return the
    baml binary bytes.

    Args:
        tag: The alpha release tag (e.g. `baml-language-0.11.0-alpha.4166`).

    Returns:
        The extracted baml binary's bytes.

    Raises:
        RuntimeError: If no matching `.tar.gz` asset exists on the release.
    """
    triple = _platform_triple()
    async with httpx.AsyncClient(timeout=180.0, follow_redirects=True) as c:
        r = await c.get(
            f"{GITHUB_API_BASE}/repos/{REPO_SLUG}/releases/tags/{tag}",
            headers=_gh_headers(),
        )
        r.raise_for_status()
        assets = r.json().get("assets", [])
        asset = next(
            (a for a in assets if triple in a["name"] and a["name"].endswith(".tar.gz")),
            None,
        )
        if asset is None:
            raise RuntimeError(f"no {triple} .tar.gz asset on release {tag}")
        dl = await c.get(asset["browser_download_url"])
        dl.raise_for_status()
        data = dl.content
    return _extract_baml(data)
