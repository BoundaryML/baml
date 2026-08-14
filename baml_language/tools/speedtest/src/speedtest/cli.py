"""CLI entry point for speedtest — subcommand dispatch."""

import argparse
import sys


def build_parser():
    top = argparse.ArgumentParser(
        prog="speedtest",
        description="Benchmark runner for BAML — timing, baselines, comparison, profiling",
    )
    sub = top.add_subparsers(dest="command")

    # ── run (default when no subcommand) ─────────────────────────────────
    run_p = sub.add_parser("run", help="Run benchmarks (default)")
    run_p.add_argument("--build", action="store_true", help="cargo build before benchmarking")
    run_p.add_argument("--filter", action="append", default=[], help="Only workloads matching substring (repeatable)")
    run_p.add_argument("--only-baml", action="store_true", help="Skip python/node/bun")
    run_p.add_argument("--runs", type=int, default=None, help="Fixed number of runs per workload (overrides adaptive timing)")
    run_p.add_argument("--measurement-time", type=float, default=5.0, metavar="SECS", help="Target seconds per workload for adaptive timing (default: 5s)")
    run_p.add_argument("--baml", default=None, help="Path to baml-cli")
    run_p.add_argument("--tag", default=None, metavar="NAME", help="Tag this run (like a git tag for benchmarks)")
    run_p.add_argument("--profile", action="store_true", help="Capture samply CPU profiles")
    run_p.add_argument("--profile-baml", default=None, help="Path to profiling-mode baml-cli")
    run_p.add_argument("--results-dir", default=None, help="Where to save results (default: ~/.speedtest/)")

    # ── compare ──────────────────────────────────────────────────────────
    cmp_p = sub.add_parser("compare", help="Compare two baselines (default: base vs new)")
    cmp_p.add_argument("a", nargs="?", default="base", help="First baseline name (default: base)")
    cmp_p.add_argument("b", nargs="?", default="new", help="Second baseline name (default: new)")
    cmp_p.add_argument("--filter", action="append", default=[], help="Only workloads matching substring")
    cmp_p.add_argument("--threshold", type=float, default=0, metavar="PCT", help="Hide changes below this %% (like critcmp -t)")
    cmp_p.add_argument("--results-dir", default=None)

    # ── open ─────────────────────────────────────────────────────────────
    open_p = sub.add_parser("open", help="Open browser UI for results")
    open_p.add_argument("--results-dir", default=None)

    # ── list ─────────────────────────────────────────────────────────────
    sub.add_parser("list", help="List workload names")

    # ── baselines ────────────────────────────────────────────────────────
    bl_p = sub.add_parser("baselines", help="List saved baselines")
    bl_p.add_argument("--results-dir", default=None)

    return top


def main():
    parser = build_parser()

    # Default to "run" when no subcommand given but flags are present
    # (e.g. `speedtest --filter string --build`)
    args = parser.parse_args()
    if args.command is None:
        args = parser.parse_args(["run"] + sys.argv[1:])

    if args.command == "run":
        from speedtest.runner import cmd_run
        cmd_run(args)
    elif args.command == "compare":
        from speedtest.compare import cmd_compare
        cmd_compare(args)
    elif args.command == "open":
        from speedtest.ui import cmd_open
        cmd_open(args)
    elif args.command == "list":
        from speedtest.loader import cmd_list
        cmd_list()
    elif args.command == "baselines":
        from speedtest.storage import cmd_baselines
        cmd_baselines(args)
    else:
        parser.print_help()
        sys.exit(1)
