"""Resolve a BAML skill from a specific baml-skill git branch (skill-arena).

A normal run injects the baked-in skill (``BAML_SKILL_DIR``). A skill-arena member
run instead uses a particular branch of the baml-skill repo, so each variant in the
cohort onboards from a different version of the skill. This module shallow-clones the
requested branch into a per-(repo, ref) cache and returns its directory; the worker
concatenates the ``*/SKILL.md`` under it exactly as it does for the static skill.

Caching is clone-to-temp-then-atomic-rename under a per-ref asyncio lock, so multiple
member runs on one host requesting the same branch don't race or clone twice. It is a
read-only-once network op kept strictly off the non-arena path: when a task has no
``skillRef`` the worker never calls in here.
"""

from __future__ import annotations

import asyncio
import hashlib
import logging
import os
import shutil
import uuid
from pathlib import Path
from typing import Optional

log = logging.getLogger("baml_worker.skill_repo")

# The baml-skill repo arena branches live on; overridable for tests (point at a
# local git fixture) and for a fork.
SKILL_REPO_URL = os.environ.get(
    "ATB_SKILL_REPO_URL", "https://github.com/BoundaryML/baml-skill.git"
)
# Where resolved branch checkouts are cached on the worker's filesystem.
SKILL_CACHE_DIR = Path(os.environ.get("BAML_SKILL_CACHE_DIR", "/tmp/baml-skill-cache"))
# Bound the clone so a wedged network op can't hang a member run forever.
CLONE_TIMEOUT_SECS = int(os.environ.get("BAML_SKILL_CLONE_TIMEOUT_SECS", "120"))

# One lock per (repo, ref) so concurrent resolves of the same branch on this host
# serialize on the clone rather than racing the cache directory.
_locks: dict[str, asyncio.Lock] = {}


def _cache_key(repo_url: str, ref: str) -> str:
    """Return a filesystem-safe cache key for a (repo, ref) pair.

    Args:
        repo_url: The skill repo URL.
        ref: The git branch/ref.

    Returns:
        A short hex digest unique to the repo+ref.
    """
    return hashlib.sha256(f"{repo_url}\0{ref}".encode()).hexdigest()[:16]


async def resolve_skill_dir(ref: str, repo_url: str = SKILL_REPO_URL) -> Path:
    """Shallow-clone ``ref`` of the skill repo (cached) and return its directory.

    Returns the cached checkout when present; otherwise clones the branch into a
    temp dir and atomically renames it into the cache, serialized per (repo, ref).

    Args:
        ref: The git branch/ref to check out (e.g. ``main`` or ``exp-a``).
        repo_url: The skill repo URL; defaults to ``ATB_SKILL_REPO_URL``.

    Returns:
        The path to the checked-out repo (contains the ``*/SKILL.md`` files).

    Raises:
        RuntimeError: When the clone fails or times out.
    """
    key = _cache_key(repo_url, ref)
    dest = SKILL_CACHE_DIR / key
    if dest.is_dir():
        return dest
    lock = _locks.setdefault(key, asyncio.Lock())
    async with lock:
        if dest.is_dir():  # filled while we waited for the lock
            return dest
        SKILL_CACHE_DIR.mkdir(parents=True, exist_ok=True)
        tmp = SKILL_CACHE_DIR / f"{key}.tmp-{uuid.uuid4().hex[:8]}"
        try:
            proc = await asyncio.create_subprocess_exec(
                "git", "clone", "--depth", "1", "--branch", ref, repo_url, str(tmp),
                stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            )
            try:
                _, err = await asyncio.wait_for(proc.communicate(), timeout=CLONE_TIMEOUT_SECS)
            except asyncio.TimeoutError as e:
                proc.kill()
                raise RuntimeError(f"git clone of {ref} timed out after {CLONE_TIMEOUT_SECS}s") from e
            if proc.returncode != 0:
                raise RuntimeError(
                    f"git clone of {ref} from {repo_url} failed: {err.decode()[:500]}"
                )
            # Atomic publish: rename the temp checkout into place. If another path
            # populated dest first, keep that and drop ours.
            if dest.is_dir():
                shutil.rmtree(tmp, ignore_errors=True)
            else:
                os.replace(tmp, dest)
        finally:
            if tmp.exists():
                shutil.rmtree(tmp, ignore_errors=True)
    log.info("resolved skill branch %s -> %s", ref, dest)
    return dest


def concat_skill_dir(directory: Path) -> Optional[str]:
    """Concatenate every ``SKILL.md`` under a directory into one skill document.

    Mirrors the worker's static-skill concatenation: each ``SKILL.md`` is prefixed
    with its parent directory name and joined by a rule.

    Args:
        directory: Directory to search recursively for ``SKILL.md`` files.

    Returns:
        The combined skill markdown, or None when no ``SKILL.md`` exists under it.
    """
    parts = []
    for skill_md in sorted(directory.rglob("SKILL.md")):
        rel = skill_md.parent.name
        parts.append(f"# BAML skill: {rel}\n\n{skill_md.read_text()}")
    return "\n\n---\n\n".join(parts) or None
