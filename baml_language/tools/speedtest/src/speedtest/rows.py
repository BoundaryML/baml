"""Rows: the implementations a workload is measured as.

Every chart draws every row of the run, grouped by family, so a missing
measurement is visible rather than silently absent. A BAML row names the
backend its executable is packed with and the telemetry level it runs at.
"""

import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

DRIVERS = Path(__file__).resolve().parent / "drivers"
TELEMETRY_LEVELS = ("off", "low", "medium", "high")


@dataclass(frozen=True)
class Row:
    key: str
    label: str
    family: str
    kind: str                       # "baml" | "python" | "node" | "bun"
    telemetry: str | None = None    # BAML_TELEMETRY for BAML rows
    backend: str = "bytecode"       # what `baml pack` emits: "bytecode" | "rust"

    @property
    def is_baml(self):
        return self.kind == "baml"

    def command(self, workload, artifact, jobs, warmup):
        if self.is_baml:
            return [str(artifact), "--jobs", str(jobs), "--warmup", str(warmup)]
        if self.kind == "python":
            return ["python3", "-S", str(DRIVERS / "driver.py"), str(workload["files"]["python"]), str(jobs), str(warmup)]
        return [self.kind, str(DRIVERS / "driver.js"), str(workload["files"]["js"]), str(jobs), str(warmup)]

    def env(self, base, baml_home):
        env = dict(base)
        if self.is_baml:
            env["BAML_TELEMETRY"] = self.telemetry
            # Each trial records into its own home, so recordings can be
            # counted per trial and never accumulate across the run.
            env["BAML_HOME"] = str(baml_home)
        return env


ROWS = [Row(f"vm-{lvl}", f"VM({lvl})", "BAML VM", "baml", lvl)
        for lvl in TELEMETRY_LEVELS] + [
    Row(f"rust-{lvl}", f"Rust({lvl})", "BAML Rust", "baml", lvl, "rust")
    for lvl in TELEMETRY_LEVELS] + [
    Row("node", "Node", "references", "node"),
    Row("bun", "Bun", "references", "bun"),
    Row("python", "Python", "references", "python"),
]
BY_KEY = {r.key: r for r in ROWS}
DEFAULT = ["vm-off", "node", "bun", "python"]
PRIMARY = "vm-off"


def toolchain_version(kind):
    """Version string of a reference toolchain, or None when it is not installed."""
    exe = {"python": "python3"}.get(kind, kind)
    if not shutil.which(exe):
        return None
    r = subprocess.run([exe, "--version"], capture_output=True, text=True)
    return (r.stdout or r.stderr).strip().splitlines()[0] if r.returncode == 0 else None


def resolve(keys, telemetry=False):
    """Rows for ``keys`` (default set when empty), plus every telemetry level of
    each BAML backend among them when asked."""
    keys = list(keys or DEFAULT)
    if telemetry:
        backends = {BY_KEY[k].backend for k in keys if k in BY_KEY and BY_KEY[k].is_baml}
        keys = [r.key for r in ROWS if r.is_baml and r.backend in backends] + [
            k for k in keys if not BY_KEY.get(k, ROWS[0]).is_baml]
    unknown = [k for k in keys if k not in BY_KEY]
    if unknown:
        raise SystemExit(f"unknown rows: {', '.join(unknown)} (known: {', '.join(BY_KEY)})")
    return [r for r in ROWS if r.key in keys]
