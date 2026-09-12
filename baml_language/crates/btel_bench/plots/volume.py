"""Preserved volume benchmark chart layout. Use plot.py as the entry point."""

import json
import statistics as stats

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt


def render(identity, runs, root):
    threads_list = sorted({r["threads"] for r in runs})
    calls_list = sorted({r["calls"] for r in runs})
    lookup = {
        (r["threads"], r["calls"], r["rep"], r["mode"]): r["result"] for r in runs
    }
    summary = []
    for n in threads_list:
        for calls in calls_list:
            pairs = [
                (
                    lookup[n, calls, rep, "encode-clock"],
                    lookup[n, calls, rep, "feeder-only"],
                )
                for rep in range(identity["repetitions"])
            ]
            entry = {
                "threads": n,
                "calls": calls,
                "equivalent_marker_bytes": pairs[0][0]["input_bytes"],
            }
            for key, label in [
                ("replay_cpu_seconds", "cpu"),
                ("replay_seconds", "elapsed"),
            ]:
                for idx, mode in [(0, "full"), (1, "baseline")]:
                    values = [p[idx][key] * 1e9 / calls for p in pairs]
                    entry[f"{mode}_{label}_ns_per_call"] = {
                        "median": stats.median(values),
                        "min": min(values),
                        "max": max(values),
                    }
                differences = [(a[key] - b[key]) * 1e9 / calls for a, b in pairs]
                entry[f"paired_difference_{label}_ns_per_call"] = {
                    "median": stats.median(differences),
                    "min": min(differences),
                    "max": max(differences),
                    "samples": differences,
                }
            entry["full_peak_rss_including_preparation_bytes"] = stats.median(
                a["peak_rss_bytes_including_preparation"] for a, b in pairs
            )
            entry["baseline_peak_rss_including_preparation_bytes"] = stats.median(
                b["peak_rss_bytes_including_preparation"] for a, b in pairs
            )
            summary.append(entry)
    (root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    plt.rcParams.update(
        {
            "font.family": "DejaVu Sans",
            "font.size": 10,
            "axes.spines.top": False,
            "axes.spines.right": False,
            "axes.titleweight": "bold",
        }
    )
    colors = {"full": "#2563eb", "baseline": "#64748b", "difference": "#0d9488"}
    fig, axes = plt.subplots(
        2, len(threads_list), figsize=(14, 8.6), dpi=180, squeeze=False
    )
    fig.patch.set_facecolor("#fafbfc")
    for col, n in enumerate(threads_list):
        rows = [r for r in summary if r["threads"] == n]
        x = [r["calls"] / 1e6 for r in rows]
        for mode in ("full", "baseline"):
            values = [r[f"{mode}_cpu_ns_per_call"] for r in rows]
            axes[0, col].plot(
                x,
                [v["median"] for v in values],
                "o-",
                color=colors[mode],
                label="Full replay" if mode == "full" else "Feeder baseline",
                linewidth=2,
            )
            axes[0, col].fill_between(
                x,
                [v["min"] for v in values],
                [v["max"] for v in values],
                color=colors[mode],
                alpha=0.13,
            )
        axes[0, col].set_title(
            f"{n} producer" + ("s" if n != 1 else ""), loc="left", pad=12
        )
        axes[0, col].legend(frameon=False, fontsize=9)
        values = [r["paired_difference_cpu_ns_per_call"] for r in rows]
        axes[1, col].plot(
            x,
            [v["median"] for v in values],
            "o-",
            color=colors["difference"],
            linewidth=2,
        )
        axes[1, col].fill_between(
            x,
            [v["min"] for v in values],
            [v["max"] for v in values],
            color=colors["difference"],
            alpha=0.14,
        )
        axes[1, col].set_title(
            "Estimated additional CPU cost", loc="left", fontsize=11, pad=12
        )
        for i, (px, v) in enumerate(zip(x, values)):
            axes[1, col].annotate(
                f"{v['median']:.1f}",
                (px, v["median"]),
                xytext=(
                    (-4, 9)
                    if i == 0
                    else (4, 10)
                    if i == 1
                    else (0, -15)
                    if i == 3
                    else (0, 9)
                ),
                textcoords="offset points",
                ha=("right" if i == 0 else "left" if i == 1 else "center"),
                color=colors["difference"],
                fontsize=9,
                fontweight="bold",
            )
        for row in (0, 1):
            ax = axes[row, col]
            ax.set_facecolor("#fafbfc")
            ax.grid(axis="y", alpha=0.18)
            ax.set_axisbelow(True)
            ax.set_xlim(0, max(calls_list) / 1e6 * 1.078125)
            ax.set_xticks(
                [1, 4, 8, 16]
                if max(calls_list) == 16_000_000
                else [c / 1e6 for c in calls_list]
            )
            ax.set_xlabel("Total function calls (millions)")
            ax.set_ylabel(
                "CPU ns / synthetic call"
                if row == 0
                else "Full − baseline CPU ns / call"
            )
            ax.axhline(0, color="#94a3b8", linewidth=0.8)
        axes[0, col].set_ylim(bottom=0)
    # Shared scales allow direct topology comparison; retain negatives if observed.
    for row in (0, 1):
        ymin = min(ax.get_ylim()[0] for ax in axes[row])
        ymax = max(ax.get_ylim()[1] for ax in axes[row])
        for ax in axes[row]:
            ax.set_ylim(ymin, ymax)
    fig.suptitle(
        "How much CPU belongs to the feeder?",
        x=0.07,
        y=0.98,
        ha="left",
        fontsize=20,
        fontweight="bold",
    )
    fig.text(
        0.07,
        0.934,
        f"Full = feeder + clock + encoding + rings + {identity.get('stage_description', 'discard drainer')}. Baseline = feeder with compiler barriers.",
        color="#475569",
        fontsize=10.5,
    )
    fig.text(
        0.07,
        0.101,
        "1 synthetic call = enter + exit. Baseline emits no bytes and has no drainer; full replay has one round-robin drainer."
        if identity.get("consumer_stage") != "copy-handoff"
        else "1 synthetic call = enter + exit. Full replay has a drainer plus a buffer-return worker; baseline has neither.",
        color="#475569",
        fontsize=10,
    )
    fig.text(
        0.07,
        0.070,
        f"Points: medians of {identity['repetitions']} runs/pairs. Bands: observed min–max. Differences are computed within each adjacent pair.",
        color="#475569",
        fontsize=10,
    )
    fig.text(
        0.07,
        0.039,
        "Subtraction estimates incremental CPU, not exact VM overhead: barriers, caches, scheduling and backpressure differ.",
        color="#475569",
        fontsize=10,
    )
    fig.subplots_adjust(
        left=0.07, right=0.985, top=0.86, bottom=0.20, wspace=0.31, hspace=0.44
    )
    for ext in ("png", "svg"):
        fig.savefig(root / f"cpu-baseline.{ext}", facecolor=fig.get_facecolor())
    fig2, axs = plt.subplots(
        1, len(threads_list), figsize=(14, 5), dpi=180, squeeze=False
    )
    axs = axs[0]
    for ax, n in zip(axs, threads_list):
        rows = [r for r in summary if r["threads"] == n]
        x = [r["calls"] / 1e6 for r in rows]
        for mode in ("full", "baseline"):
            ms = lambda r, k, mode=mode: (
                r[f"{mode}_elapsed_ns_per_call"][k] * r["calls"] / 1e6
            )
            ax.plot(
                x,
                [ms(r, "median") for r in rows],
                "o-",
                color=colors[mode],
                label="Full replay" if mode == "full" else "Feeder baseline",
            )
            ax.fill_between(
                x,
                [ms(r, "min") for r in rows],
                [ms(r, "max") for r in rows],
                color=colors[mode],
                alpha=0.13,
            )
        ax.set_title(f"{n} producer" + ("s" if n != 1 else ""), loc="left")
        ax.set_xlabel("Total function calls (millions)")
        ax.set_ylabel("Elapsed time (ms)")
        ax.set_ylim(bottom=0)
        ax.grid(axis="y", alpha=0.18)
        ax.legend(frameon=False, fontsize=9)
    fig2.suptitle(
        "Time to finish: full replay vs feeder baseline",
        x=0.07,
        y=0.98,
        ha="left",
        fontsize=18,
        fontweight="bold",
    )
    fig2.text(
        0.07,
        0.045,
        f"Medians and observed min–max of {identity['repetitions']} runs. Elapsed measures completion time; subtracting it does not isolate stage latency.",
        fontsize=10,
        color="#475569",
    )
    fig2.subplots_adjust(left=0.07, right=0.985, top=0.81, bottom=0.20, wspace=0.32)
    for ext in ("png", "svg"):
        fig2.savefig(root / f"elapsed-baseline.{ext}", facecolor="#fafbfc")
    for r in summary:
        if r["calls"] == 16_000_000:
            print(json.dumps(r))
