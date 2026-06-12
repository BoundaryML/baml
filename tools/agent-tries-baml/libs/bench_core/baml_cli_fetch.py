"""Fetch the `baml-cli` binary matching a BAML release tag (ported from baml-changelog2).

The deployed image pip-installs one `baml-py` (a different version line than the
`baml-language-*` releases), so its bundled `baml-cli` can disagree with the
syntax a given entry shows. To make code validation authoritative, we validate
each entry's snippets with the CLI built from *that exact release*.

Every `boundaryml/baml` release ships prebuilt CLI tarballs as assets, e.g.
`baml-language-0.11.3-nightly.20260608.a-x86_64-unknown-linux-gnu.tar.gz`, each
containing `bin/baml-cli` (+ its `baml-pack-host` helper). `resolve_cli(tag)`
downloads the asset for the current platform, extracts it into a per-tag cache
dir, and returns the path to `bin/baml-cli`. Repeated calls reuse the cache; a
small LRU cap keeps disk bounded. Any failure returns None so the caller can
fall back to the image's `baml-cli` rather than failing the request.
"""

from __future__ import annotations

import logging
import os
import platform
import tarfile
import tempfile
import threading
import time

import httpx

log = logging.getLogger("changelog.baml_cli")

# Direct release-asset URLs (no API, no auth, no rate limit).
_RELEASE_BASE = "https://github.com/boundaryml/baml/releases/download"
# Where extracted CLIs live. Override for deploys; defaults under the system temp
# dir (ephemeral, re-fetched on cold start, which is fine).
CACHE_DIR = os.environ.get(
    "BAML_CLI_CACHE_DIR", os.path.join(tempfile.gettempdir(), "baml-cli-cache")
)
# Keep at most this many pinned versions on disk (each ~30-40MB extracted).
_MAX_CACHED = int(os.environ.get("BAML_CLI_CACHE_MAX", "6"))
_DOWNLOAD_TIMEOUT_S = int(os.environ.get("BAML_CLI_DOWNLOAD_TIMEOUT_S", "120"))

# One lock serializes downloads so concurrent validations don't race on the same
# tag (the common case: a batch of entries on one release).
_LOCK = threading.Lock()


def _platform_triple() -> str | None:
    """The Rust target triple naming the release asset for this machine."""
    sysname = platform.system()
    machine = platform.machine().lower()
    arch = (
        "aarch64" if machine in ("arm64", "aarch64")
        else "x86_64" if machine in ("x86_64", "amd64")
        else None
    )
    if arch is None:
        return None
    if sysname == "Linux":
        # The deploy image (python:3.12-slim) is glibc; musl builds exist too but
        # we target the gnu asset to match Debian.
        return f"{arch}-unknown-linux-gnu"
    if sysname == "Darwin":
        return f"{arch}-apple-darwin"
    return None


def _bin_path(tag: str) -> str:
    return os.path.join(CACHE_DIR, tag, "bin", "baml-cli")


def _evict_if_needed() -> None:
    """Bounded LRU: drop the oldest extracted versions over the cap."""
    try:
        if not os.path.isdir(CACHE_DIR):
            return
        entries = [
            (os.path.getmtime(os.path.join(CACHE_DIR, d)), d)
            for d in os.listdir(CACHE_DIR)
            if os.path.isdir(os.path.join(CACHE_DIR, d))
        ]
        for _, d in sorted(entries)[: max(0, len(entries) - _MAX_CACHED + 1)]:
            import shutil
            shutil.rmtree(os.path.join(CACHE_DIR, d), ignore_errors=True)
    except Exception as e:  # eviction is best-effort
        log.warning("baml-cli cache eviction failed: %s", e)


def resolve_cli(tag: str) -> str | None:
    """Path to the `baml-cli` matching `tag` (e.g. `baml-language-0.11.3-...`),
    fetching + caching it on first use. None on any failure (caller falls back)."""
    if not tag:
        return None
    cached = _bin_path(tag)
    if os.path.exists(cached):
        return cached

    triple = _platform_triple()
    if triple is None:
        log.warning("baml-cli: unsupported platform %s/%s", platform.system(), platform.machine())
        return None

    with _LOCK:
        # Re-check inside the lock: another thread may have just fetched it.
        if os.path.exists(cached):
            return cached
        asset = f"{tag}-{triple}.tar.gz"
        url = f"{_RELEASE_BASE}/{tag}/{asset}"
        dest = os.path.join(CACHE_DIR, tag)
        try:
            _evict_if_needed()
            os.makedirs(dest, exist_ok=True)
            tar_path = os.path.join(dest, "cli.tar.gz")
            with httpx.stream("GET", url, follow_redirects=True,
                              timeout=_DOWNLOAD_TIMEOUT_S) as r:
                if r.status_code != 200:
                    log.warning("baml-cli: %s -> HTTP %s", url, r.status_code)
                    return None
                with open(tar_path, "wb") as f:
                    for chunk in r.iter_bytes():
                        f.write(chunk)
            with tarfile.open(tar_path, "r:gz") as tf:
                tf.extractall(dest, filter="data")
            os.remove(tar_path)
            if os.path.exists(cached):
                os.chmod(cached, 0o755)
                log.info("baml-cli: pinned %s for validation", tag)
                return cached
            log.warning("baml-cli: %s extracted but bin/baml-cli missing", asset)
            return None
        except Exception as e:  # network / extract / disk -- degrade gracefully
            log.warning("baml-cli: could not fetch %s: %s", url, e)
            return None
