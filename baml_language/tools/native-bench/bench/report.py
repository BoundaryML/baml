"""Read a bundle, judge it against a previous one, and write the PR summary."""
import html
import json
import math
from pathlib import Path

from .rows import ORDER

NAMES = {"native": "BAML native", "native-off": "BAML native, telemetry off", "control": "control (frozen native)", "vm-off": "BAML VM, telemetry off", "vm-low": "BAML VM, low",
         "vm-medium": "BAML VM, medium", "vm-high": "BAML VM, high", "rust": "Rust", "go": "Go",
         "node": "Node", "bun": "Bun", "python": "Python"}
CASE_TITLES = {"00_startup": "Startup", "01_array_traversal": "Array traversal", "02_merge_sort": "Merge sort",
               "10_quick_sort": "Quick sort", "03_json_aggregate": "JSON aggregate",
               "04_allocation_retention": "Allocation and retention", "07_function_calls": "Function calls",
               "09_json_hello": "JSON hello", "11_generate_sort": "Generate and sort"}


class Bundle:
    def __init__(self, path):
        self.path = Path(path)
        self.manifest = json.loads((self.path / "manifest.json").read_text())
        self.summary = json.loads((self.path / "results" / "summary.json").read_text())
        for scen in self.summary.values():  # "native" and "native-off" are one row; the first bundle used the short name
            if "native" in scen and "native-off" not in scen:
                scen["native-off"] = scen["native"]
            if "native-off" in scen and "native" not in scen:
                scen["native"] = scen["native-off"]
        self.protocol = json.loads((self.path / "results" / "protocol.json").read_text()) if (self.path / "results" / "protocol.json").exists() else {}

    def scenarios(self):
        order = [s["name"] for s in self.protocol.get("scenarios", [])] or list(self.summary)
        return [s for s in order if s in self.summary]

    def case_of(self, scenario):
        for s in self.protocol.get("scenarios", []):
            if s["name"] == scenario:
                return s["case"]
        return "?"

    def cell(self, scenario, row):
        return self.summary.get(scenario, {}).get(row)

    def us(self, scenario, row, k="median"):
        c = self.cell(scenario, row)
        v = c and c.get("execution_wall_ns", {}).get("per_job", {}).get(k)
        return v / 1e3 if v is not None else None

    def rss(self, scenario, row):
        c = self.cell(scenario, row)
        return c and c.get("peak_rss_bytes", {}).get("median")


def fmt_us(v):
    if v is None:
        return "—"
    if v >= 100000:
        return f"{v / 1000:,.0f} ms"
    if v >= 1000:
        return f"{v / 1000:,.2f} ms"
    if v >= 10:
        return f"{v:,.1f} µs"
    return f"{v:.3f} µs"


def ratio(a, b):
    if a is None or b is None or b == 0:
        return "—"
    r = a / b
    return f"{r:.0f}×" if r >= 10 else f"{r:.1f}×" if r >= 1 else f"{r:.2f}×"


# ── verdicts ─────────────────────────────────────────────────────────────────

def verdicts(cur, prev, rule):
    """Per scenario: native change, control change (host drift), net, verdict."""
    thr = rule.get("threshold_percent", 5)
    out = []
    for scen in cur.scenarios():
        a, b = prev.us(scen, "native"), cur.us(scen, "native")
        if a is None or b is None:
            continue
        d = (b - a) / a * 100
        ca, cb = prev.us(scen, "control"), cur.us(scen, "control")
        drift = (cb - ca) / ca * 100 if ca and cb else None
        net = d - (drift or 0.0) if rule.get("net_of_control", True) else d
        disjoint = (cur.us(scen, "native", "min") > prev.us(scen, "native", "max")) or (cur.us(scen, "native", "max") < prev.us(scen, "native", "min"))
        significant = abs(net) > thr and (disjoint or not rule.get("require_disjoint_ranges", True))
        verdict = "unchanged" if not significant else ("faster" if net < 0 else "slower")
        out.append(dict(scenario=scen, before=a, after=b, delta=d, drift=drift, net=net, verdict=verdict))
    return out



def markdown(bundle_dir, previous=None):
    b = Bundle(bundle_dir)
    m = b.manifest
    lines = [f"## Native bench `{(m.get('candidate_sha') or '?')[:12]}` on {m.get('machine')}",
             "", "| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |", "|---|---:|---:|---:|---:|---:|---:|---:|"]
    for scen in b.scenarios():
        nat, vmo, vmm = b.us(scen, "native-off"), b.us(scen, "vm-off"), b.us(scen, "vm-medium")
        lines.append(f"| {scen} | {fmt_us(nat)} | {fmt_us(vmo)} | {fmt_us(vmm)} | {ratio(vmm, nat)} | {fmt_us(b.us(scen, 'go'))} | {ratio(b.us(scen, 'go'), nat)} | {ratio(b.us(scen, 'node'), nat)} |")
    lines += ["", "Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster."]
    if previous:
        p = Bundle(previous)
        vs = verdicts(b, p, b.protocol.get("regression", {}))
        n = {k: sum(1 for v in vs if v["verdict"] == k) for k in ("faster", "slower", "unchanged")}
        lines += ["", f"Against `{(p.manifest.get('candidate_sha') or '?')[:12]}`: **{n['faster']} faster, {n['slower']} slower, {n['unchanged']} unchanged** (net of control drift, threshold {b.protocol.get('regression', {}).get('threshold_percent', 5)}%)."]
        for v in vs:
            if v["verdict"] != "unchanged":
                lines.append(f"- {v['scenario']}: {fmt_us(v['before'])} → {fmt_us(v['after'])} ({v['delta']:+.1f}%, control {('%+.1f%%' % v['drift']) if v['drift'] is not None else '—'}) **{v['verdict']}**")
    return "\n".join(lines) + "\n"
