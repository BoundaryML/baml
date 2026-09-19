"""Preserved rate benchmark chart layout. Use plot.py as the entry point."""

import json
import statistics as stats

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D


def render(identity, runs, root):
    threads_list = sorted({r["threads"] for r in runs})
    max_rate = max(identity["target_calls_per_second"]) / 1e6
    lookup = {
        (r["threads"], r["requested_calls_per_second"], r["rep"], r["mode"]): r[
            "result"
        ]
        for r in runs
    }
    summary = []

    def describe(values):
        return {
            "median": stats.median(values),
            "min": min(values),
            "max": max(values),
            "samples": values,
        }

    for threads in threads_list:
        for rate in identity["target_calls_per_second"]:
            pairs = [
                (
                    lookup[threads, rate, i, "encode-clock"],
                    lookup[threads, rate, i, "feeder-only"],
                )
                for i in range(identity["repetitions"])
            ]
            row = {
                "threads": threads,
                "requested_calls_per_second": rate,
                "calls": identity["calls_per_run"],
            }
            for index, mode in ((0, "full"), (1, "baseline")):
                row[f"{mode}_achieved_calls_per_second"] = describe(
                    [row["calls"] / p[index]["replay_seconds"] for p in pairs]
                )
                row[f"{mode}_cpu_ns_per_call"] = describe(
                    [p[index]["replay_cpu_seconds"] * 1e9 / row["calls"] for p in pairs]
                )
                row[f"{mode}_elapsed_seconds"] = describe(
                    [p[index]["replay_seconds"] for p in pairs]
                )
                row[f"{mode}_peak_rss_bytes_including_preparation"] = describe(
                    [p[index]["peak_rss_bytes_including_preparation"] for p in pairs]
                )
            row["paired_difference_cpu_ns_per_call"] = describe(
                [
                    (a["replay_cpu_seconds"] - b["replay_cpu_seconds"])
                    * 1e9
                    / row["calls"]
                    for a, b in pairs
                ]
            )
            summary.append(row)
    (root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    plt.rcParams.update(
        {
            "font.family": "DejaVu Sans",
            "font.size": 10.5,
            "axes.spines.top": False,
            "axes.spines.right": False,
            "axes.titleweight": "bold",
        }
    )
    fig, axes = plt.subplots(2, 2, figsize=(14, 10.8), dpi=180)
    fig.patch.set_facecolor("#fafbfc")
    colors = {n: plt.get_cmap("tab10")(i % 10) for i, n in enumerate(threads_list)}
    colors.update({1: "#7c3aed", 4: "#2563eb", 32: "#0d9488"})
    metrics = [
        (
            axes[0, 0],
            "full_achieved_calls_per_second",
            1e-6,
            "Can the pipeline keep up?",
            "Achieved function calls / second (millions)",
        ),
        (
            axes[0, 1],
            "full_cpu_ns_per_call",
            1,
            "Total CPU cost",
            "Process CPU ns / function call",
        ),
        (
            axes[1, 0],
            "full_peak_rss_bytes_including_preparation",
            1 / 2**30,
            "Process peak memory, including prepared inputs",
            "Peak RSS (GiB)",
        ),
        (
            axes[1, 1],
            "paired_difference_cpu_ns_per_call",
            1,
            "Estimated additional CPU above feeder baseline",
            "Full − baseline CPU ns / function call",
        ),
    ]
    for threads in threads_list:
        rows = [r for r in summary if r["threads"] == threads]
        x = [r["requested_calls_per_second"] / 1e6 for r in rows]
        for ax, key, factor, title, ylabel in metrics:
            values = [r[key] for r in rows]
            ax.plot(
                x,
                [v["median"] * factor for v in values],
                "o-",
                color=colors[threads],
                linewidth=2,
                markersize=4.5,
                label=f"{threads} producer" + ("s" if threads != 1 else ""),
            )
            ax.fill_between(
                x,
                [v["min"] * factor for v in values],
                [v["max"] * factor for v in values],
                color=colors[threads],
                alpha=0.10,
            )
        axes[0, 1].plot(
            x,
            [r["baseline_cpu_ns_per_call"]["median"] for r in rows],
            ":",
            color=colors[threads],
            linewidth=1.8,
        )
    axes[0, 0].plot(
        [0, max_rate],
        [0, max_rate],
        "--",
        color="#94a3b8",
        linewidth=1.2,
        label="Keeping up (y = x)",
    )
    axes[0, 0].legend(frameon=False, loc="upper left", fontsize=9)
    axes[0, 1].legend(
        handles=[
            Line2D([0], [0], color="#475569", linewidth=2, label="Full replay"),
            Line2D(
                [0],
                [0],
                color="#475569",
                linestyle=":",
                linewidth=1.8,
                label="Feeder baseline",
            ),
        ],
        frameon=False,
        fontsize=9,
    )
    for ax, key, factor, title, ylabel in metrics:
        ax.set_title(title, loc="left", pad=12, fontsize=12)
        ax.set_ylabel(ylabel)
        ax.set_xlabel("Requested function calls / second (millions, total)")
        ax.set_xlim(0, max_rate * 1.0625)
        ax.set_xticks([max_rate * i / 8 for i in range(9)])
        ax.set_facecolor("#fafbfc")
        ax.grid(axis="y", alpha=0.18)
        ax.set_axisbelow(True)
        if key != "paired_difference_cpu_ns_per_call":
            ax.set_ylim(bottom=0)
        else:
            ax.axhline(0, color="#94a3b8", linewidth=0.8)
    fig.suptitle(
        "Function-call rate: where do we stop keeping up?",
        x=0.075,
        y=0.978,
        ha="left",
        fontsize=20,
        fontweight="bold",
    )
    fig.text(
        0.075,
        0.937,
        f"{identity['calls_per_run'] / 1e6:g} million calls per run • requested rate split evenly across producers • clock + encoding + rings + {identity.get('stage_description', 'discard drainer')}",
        fontsize=10.5,
        color="#475569",
    )
    fig.text(
        0.075,
        0.118,
        "1 call = enter + exit. Achieved rate = total calls / elapsed time through complete drain; "
        + (
            "includes span building and output counting."
            if identity["consumer_stage"] == "build-spans"
            else "this is not span-builder throughput."
        ),
        fontsize=10,
        color="#475569",
    )
    fig.text(
        0.075,
        0.088,
        f"Pacing uses batches of {runs[0]['result']['loads'][0]['batch_markers']:,} markers. {runs[0]['result']['memory_budget_bytes'] / 2**20:g} MiB transport budget. RSS includes prepared inputs and other process memory.",
        fontsize=10,
        color="#475569",
    )
    fig.text(
        0.075,
        0.058,
        f"Points: medians of {identity['repetitions']} runs/pairs. Bands: observed min–max, not confidence intervals. Dotted CPU curves: feeder baseline.",
        fontsize=10,
        color="#475569",
    )
    fig.text(
        0.075,
        0.028,
        "CPU subtraction is an estimate: pacing, scheduling, cache behavior and backpressure can differ between paired runs.",
        fontsize=10,
        color="#475569",
    )
    fig.subplots_adjust(
        left=0.075, right=0.985, top=0.868, bottom=0.215, wspace=0.27, hspace=0.41
    )
    for ext in ("png", "svg"):
        fig.savefig(root / f"call-rate-sweep.{ext}", facecolor=fig.get_facecolor())
    for threads in threads_list:
        rows = [r for r in summary if r["threads"] == threads]
        print(f"{threads} producers:")
        for row in rows:
            print(
                f"  requested={row['requested_calls_per_second'] / 1e6:.0f}M/s achieved={row['full_achieved_calls_per_second']['median'] / 1e6:.1f}M/s cpu={row['full_cpu_ns_per_call']['median']:.1f}ns/call extra={row['paired_difference_cpu_ns_per_call']['median']:.1f}ns/call RSS={row['full_peak_rss_bytes_including_preparation']['median'] / 2**30:.2f}GiB"
            )
