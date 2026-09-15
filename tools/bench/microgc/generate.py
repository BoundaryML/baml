#!/usr/bin/env python3

import argparse
from pathlib import Path

from workloads import WORKLOADS, write_workload


HERE = Path(__file__).resolve().parent


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate the BAML micro-GC workload projects")
    parser.add_argument("workloads", nargs="*", metavar="WORKLOAD")
    parser.add_argument("--output", type=Path, default=HERE / ".build/apps")
    args = parser.parse_args()
    unknown = sorted(set(args.workloads) - set(WORKLOADS))
    if unknown:
        parser.error(f"unknown workload(s): {', '.join(unknown)}; choose from {', '.join(sorted(WORKLOADS))}")
    return args


def main() -> None:
    args = parse_args()
    names = args.workloads or sorted(WORKLOADS)
    for name in names:
        app = write_workload(WORKLOADS[name], args.output)
        print(app)


if __name__ == "__main__":
    main()
