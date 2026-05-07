#!/usr/bin/env python3
"""Cross-language Python wrapper: import the workload at the given path
and invoke its main(). hyperfine times the whole subprocess from spawn
to exit, so this matches what cross_lang_baml and the Go CLI do.

Usage: python3 run.py <path-to-workload.py>
"""

import importlib.util
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: run.py <path-to-workload.py>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    spec = importlib.util.spec_from_file_location("workload", path)
    if spec is None or spec.loader is None:
        print(f"failed to load {path}", file=sys.stderr)
        return 2
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    if not hasattr(module, "main"):
        print(f"{path} has no main()", file=sys.stderr)
        return 2
    result = module.main()
    # Touch result so it can't be DCE'd by an aggressive interpreter.
    _ = repr(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
