"""Unit coverage for the skill-arena branch resolver
(``services/baml_worker/skill_repo.py``).

Builds a tiny local git repo with two branches, each carrying a different
``SKILL.md``, and asserts ``resolve_skill_dir`` checks out the requested branch,
caches the checkout (a second call returns the same dir), and that
``concat_skill_dir`` reads the branch's skill. No network, no backend — just git.
"""

from __future__ import annotations

import subprocess

import pytest

from services.baml_worker.skill_repo import concat_skill_dir, resolve_skill_dir


def _git(args: list[str], cwd) -> None:
    """Run a git command in ``cwd``, raising on failure (output suppressed)."""
    subprocess.run(["git", *args], cwd=str(cwd), check=True, capture_output=True)


@pytest.fixture
def skill_origin(tmp_path):
    """Create a local git repo with `main` and `exp-a` branches, distinct SKILL.md.

    Args:
        tmp_path: Pytest temp dir.

    Returns:
        The path to the origin repo.
    """
    repo = tmp_path / "origin"
    repo.mkdir()
    _git(["init", "-q", "-b", "main"], repo)
    _git(["config", "user.email", "t@example.com"], repo)
    _git(["config", "user.name", "t"], repo)
    skills = repo / "plugins" / "baml" / "skills" / "core"
    skills.mkdir(parents=True)
    (skills / "SKILL.md").write_text("MAIN SKILL CONTENT")
    _git(["add", "-A"], repo)
    _git(["commit", "-qm", "main skill"], repo)
    _git(["checkout", "-q", "-b", "exp-a"], repo)
    (skills / "SKILL.md").write_text("EXP-A SKILL CONTENT")
    _git(["add", "-A"], repo)
    _git(["commit", "-qm", "exp-a skill"], repo)
    _git(["checkout", "-q", "main"], repo)
    return repo


async def test_resolve_skill_dir_selects_branch_and_caches(tmp_path, skill_origin, monkeypatch):
    """resolve_skill_dir checks out the right branch, concatenates it, and caches.

    Args:
        tmp_path: Pytest temp dir (holds the cache).
        skill_origin: The local origin repo fixture.
        monkeypatch: Used to redirect the module's cache dir into tmp.
    """
    cache = tmp_path / "cache"
    monkeypatch.setattr("services.baml_worker.skill_repo.SKILL_CACHE_DIR", cache)
    url = skill_origin.as_uri()  # file://... so --branch clone works locally

    d_main = await resolve_skill_dir("main", repo_url=url)
    d_exp = await resolve_skill_dir("exp-a", repo_url=url)
    assert d_main != d_exp
    assert "MAIN SKILL CONTENT" in (concat_skill_dir(d_main) or "")
    assert "EXP-A SKILL CONTENT" in (concat_skill_dir(d_exp) or "")

    # A second resolve of the same branch is a cache hit (same dir, no re-clone).
    assert await resolve_skill_dir("main", repo_url=url) == d_main


async def test_resolve_skill_dir_raises_on_bad_ref(tmp_path, skill_origin, monkeypatch):
    """A nonexistent branch surfaces as a RuntimeError (the worker then falls back).

    Args:
        tmp_path: Pytest temp dir (holds the cache).
        skill_origin: The local origin repo fixture.
        monkeypatch: Used to redirect the module's cache dir into tmp.
    """
    monkeypatch.setattr("services.baml_worker.skill_repo.SKILL_CACHE_DIR", tmp_path / "cache")
    with pytest.raises(RuntimeError):
        await resolve_skill_dir("no-such-branch", repo_url=skill_origin.as_uri())
