"""The implementation rows: what runs, at which telemetry level, from which artifact."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Row:
    name: str
    kind: str          # "vm" | "native" | "control" | "rust" | "go" | "python" | "node" | "bun"
    level: str = "off"  # BAML_TELEMETRY for the child
    sink: str = "none"  # BAML recording sink: none | local
    supported: bool = True  # False: listed on every chart as "not yet", never run

    @property
    def baml(self):
        return self.kind in ("vm", "native", "control")

    @property
    def collector(self):
        """Whether a GC counter is expected (null is 'unavailable', never zero)."""
        return self.kind in ("vm", "go", "python")

    def artifact(self, layout, case):
        if self.kind == "vm":
            return layout.artifacts / case / "program"
        if self.kind == "native":
            return layout.artifacts / case / "native"
        if self.kind == "control":
            return layout.artifacts / case / "native-control"
        if self.kind in ("rust", "go"):
            return layout.artifacts / case / self.kind
        return layout.references / case / f"workload.{'py' if self.kind == 'python' else 'ts'}"

    def command(self, layout, case, input_path, jobs, warmup, recordings):
        art = self.artifact(layout, case)
        if self.kind == "vm":
            return [str(layout.host), "run", str(art), str(input_path), str(jobs), str(warmup), self.sink, str(recordings)]
        if self.kind in ("native", "control"):
            return [str(art), "run", str(art), str(input_path), str(jobs), str(warmup), self.sink, str(recordings)]
        if self.kind in ("rust", "go"):
            return [str(art), str(input_path), str(jobs), str(warmup)]
        adapter = layout.references / "adapters" / ("main.py" if self.kind == "python" else "main.ts")
        return [str(layout.tool(self.kind)), str(adapter), str(art), str(input_path), str(jobs), str(warmup)]


ROWS = {r.name: r for r in [
    Row("native-off", "native", "off", "none"),
    Row("native-low", "native", "low", "local", supported=False),
    Row("native-medium", "native", "medium", "local", supported=False),
    Row("native-high", "native", "high", "local", supported=False),
    Row("control", "control"),
    Row("vm-off", "vm", "off", "none"),
    Row("vm-low", "vm", "low", "local"),
    Row("vm-medium", "vm", "medium", "local"),
    Row("vm-high", "vm", "high", "local"),
    Row("rust", "rust"),
    Row("go", "go"),
    Row("node", "node"),
    Row("bun", "bun"),
    Row("python", "python"),
]}

ORDER = list(ROWS)

# Backward name: the first bundle called the native row "native".
ROWS["native"] = ROWS["native-off"]
