#!/usr/bin/env python3
"""Plot aligned RSS, allocator, heap, and permit metrics from an explicit-GC run."""

import argparse
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter, MultipleLocator


MIB = 1024 * 1024
COLORS = {
    "rss": "#202124",
    "allocator_live": "#1a73e8",
    "allocator_reserved": "#f29900",
    "baml_capacity": "#188038",
    "runtime_slots": "#d93025",
    "allocations": "#9334e6",
    "blocks": "#007b83",
    "permits": "#c26401",
    "gc": "#b3261e",
}


def load_samples(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


def series(samples, field, section="heap", scale=1):
    return [sample[section][field] / scale for sample in samples]


def thousands(value, _position):
    if abs(value) >= 1000:
        return f"{value / 1000:g}k"
    return f"{value:g}"


def decorate_gc(axis, samples):
    cycles = sorted({sample["cycle"] for sample in samples})
    for cycle in cycles:
        before = [sample["elapsed_seconds"] for sample in samples if sample["cycle"] == cycle and sample["phase"] == "before_gc"]
        after = [sample["elapsed_seconds"] for sample in samples if sample["cycle"] == cycle and sample["phase"].startswith("after_gc")]
        exact = [sample["elapsed_seconds"] for sample in samples if sample["cycle"] == cycle and sample["phase"] == "after_gc"]
        if before and after:
            axis.axvspan(before[0], max(after), color=COLORS["gc"], alpha=0.055, linewidth=0)
        if exact:
            axis.axvline(exact[0], color=COLORS["gc"], alpha=0.42, linewidth=0.9, linestyle="--")


def decorate_automatic_gc(axis, samples):
    loaded = [sample for sample in samples if sample["phase"] == "load"]
    for before, after in zip(loaded, loaded[1:]):
        if before["cycle"] == after["cycle"] and after["heap"]["allocations_since_gc"] < before["heap"]["allocations_since_gc"]:
            axis.axvline(after["elapsed_seconds"], color=COLORS["baml_capacity"], alpha=0.35, linewidth=0.9, linestyle=":")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence_dir", type=Path, help="Directory containing pure-samples.jsonl and python-samples.jsonl")
    parser.add_argument("--output", type=Path, help="PNG destination; defaults to gc-allocator-timeline.png in the evidence directory")
    parser.add_argument("--title", default="One-minute BAML GC and allocator accounting")
    parser.add_argument("--subtitle", default="1,000 RPS per target in parallel · 9 s load · explicit major GC · 1 s pause · shaded bands are GC/pause windows")
    parser.add_argument("--observation", action="append", help="Footer observation; repeat for multiple lines")
    parser.add_argument("--mark-automatic-gc", action="store_true", help="Mark sampled drops in allocations-since-GC during a loaded interval")
    args = parser.parse_args()
    evidence = args.evidence_dir.resolve()
    output = (args.output or evidence / "gc-allocator-timeline.png").resolve()
    targets = [
        ("Pure BAML", load_samples(evidence / "pure-samples.jsonl")),
        ("Python + BAML", load_samples(evidence / "python-samples.jsonl")),
    ]

    plt.style.use("seaborn-v0_8-whitegrid")
    figure, axes = plt.subplots(4, 2, figsize=(16, 13), sharex="col", constrained_layout=False)
    figure.subplots_adjust(left=0.075, right=0.98, top=0.88, bottom=0.15, hspace=0.22, wspace=0.16)
    figure.suptitle(args.title, fontsize=21, fontweight="bold", y=0.965)
    figure.text(0.5, 0.925, args.subtitle, ha="center", fontsize=12, color="#4a4a4a")

    for column, (title, samples) in enumerate(targets):
        elapsed = [sample["elapsed_seconds"] for sample in samples]
        for row in range(4):
            axis = axes[row][column]
            decorate_gc(axis, samples)
            if args.mark_automatic_gc:
                decorate_automatic_gc(axis, samples)
            axis.set_xlim(0, 61)
            axis.xaxis.set_major_locator(MultipleLocator(10))
            axis.grid(True, color="#d9d9d9", linewidth=0.65, alpha=0.72)
            axis.spines[["top", "right"]].set_visible(False)
        axes[0][column].set_title(title, fontsize=16, fontweight="bold", pad=12)

        memory = axes[0][column]
        memory.plot(elapsed, series(samples, "rss_bytes", "process", MIB), color=COLORS["rss"], linewidth=2.2, marker="o", markersize=2.8, label="RSS")
        memory.plot(elapsed, series(samples, "allocator_bytes_reserved", scale=MIB), color=COLORS["allocator_reserved"], linewidth=1.9, label="malloc reserved")
        memory.plot(elapsed, series(samples, "allocator_bytes_in_use", scale=MIB), color=COLORS["allocator_live"], linewidth=2.0, label="malloc live")
        memory.plot(elapsed, series(samples, "tracked_slot_capacity_bytes", scale=MIB), color=COLORS["baml_capacity"], linewidth=1.8, label="BAML slot capacity")
        memory.set_ylabel("Memory (MiB)")
        memory.set_ylim(bottom=0)
        memory.legend(loc="upper left", frameon=True, framealpha=0.94, fontsize=9, ncol=2)

        objects = axes[1][column]
        objects.plot(elapsed, series(samples, "runtime_objects"), color=COLORS["runtime_slots"], linewidth=2.0, marker="o", markersize=2.5, label="Reserved runtime slots")
        objects.plot(elapsed, series(samples, "allocations_since_gc"), color=COLORS["allocations"], linewidth=1.8, linestyle="-.", label="Objects allocated since GC")
        objects.set_ylabel("BAML objects / slots")
        objects.set_ylim(bottom=0)
        objects.yaxis.set_major_formatter(FuncFormatter(thousands))
        objects.legend(loc="upper left", frameon=True, framealpha=0.94, fontsize=9)

        blocks = axes[2][column]
        blocks.plot(elapsed, series(samples, "allocator_blocks_in_use"), color=COLORS["blocks"], linewidth=2.0, marker="o", markersize=2.5)
        blocks.set_ylabel("malloc blocks in use")
        blocks.set_ylim(bottom=0)
        blocks.yaxis.set_major_formatter(FuncFormatter(thousands))

        permits = axes[3][column]
        permits.plot(elapsed, series(samples, "permit_holder_slots"), color=COLORS["permits"], linewidth=2.0, marker="o", markersize=2.5)
        permits.set_ylabel("Weak permit registrations")
        permits.set_xlabel("Elapsed time (seconds)")
        permits.set_ylim(bottom=0)
        permits.yaxis.set_major_formatter(FuncFormatter(thousands))

    figure.text(0.075, 0.087, "How to read: rising BAML slots or permits means the runtime still tracks objects or roots.", fontsize=10.5, color="#303030")
    figure.text(0.075, 0.066, "Flat BAML counters + rising malloc-live bytes means live memory elsewhere. Flat malloc-live bytes + rising RSS means retained allocator pages or non-malloc mappings.", fontsize=10.5, color="#303030")
    observations = args.observation or [
        "Observed: every GC collapses BAML slots and permits. Pure BAML malloc-live bytes return to ~19 MiB; Python returns to ~33 MiB.",
        "RSS stays near the higher malloc-reserved watermark in both processes.",
    ]
    for index, observation in enumerate(observations[:2]):
        figure.text(0.075, 0.039 - index * 0.021, observation, fontsize=10.5, fontweight="bold", color="#303030")
    output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(output, dpi=180, facecolor="white")
    figure.savefig(output.with_suffix(".svg"), facecolor="white")
    print(output)
    print(output.with_suffix(".svg"))


if __name__ == "__main__":
    main()
