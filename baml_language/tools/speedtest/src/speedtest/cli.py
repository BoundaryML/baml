"""CLI entry point for speedtest: subcommand dispatch."""

import argparse
import sys


def build_parser():
    top = argparse.ArgumentParser(
        prog="speedtest",
        description="BAML benchmarks: every workload as BAML and as reference ports, checked against an expected "
                    "value, measured in fresh processes, reported as kpis.json and kpis.html.",
    )
    sub = top.add_subparsers(dest="command")

    run_p = sub.add_parser("run", help="Run workloads and write the report (default)")
    run_p.add_argument("--kpi", action="store_true",
                       help="Run the workloads kpis.toml lists, with every telemetry level")
    run_p.add_argument("--filter", action="append", default=[], help="Only workloads whose id or name contains this (repeatable)")
    run_p.add_argument("--rows", default=None, help="Comma-separated rows (default: vm-off,node,bun,python); rust-off packs with --rust")
    run_p.add_argument("--telemetry", action="store_true", help="Add every BAML row's backend at telemetry low, medium and high")
    run_p.add_argument("--trials", type=int, default=None, help="Trials per (workload, row) (default 5)")
    run_p.add_argument("--jobs", type=int, default=None, help="Override every workload's jobs per trial")
    run_p.add_argument("--seed", type=int, default=1, help="Seed for the trial order (default 1)")
    run_p.add_argument("--cpu", type=int, default=None, help="Pin every trial to this CPU (Linux)")
    run_p.add_argument("--build", action="store_true", help="cargo build --release baml-cli and the pack host first")
    run_p.add_argument("--baml", default=None, help="Path to baml-cli (default: target/release/baml-cli)")
    run_p.add_argument("--tag", default=None, metavar="NAME", help="Also save this run as baseline NAME")
    run_p.add_argument("--out", default=None, metavar="DIR", help="Also copy the report bundle here (e.g. a CI artifact dir)")
    run_p.add_argument("--profile", action="store_true", help="Capture samply CPU profiles of each BAML workload")
    run_p.add_argument("--profile-baml", default=None, help="Path to a profiling-profile baml-cli")
    run_p.add_argument("--results-dir", default=None, help="Where runs and baselines live (default: ~/.speedtest/)")

    cmp_p = sub.add_parser("compare", help="Compare two baselines (default: base vs new)")
    cmp_p.add_argument("a", nargs="?", default="base", help="First baseline name (default: base)")
    cmp_p.add_argument("b", nargs="?", default="new", help="Second baseline name (default: new)")
    cmp_p.add_argument("--filter", action="append", default=[], help="Only workloads matching substring")
    cmp_p.add_argument("--threshold", type=float, default=0, metavar="PCT", help="Hide changes below this %% (like critcmp -t)")
    cmp_p.add_argument("--results-dir", default=None)

    open_p = sub.add_parser("open", help="Open the browser UI for saved results")
    open_p.add_argument("--results-dir", default=None)

    rep_p = sub.add_parser("report", help="Re-render a saved run's kpis.json and kpis.html")
    rep_p.add_argument("run", help="Run directory, or a baseline like canary/latest")
    rep_p.add_argument("--results-dir", default=None)

    sub.add_parser("list", help="List workloads")

    bl_p = sub.add_parser("baselines", help="List saved baselines")
    bl_p.add_argument("--results-dir", default=None)
    return top


def main():
    parser = build_parser()
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
    elif args.command == "report":
        from speedtest.kpis import cmd_report
        cmd_report(args)
    elif args.command == "list":
        from speedtest.loader import cmd_list
        cmd_list()
    elif args.command == "baselines":
        from speedtest.storage import cmd_baselines
        cmd_baselines(args)
    else:
        parser.print_help()
        sys.exit(1)
