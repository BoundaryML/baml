#!/usr/bin/env python3

import argparse
import json
from pathlib import Path
from typing import Dict, List


MIB = 1024 * 1024
COLORS = {
    "rss": "#202124",
    "live": "#1976d2",
    "reserved": "#ef6c00",
    "runtime": "#d32f2f",
    "allocated": "#7b1fa2",
    "blocks": "#00838f",
    "capacity": "#238b45",
    "permits": "#c76500",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Plot one or more micro-GC result directories")
    parser.add_argument("results", type=Path, nargs="+")
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def read_json(path: Path) -> Dict[str, object]:
    return json.loads(path.read_text())


def read_samples(path: Path) -> List[Dict[str, object]]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def main() -> None:
    args = parse_args()
    if len(args.results) > 1 and args.output is None:
        raise SystemExit("--output is required when comparing multiple result directories")
    try:
        import matplotlib.pyplot as plt
        from matplotlib.ticker import FuncFormatter
    except ImportError as error:
        raise SystemExit("matplotlib is required; run with `uv run --with matplotlib python plot.py ...`") from error

    datasets = []
    for result in args.results:
        result = result.expanduser().resolve()
        datasets.append((read_json(result / "config.json"), read_samples(result / "samples.jsonl")))
    output = (args.output or args.results[0] / "memory.png").expanduser().resolve()

    plt.style.use("seaborn-v0_8-whitegrid")
    figure, axes = plt.subplots(4, len(datasets), figsize=(9 * len(datasets), 20), sharex="col", squeeze=False)
    figure.suptitle("Pure BAML micro-GC workloads", fontsize=27, fontweight="bold", y=0.985)
    figure.text(0.5, 0.965, "no HTTP server or language bridge · macOS process and malloc metrics · independent column scales", ha="center", fontsize=13, color="#444444")

    for column, (config, samples) in enumerate(datasets):
        elapsed = [sample["elapsed_seconds"] for sample in samples]

        def heap(field: str, scale: int = 1) -> List[float]:
            return [sample["heap"][field] / scale for sample in samples]

        def process(field: str, scale: int = 1) -> List[float]:
            return [sample["process"][field] / scale for sample in samples]

        axes[0, column].set_title(f"{config['workload']}\n{config['description']}", fontsize=15, fontweight="bold", pad=12)
        axes[0, column].plot(elapsed, process("rss_bytes", MIB), color=COLORS["rss"], linewidth=2.6, label="RSS")
        axes[0, column].plot(elapsed, heap("allocator_bytes_in_use", MIB), color=COLORS["live"], linewidth=2.4, label="malloc live")
        axes[0, column].plot(elapsed, heap("allocator_bytes_reserved", MIB), color=COLORS["reserved"], linewidth=2.4, label="malloc reserved")
        axes[0, column].set_ylabel("Memory (MiB)")
        axes[0, column].legend(loc="upper left", fontsize=10, frameon=True)

        axes[1, column].plot(elapsed, heap("runtime_objects"), color=COLORS["runtime"], linewidth=2.5, label="Runtime slots")
        axes[1, column].plot(elapsed, heap("allocations_since_gc"), color=COLORS["allocated"], linewidth=2.2, linestyle="--", label="Allocations since GC")
        axes[1, column].set_ylabel("Objects / slots")
        axes[1, column].legend(loc="upper left", fontsize=10, frameon=True)

        axes[2, column].plot(elapsed, heap("allocator_blocks_in_use"), color=COLORS["blocks"], linewidth=2.5)
        axes[2, column].set_ylabel("malloc blocks in use")

        axes[3, column].plot(elapsed, heap("tracked_slot_capacity_bytes", MIB), color=COLORS["capacity"], linewidth=2.5, label="BAML slot capacity")
        axes[3, column].axhline(32, color=COLORS["capacity"], linewidth=1.5, linestyle=":", alpha=0.8, label="Minimum full-GC budget")
        axes[3, column].set_ylabel("Slot capacity (MiB)", color=COLORS["capacity"])
        axes[3, column].tick_params(axis="y", labelcolor=COLORS["capacity"])
        axes[3, column].set_xlabel("Elapsed time (seconds)")
        permits = axes[3, column].twinx()
        permits.plot(elapsed, heap("permit_holder_slots"), color=COLORS["permits"], linewidth=2, linestyle="--", label="Weak permits")
        permits.set_ylabel("Weak permit registrations", color=COLORS["permits"])
        permits.tick_params(axis="y", labelcolor=COLORS["permits"])

        for row in range(4):
            axes[row, column].set_xlim(0, max(elapsed))
            axes[row, column].set_ylim(bottom=0)
            axes[row, column].grid(True, color="#d7d7d7", linewidth=0.8)
            axes[row, column].spines["top"].set_visible(False)
            axes[row, column].spines["right"].set_visible(False)
            axes[row, column].tick_params(axis="both", labelsize=10)
            axes[row, column].yaxis.set_major_formatter(FuncFormatter(lambda value, _: f"{value:g}" if abs(value) < 1000 else f"{value / 1000:g}k"))

    figure.tight_layout(rect=[0.025, 0.025, 0.99, 0.945], h_pad=2.2, w_pad=3.5)
    output.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(output, dpi=130, bbox_inches="tight", facecolor="white")
    print(output)


if __name__ == "__main__":
    main()
