#!/usr/bin/env python3
"""Render resource benchmark JSONL using only the Python standard library."""

from __future__ import annotations

import argparse
import csv
import html
from html.parser import HTMLParser
import json
import math
from pathlib import Path
import re
import statistics


WORKLOADS = {
    "tiny": "Tiny roots",
    "calls": "Dense calls",
    "spawn": "Spawn throughput",
    "async": "Bounded async",
    "burst": "Bursty + idle",
}
MODES = {
    "off": ("Off", "#64748b"),
    "auto-no-sink": ("Auto - no sink", "#2563eb"),
    "local": ("Local files", "#ea580c"),
    "cloud-fast": ("Cloud - fast", "#16a34a"),
    "cloud-slow": ("Cloud - slow", "#9333ea"),
}
METRICS = (
    ("cpu_percent", "CPU %", 1),
    ("peak_rss_bytes", "Peak RSS MiB", 2**20),
    ("output_bytes", "Output MiB", 2**20),
    ("drain_ms", "Exact drain ms", 1),
)
SUMMARY_FIELDS = (
    "work_per_s", "cpu_percent", "peak_rss_bytes", "output_bytes",
    "drain_ms", "execute_ms", "load_ms", "setup_ms", "work_units",
    "invocations", "read_bytes", "write_bytes",
)


def _escape(value):
    return html.escape(str(value), quote=True)


def _number(value):
    return (
        isinstance(value, (int, float))
        and not isinstance(value, bool)
        and math.isfinite(value)
        and value >= 0
    )


def _invalid_reason(run, metadata):
    if run.get("error"):
        return str(run["error"])
    if run.get("capped") or run.get("memory_capped"):
        return "Memory/resource cap reached"
    limit = metadata.get("memory_limit_bytes")
    peak = run.get("peak_rss_bytes")
    if _number(limit) and limit > 0 and _number(peak) and peak > limit:
        return "Observed RSS exceeded memory limit"
    if run.get("telemetry_success") is not True:
        return "Telemetry unsuccessful, disabled, or not confirmed"
    required = ("peak_rss_bytes", "output_bytes", "drain_ms", "execute_ms", "work_per_s")
    if any(not _number(run.get(key)) for key in required):
        return "Missing or invalid required metrics"
    return ""


def _values(runs, key):
    return [r[key] for r in runs if _number(r.get(key))]


def _median(runs, key):
    values = _values(runs, key)
    return statistics.median(values) if values else None


def _format(value, digits=2):
    return "n/a" if value is None else f"{value:,.{digits}f}"


def _range(runs, key, scale=1):
    values = _values(runs, key)
    if not values:
        return "n/a"
    return (
        f"{_format(statistics.median(values) / scale)} "
        f"[{_format(min(values) / scale)}, {_format(max(values) / scale)}]"
    )


def _ratio(value, baseline):
    if value is None or baseline is None or baseline == 0:
        return None
    return value / baseline


def _table(headers, rows):
    return (
        '<div class="table-wrap"><table><thead><tr>'
        + "".join(f"<th>{_escape(h)}</th>" for h in headers)
        + "</tr></thead><tbody>"
        + "".join(
            "<tr>" + "".join(f"<td>{_escape(c)}</td>" for c in row) + "</tr>"
            for row in rows
        )
        + "</tbody></table></div>"
    )


class _ReferenceTable(HTMLParser):
    """Read only the first table following the named resource heading."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.heading = None
        self.heading_text = []
        self.ready = False
        self.active = False
        self.done = False
        self.cell = None
        self.row = []
        self.rows = []

    def handle_starttag(self, tag, attrs):
        if tag in ("h1", "h2", "h3", "h4"):
            self.heading = tag
            self.heading_text = []
        if tag == "table" and self.ready and not self.done:
            self.active = True
        if self.active and tag == "tr":
            self.row = []
        if self.active and tag in ("td", "th"):
            self.cell = []

    def handle_data(self, data):
        if self.heading:
            self.heading_text.append(data)
        if self.cell is not None:
            self.cell.append(data)

    def handle_endtag(self, tag):
        if tag == self.heading:
            title = " ".join("".join(self.heading_text).split()).casefold()
            if title == "work completed & resource use":
                self.ready = True
            self.heading = None
        if self.active and tag in ("td", "th") and self.cell is not None:
            self.row.append(" ".join("".join(self.cell).split()))
            self.cell = None
        if self.active and tag == "tr" and self.row:
            self.rows.append(self.row)
        if self.active and tag == "table":
            self.active = False
            self.done = True


def _reference_metrics(path):
    parser = _ReferenceTable()
    parser.feed(path.read_text(encoding="utf-8"))
    if not parser.rows:
        raise ValueError("No 'Work completed & resource use' table found")
    headers = [h.casefold() for h in parser.rows[0]]
    columns = {}
    for key, prefix in (
        ("work_per_s", "work / second"),
        ("cpu_percent", "cpu"),
        ("peak_rss_bytes", "peak rss"),
    ):
        columns[key] = next(i for i, h in enumerate(headers) if h.startswith(prefix))
    result = {}
    for row in parser.rows[1:]:
        label = row[0].casefold()
        workload = next((k for k, v in WORKLOADS.items() if label.startswith(v.casefold())), None)
        mode = (
            "auto-no-sink" if "no sink" in label else
            "local" if "local" in label else
            "off" if re.search(r"\boff\b", label) else None
        )
        if workload is None or mode is None:
            continue
        metrics = {}
        for key, column in columns.items():
            match = re.match(r"\s*([0-9,]+(?:\.[0-9]+)?)", row[column])
            if match:
                metrics[key] = float(match[1].replace(",", ""))
                if key == "peak_rss_bytes" and "mib" in headers[column]:
                    metrics[key] *= 2**20
        result[workload, mode] = metrics
    if not result:
        raise ValueError("No recognized reference workload/mode rows")
    return result


def _overview(groups):
    width, height = 1320, 1270
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" '
        'role="img" aria-labelledby="title desc">',
        '<title id="title">Telemetry resource benchmark</title>',
        '<desc id="desc">Five workloads, four resource panels. Bars show valid-run '
        'medians and whiskers show min/max. Missing runs are marked n/a.</desc>',
        '<rect width="100%" height="100%" fill="#fff"/>',
        '<g font-family="system-ui, sans-serif" font-size="12" fill="#172033">',
        '<text x="24" y="30" font-size="22" font-weight="700">Telemetry resource benchmark</text>',
        '<text x="24" y="54">Median with min/max; independent panel scales. CPU: 100% = one core.</text>',
    ]
    for i, (mode, (label, color)) in enumerate(MODES.items()):
        x = 24 + i * 245
        parts.extend((
            f'<rect x="{x}" y="72" width="12" height="12" fill="{color}"/>',
            f'<text x="{x + 19}" y="83">{_escape(label)}</text>',
        ))
    for row, (workload, label) in enumerate(WORKLOADS.items()):
        y = 115 + row * 229
        parts.append(f'<text x="24" y="{y}" font-size="18" font-weight="700">{_escape(label)}</text>')
        for col, (metric, title, scale) in enumerate(METRICS):
            x = 24 + col * 325
            all_values = [
                value / scale for mode in MODES
                for value in _values(groups.get((workload, mode), []), metric)
            ]
            maximum = max(all_values, default=0) or 1
            parts.append(f'<text x="{x}" y="{y + 24}" font-weight="600">{title}</text>')
            parts.append(f'<text x="{x + 300}" y="{y + 24}" text-anchor="end">max {_format(maximum)}</text>')
            for i, (mode, (name, color)) in enumerate(MODES.items()):
                by = y + 42 + i * 32
                values = [v / scale for v in _values(groups.get((workload, mode), []), metric)]
                parts.append(f'<text x="{x}" y="{by + 12}" font-size="10">{_escape(name)}</text>')
                if not values:
                    parts.append(f'<text x="{x + 118}" y="{by + 12}" fill="#9f1239">n/a</text>')
                    continue
                lo, hi, mid = min(values), max(values), statistics.median(values)
                origin, span = x + 112, 118
                left, right = origin + span * lo / maximum, origin + span * hi / maximum
                parts.append(f'<rect x="{origin}" y="{by}" width="{span * mid / maximum:.3f}" height="17" fill="{color}"/>')
                parts.append(f'<path d="M {left:.3f} {by + 8} H {right:.3f} M {left:.3f} {by + 3} V {by + 14} M {right:.3f} {by + 3} V {by + 14}" stroke="#172033" fill="none"/>')
                parts.append(f'<text x="{x + 303}" y="{by + 12}" text-anchor="end" font-size="10">{_format(mid)}</text>')
    parts.append("</g></svg>")
    return "\n".join(parts)


def render_report(runs: list[dict], metadata: dict, output: Path, reference: Path | None) -> None:
    """Write an offline HTML report, SVG overview, and per-group summary CSV."""
    output = Path(output)
    groups = {}
    attempted = {}
    failures = []
    for run in runs:
        key = (run.get("workload"), run.get("mode"))
        attempted.setdefault(key, []).append(run)
        reason = _invalid_reason(run, metadata)
        if reason:
            failures.append((run.get("workload"), run.get("mode"), run.get("trial"), reason))
        else:
            groups.setdefault(key, []).append(run)
    overview = _overview(groups)
    (output / "overview.svg").write_text(overview, encoding="utf-8")
    keys = [(workload, mode) for workload in (*WORKLOADS, "cold") for mode in MODES]
    with (output / "summary.csv").open("w", encoding="utf-8", newline="") as stream:
        fields = ["workload", "mode", "valid_runs", "attempted_runs", "work_unit"]
        fields += [f"{metric}_{stat}" for metric in SUMMARY_FIELDS for stat in ("median", "min", "max", "n")]
        writer = csv.DictWriter(stream, fields)
        writer.writeheader()
        for workload, mode in keys:
            valid = groups.get((workload, mode), [])
            row = {
                "workload": workload, "mode": mode, "valid_runs": len(valid),
                "attempted_runs": len(attempted.get((workload, mode), [])),
                "work_unit": ", ".join(sorted({str(r.get("work_unit", "")) for r in valid})),
            }
            for metric in SUMMARY_FIELDS:
                values = _values(valid, metric)
                for stat, value in zip(
                    ("median", "min", "max", "n"),
                    (statistics.median(values), min(values), max(values), len(values)) if values else ("", "", "", 0),
                ):
                    row[f"{metric}_{stat}"] = value
            writer.writerow(row)
    content = [
        "<h1>BAML telemetry: resource benchmark</h1>",
        "<p>Native runtime controls on a shared machine. Medians and observed min/max "
        "are descriptive, not statistical speed claims.</p>",
        _table(("Environment", "Value"), [(k, v) for k, v in metadata.items()]),
        '<p><a href="overview.svg">SVG overview</a> | <a href="summary.csv">Summary CSV</a></p>',
        overview,
        "<h2>Work completed &amp; resource use</h2>",
        "<p>Values are median [min, max] across valid trials. CPU is execution-phase "
        "process CPU (100% = one core), not whole-machine utilization. Output is "
        "completed local file bytes or ACKed cloud HTTP body bytes, not OS writes. "
        "Drain uses exact native timers, not sampling intervals.</p>",
    ]
    rows = []
    for workload, mode in keys:
        if workload == "cold":
            continue
        valid = groups.get((workload, mode), [])
        count = len(attempted.get((workload, mode), []))
        units = ", ".join(sorted({str(r.get("work_unit", "")) for r in valid}))
        rows.append((
            WORKLOADS[workload], MODES[mode][0], f"{len(valid)}/{count}",
            _range(valid, "work_per_s") + " " + units + "/s",
            *(_range(valid, metric, scale) for metric, _, scale in METRICS),
        ))
    content.append(_table(
        ("Workload", "Mode", "Valid / attempted", "Work / second", "CPU %", "Peak RSS MiB", "Output MiB", "Drain ms"),
        rows,
    ))
    content += [
        "<h2>Within-workload ratios</h2>",
        "<p>Ratios of valid-run medians, not paired-trial ratios. CPU and RSS above 1 "
        "mean more resources; throughput above 1 means more completed work per second. "
        "A missing or zero denominator is n/a. Fixed-time runs do different amounts "
        "of work; CPU ratios are not per-operation overhead.</p>",
    ]
    ratios = []
    ratio_keys = ("cpu_percent", "work_per_s", "peak_rss_bytes")
    for workload in WORKLOADS:
        for mode, baseline in [(m, "off") for m in MODES] + [("local", "auto-no-sink")]:
            ratios.append((
                WORKLOADS[workload], f"{MODES[mode][0]} / {MODES[baseline][0]}",
                *(_format(_ratio(_median(groups.get((workload, mode), []), metric),
                                 _median(groups.get((workload, baseline), []), metric)), 3)
                  for metric in ratio_keys),
            ))
    content.append(_table(("Workload", "Comparison", "CPU ratio", "Throughput ratio", "RSS ratio"), ratios))
    if reference is not None:
        content += [
            "<h2>Reference M2 report: normalized comparison</h2>",
            "<p>Only the first reference 'Work completed &amp; resource use' table is "
            "read. Compare each report's own mode/off and local/no-sink ratios, "
            "not absolute metrics across machines, revisions, or methodology. "
            "Reference medians were rounded; these ratios are approximate.</p>",
        ]
        try:
            old = _reference_metrics(Path(reference))
            comparisons = []
            for workload in WORKLOADS:
                for mode, baseline in (("off", "off"), ("auto-no-sink", "off"), ("local", "off"), ("local", "auto-no-sink")):
                    for metric in ratio_keys:
                        comparisons.append((
                            WORKLOADS[workload], f"{mode} / {baseline}", metric,
                            _format(_ratio(_median(groups.get((workload, mode), []), metric),
                                           _median(groups.get((workload, baseline), []), metric)), 3),
                            _format(_ratio(old.get((workload, mode), {}).get(metric),
                                           old.get((workload, baseline), {}).get(metric)), 3),
                        ))
            content.append(_table(("Workload", "Comparison", "Metric", "Current ratio", "M2 ratio"), comparisons))
        except (OSError, ValueError, StopIteration, IndexError) as error:
            content.append(f'<p class="failure">Reference comparison unavailable: {_escape(error)}</p>')
    content += [
        "<h2>One cold root</h2>",
        "<p>Fresh process, one invocation. Exact timers include first-call costs, "
        "not steady-state call latency; no eight-root GC cycle.</p>",
        _table(
            ("Mode", "Valid / attempted", "Load ms", "Setup ms", "Execute ms", "Drain ms", "Peak RSS MiB"),
            [
                (MODES[mode][0],
                 f'{len(groups.get(("cold", mode), []))}/{len(attempted.get(("cold", mode), []))}',
                 *(_range(groups.get(("cold", mode), []), metric, scale) for metric, scale in (
                     ("load_ms", 1), ("setup_ms", 1), ("execute_ms", 1), ("drain_ms", 1), ("peak_rss_bytes", 2**20))))
                for mode in MODES
            ],
        ),
        "<h2>Run validity</h2>",
        "<p>Failed, capped, or unconfirmed telemetry runs are excluded from all "
        "medians and ratios. Missing or disabled cloud modes are n/a, never zero-cost results. "
        "The valid/attempted counts expose partial failures.</p>",
        _table(("Workload", "Mode", "Trial", "Excluded reason"), failures) if failures else "<p>No excluded runs.</p>",
        "<h2>Methodology and limits</h2><ul>",
        "<li>Shared developer machine: background activity, power state, and scheduling affect results. "
        "Controls are not statistical speed claims.</li>",
        "<li>Fixed-time execution windows can complete different amounts of work. "
        "Forced major GC every eight roots is included, as are deliberate waits and idle gaps.</li>",
        "<li>RSS is sampled (100 ms by default; see sample_ms above), not true kernel high-water memory. "
        "Short spikes can be missed, and an observed-memory cap is not a kernel-enforced limit.</li>",
        "<li>Compilation occurs outside the measured child. Load, setup, execution, and drain "
        "are separate phases; cold-root measurements are reported separately.</li>",
        "<li>Execution CPU uses external samples at phase-marker receipt; sub-millisecond "
        "notification latency can affect very short phases. Drain is timed inside the native runner.</li>",
        "<li>Cloud HTTP server CPU/RSS is excluded from child resource measurements. "
        "HTTP loopback is a transport control, not S3 or real network performance.</li>",
        "<li>These exact reference workloads use the ordinary Auto capture policy, without "
        "synthetic LLM metadata. They test span/aggregate delivery, not large CAS payloads.</li>",
        "<li>The reference runtime revision and worker count are not pinned by its HTML. "
        "Matching the recipe is not an identical-binary, identical-machine reproduction.</li>",
        "<li>No fsync: completed local output is not a power-loss durability guarantee. "
        "ACKed HTTP body bytes are not object-store durability or raw wire bytes.</li>",
        "<li>OS read/write counters include caching and process startup effects and need not "
        "equal completed telemetry output bytes. CPU counters are cumulative process time; "
        "interval CPU must use counter deltas, not cumulative CPU divided by each sample time.</li>",
        "</ul>",
    ]
    document = (
        '<!doctype html><html lang="en"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width, initial-scale=1">'
        '<title>BAML telemetry resource benchmark</title><style>'
        'body{font:15px/1.55 system-ui,sans-serif;color:#172033;background:#f5f7fa;'
        'max-width:1440px;margin:auto;padding:28px}h1,h2{line-height:1.2}'
        'h2{margin-top:36px}.table-wrap{overflow-x:auto;background:white;border-radius:8px}'
        'table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}'
        'th,td{text-align:left;padding:9px 12px;border-bottom:1px solid #e2e8f0;white-space:nowrap}'
        'th{background:#eaf0f7}svg{width:100%;height:auto;margin-top:24px}'
        'a{color:#1d4ed8}.failure{color:#9f1239}li{margin:8px 0}'
        '@media print{body{padding:0;background:white}table{font-size:10px}}'
        '</style></head><body>' + "\n".join(content) + "</body></html>"
    )
    (output / "report.html").write_text(document, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", required=True, type=Path, help="Run records as JSONL")
    parser.add_argument("--metadata", required=True, type=Path, help="Metadata JSON object")
    parser.add_argument("--output", required=True, type=Path, help="Existing output directory")
    parser.add_argument("--reference", type=Path, help="Optional earlier HTML report")
    args = parser.parse_args()
    runs = [json.loads(line) for line in args.runs.read_text(encoding="utf-8").splitlines() if line.strip()]
    metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
    render_report(runs, metadata, args.output, args.reference)


if __name__ == "__main__":
    main()
