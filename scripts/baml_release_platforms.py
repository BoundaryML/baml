"""Shared wrapper release platform contract."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PLATFORMS = ROOT / "release" / "platforms.json"
WRAPPER_VARIANTS = frozenset({"self-update", "no-self-update"})


def fail(message: str) -> None:
    """Exit with a platform contract validation error."""
    raise SystemExit(message)


def require_string(value: Any, field: str) -> str:
    """Require a nonempty string field in the platform contract."""
    if not isinstance(value, str) or not value:
        fail(f"{field} must be a non-empty string")
    return value


@dataclass(frozen=True)
class WrapperTarget:
    """Validated wrapper release metadata for one Rust target triple."""

    triple: str
    os: str
    arch: str
    libc: str | None
    archive_suffix: str
    runner: str
    variants: tuple[str, ...]
    glibc_max: str | None

    @property
    def executable_suffix(self) -> str:
        """Return the target platform's executable suffix."""
        return ".exe" if self.os == "windows" else ""

    def supports(self, variant: str) -> bool:
        """Return whether this target publishes the requested wrapper variant."""
        return variant in self.variants

    def asset_name(self, version: str, variant: str) -> str:
        """Return the canonical release asset name for a supported variant."""
        if not self.supports(variant):
            fail(f"{self.triple}: wrapper variant {variant!r} is not supported")
        infix = "" if variant == "self-update" else f"-{variant}"
        return f"baml-wrapper{infix}-{version}-{self.triple}{self.archive_suffix}"

    def matrix_entry(self) -> dict[str, Any]:
        """Return this target's GitHub Actions wrapper matrix entry."""
        return {
            "target": self.triple,
            "os": self.runner,
            "libc": self.libc,
            "archive_suffix": self.archive_suffix,
            "executable_suffix": self.executable_suffix,
            "no_self_update": self.supports("no-self-update"),
            "glibc_max": self.glibc_max,
        }


def load_wrapper_targets(
    platforms: Path = DEFAULT_PLATFORMS,
) -> tuple[WrapperTarget, ...]:
    """Load and validate wrapper targets from the shared platform contract."""
    try:
        contract = json.loads(platforms.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"failed to read platform contract {platforms}: {exc}")
    if not isinstance(contract, dict) or contract.get("schema") != 1:
        fail("platform contract must be a schema-1 JSON object")
    raw_targets = contract.get("targets")
    if not isinstance(raw_targets, list):
        fail("platform contract targets must be a JSON array")

    seen: set[str] = set()
    targets: list[WrapperTarget] = []
    for index, raw_target in enumerate(raw_targets):
        if not isinstance(raw_target, dict):
            fail(f"platform target {index} must be a JSON object")
        triple = require_string(
            raw_target.get("triple"), f"platform target {index}.triple"
        )
        if triple in seen:
            fail(f"duplicate platform target: {triple}")
        seen.add(triple)

        artifacts = raw_target.get("artifacts")
        if not isinstance(artifacts, dict):
            fail(f"{triple}: artifacts must be a JSON object")
        wrapper = artifacts.get("wrapper")
        if wrapper is None:
            continue
        if not isinstance(wrapper, dict):
            fail(f"{triple}: wrapper artifact must be a JSON object")

        os_name = require_string(raw_target.get("os"), f"{triple}: os")
        if os_name not in {"linux", "macos", "windows"}:
            fail(f"{triple}: unsupported os {os_name!r}")
        arch = require_string(raw_target.get("arch"), f"{triple}: arch")
        libc = raw_target.get("libc")
        if libc is not None and not isinstance(libc, str):
            fail(f"{triple}: libc must be a string or null")
        archive_suffix = require_string(
            raw_target.get("archive_suffix"), f"{triple}: archive_suffix"
        )
        expected_suffix = ".zip" if os_name == "windows" else ".tar.gz"
        if archive_suffix != expected_suffix:
            fail(f"{triple}: archive_suffix must be {expected_suffix!r} for {os_name}")

        runner = require_string(wrapper.get("runner"), f"{triple}: wrapper.runner")
        runner_marker = {"linux": "ubuntu", "macos": "macos", "windows": "windows"}[
            os_name
        ]
        if runner_marker not in runner.lower():
            fail(f"{triple}: wrapper runner {runner!r} does not match {os_name}")

        raw_variants = wrapper.get("variants")
        if not isinstance(raw_variants, list) or not raw_variants:
            fail(f"{triple}: wrapper.variants must be a non-empty JSON array")
        if any(not isinstance(variant, str) for variant in raw_variants):
            fail(f"{triple}: wrapper variants must be strings")
        variants = tuple(raw_variants)
        if len(variants) != len(set(variants)):
            fail(f"{triple}: wrapper variants must be unique")
        unknown = sorted(set(variants) - WRAPPER_VARIANTS)
        if unknown:
            fail(f"{triple}: unknown wrapper variants: {unknown}")
        if "self-update" not in variants:
            fail(f"{triple}: wrapper variants must include 'self-update'")

        glibc_max = wrapper.get("glibc_max")
        if libc == "gnu":
            glibc_max = require_string(glibc_max, f"{triple}: wrapper.glibc_max")
            glibc_parts = glibc_max.split(".")
            if len(glibc_parts) < 2 or not all(part.isdigit() for part in glibc_parts):
                fail(f"{triple}: wrapper.glibc_max must be a dotted numeric version")
        elif glibc_max is not None:
            fail(f"{triple}: wrapper.glibc_max is only valid for GNU targets")

        targets.append(
            WrapperTarget(
                triple=triple,
                os=os_name,
                arch=arch,
                libc=libc,
                archive_suffix=archive_suffix,
                runner=runner,
                variants=variants,
                glibc_max=glibc_max,
            )
        )

    if not targets:
        fail("platform contract defines no wrapper targets")
    return tuple(targets)


def expected_wrapper_assets(
    version: str, platforms: Path = DEFAULT_PLATFORMS
) -> tuple[str, ...]:
    """Return every wrapper archive required for a release version."""
    return tuple(
        target.asset_name(version, variant)
        for target in load_wrapper_targets(platforms)
        for variant in target.variants
    )


def verify_wrapper_artifacts(
    wrapper_dir: Path, version: str, platforms: Path = DEFAULT_PLATFORMS
) -> None:
    """Require the directory to contain exactly the contracted wrapper archives."""
    if not wrapper_dir.is_dir():
        fail(f"wrapper artifact directory does not exist: {wrapper_dir}")
    expected = set(expected_wrapper_assets(version, platforms))
    actual = {
        path.name
        for path in wrapper_dir.iterdir()
        if path.is_file()
        and path.name.startswith("baml-wrapper-")
        and path.name.endswith((".tar.gz", ".zip"))
    }
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    if missing or extra:
        fail(f"wrapper artifact mismatch missing={missing} extra={extra}")
