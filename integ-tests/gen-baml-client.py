#!/usr/bin/env python3
"""
This script verifies that our code generation is stable by re‐generating
the baml_client folders several times and ensuring that the output is identical.
Both the Python and TypeScript versions of baml-cli generate the clients.
This script runs each generator command multiple times and
checks that the resulting outputs remain stable. In cases of instability, it
summarizes the results as "in N runs, generated M versions" and shows diffs of
the unstable parts.
"""

import argparse
import hashlib
import subprocess
import sys
import threading
import itertools
import time
from pathlib import Path
from difflib import unified_diff

# ANSI escape sequences for colored output
RESET = "\033[0m"
BOLD = "\033[1m"
RED = "\033[91m"
GREEN = "\033[92m"
YELLOW = "\033[93m"
CYAN = "\033[96m"

FILE_PATH = Path(__file__).parent.absolute()
BAML_SRC_PATH = FILE_PATH / "baml_src"
target_clients = {
    "python": FILE_PATH / "python" / "baml_client",
    "typescript": FILE_PATH / "typescript" / "baml_client",
    "ruby": FILE_PATH / "ruby" / "baml_client",
    "openapi": FILE_PATH / "openapi" / "baml_client",
}


class Spinner:
    """
    A simple spinner to show progress while waiting for a command to finish.
    """
    def __init__(self, delay=0.1):
        self.delay = delay
        self.spinner = itertools.cycle(["-", "\\", "|", "/"])
        self.running = False
        self.thread = None

    def start(self):
        self.running = True
        self.thread = threading.Thread(target=self.spin)
        self.thread.start()

    def spin(self):
        # The spinner prints on the same line using backspaces.
        while self.running:
            sys.stdout.write(next(self.spinner))
            sys.stdout.flush()
            time.sleep(self.delay)
            sys.stdout.write("\b")
            sys.stdout.flush()

    def stop(self):
        self.running = False
        if self.thread is not None:
            self.thread.join()


def run_command(cmd: list, cwd: Path, description: str, verbose: bool = False) -> None:
    """
    Runs a subprocess command with live output streaming.
    The spinner and description are printed on the same line.
    When the step is done, the elapsed time is printed.
    """
    start_time = time.time()
    # Print description without a newline.
    sys.stdout.write(f"{BOLD}{GREEN}{description}... {RESET}")
    sys.stdout.flush()

    spinner = None
    if not verbose:
        spinner = Spinner()
        spinner.start()

    process = subprocess.Popen(
        cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    output_lines = []
    while True:
        line = process.stdout.readline()
        if line == "" and process.poll() is not None:
            break
        if line:
            output_lines.append(line)
            if verbose:
                print(line, end="")  # line already has newline

    if spinner:
        spinner.stop()
    elapsed = time.time() - start_time
    # Overwrite spinner line with elapsed time message.
    print(f"done in {elapsed:.2f}s")

    retcode = process.poll()
    if retcode:
        print(f"{BOLD}{RED}Command failed: {' '.join(cmd)}{RESET}")
        print("".join(output_lines))
        raise subprocess.CalledProcessError(retcode, cmd, output="".join(output_lines))


# ---------------------------
# Python CLI: Build and Generate
# ---------------------------
def run_python_build(verbose: bool = False) -> None:
    """
    Build step for the Python baml-cli.
    (This only needs to be done once.)
    """
    python_cwd = FILE_PATH / "python"
    run_command(
        [
            "uv",
            "run",
            "maturin",
            "develop",
            "--uv",
            "--manifest-path",
            "../../engine/language_client_python/Cargo.toml",
        ],
        cwd=python_cwd,
        description="[Python CLI] Building Python client with maturin",
        verbose=verbose,
    )


def run_python_generate(verbose: bool = False) -> None:
    """
    Generate step for the Python baml-cli.
    (This will be run on each iteration.)
    """
    python_cwd = FILE_PATH / "python"
    run_command(
        ["uv", "run", "baml-cli", "generate", "--from", str(BAML_SRC_PATH)],
        cwd=python_cwd,
        description="[Python CLI] Generating baml_client for all targets",
        verbose=verbose,
    )


# ---------------------------
# TypeScript CLI: Build and Generate
# ---------------------------
def run_ts_build(verbose: bool = False) -> None:
    """
    Build step for the TypeScript baml-cli.
    (This only needs to be done once.)
    """
    ts_cwd = FILE_PATH / "typescript"
    run_command(
        ["pnpm", "build:debug"],
        cwd=ts_cwd,
        description="[TypeScript CLI] Building TypeScript client (debug)",
        verbose=verbose,
    )


def run_ts_generate(verbose: bool = False) -> None:
    """
    Generate step for the TypeScript baml-cli.
    (This will be run on each iteration.)
    """
    ts_cwd = FILE_PATH / "typescript"
    run_command(
        ["pnpm", "generate"],
        cwd=ts_cwd,
        description="[TypeScript CLI] Generating baml_client for all targets",
        verbose=verbose,
    )


def get_baml_client_hash(baml_client_path: Path) -> str:
    """
    Computes a stable SHA-256 hash of all files in the given folder.
    """
    files = sorted(f for f in baml_client_path.glob("**/*") if f.is_file())
    file_contents = [f.read_bytes() for f in files]
    return hashlib.sha256(b"".join(file_contents)).hexdigest()


def capture_snapshot(folder: Path) -> dict:
    """
    Captures a snapshot of the folder's file contents.
    Returns a dictionary mapping each file's relative path to its text content.
    """
    snapshot = {}
    for file in sorted(folder.glob("**/*")):
        if file.is_file():
            try:
                content = file.read_text(encoding="utf-8")
            except Exception:
                content = file.read_bytes().decode("utf-8", errors="replace")
            snapshot[str(file.relative_to(folder))] = content
    return snapshot


def diff_snapshots(snapshot1: dict, snapshot2: dict) -> str:
    """
    Computes a unified diff between two snapshots.
    Returns the diff output as a string.
    """
    diff_lines = []
    all_files = sorted(set(snapshot1.keys()) | set(snapshot2.keys()))
    for file in all_files:
        content1 = snapshot1.get(file, "").splitlines(keepends=True)
        content2 = snapshot2.get(file, "").splitlines(keepends=True)
        if content1 != content2:
            diff = list(unified_diff(content1, content2,
                                     fromfile=f"{file} (v1)",
                                     tofile=f"{file} (v2)"))
            if diff:
                diff_lines.append(f"--- Diff for {file} ---\n" + "".join(diff))
    return "\n".join(diff_lines)


def print_table(table_rows):
    """
    Prints a summary table given the rows.
    Each row is a tuple: (Generator, Target, Iteration, Hash)
    """
    col1_width = max(len("Generator"), max((len(row[0]) for row in table_rows), default=0))
    col2_width = max(len("Target"), max((len(row[1]) for row in table_rows), default=0))
    col3_width = max(len("Iteration"), max((len(str(row[2])) for row in table_rows), default=0))
    col4_width = max(len("Hash"), max((len(row[3]) for row in table_rows), default=0))

    sep_line = (
        "+" + "-" * (col1_width + 2) +
        "+" + "-" * (col2_width + 2) +
        "+" + "-" * (col3_width + 2) +
        "+" + "-" * (col4_width + 2) + "+"
    )

    print(sep_line)
    print("| {0:<{w1}} | {1:<{w2}} | {2:<{w3}} | {3:<{w4}} |".format(
        "Generator", "Target", "Iteration", "Hash",
        w1=col1_width, w2=col2_width, w3=col3_width, w4=col4_width))
    print(sep_line)
    for row in table_rows:
        print("| {0:<{w1}} | {1:<{w2}} | {2:<{w3}} | {3:<{w4}} |".format(
            row[0], row[1], row[2], row[3],
            w1=col1_width, w2=col2_width, w3=col3_width, w4=col4_width))
    print(sep_line)


def main():
    parser = argparse.ArgumentParser(
        description="Run code generation multiple times to ensure stability."
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=3,
        help="Number of iterations to run each codegen (default: 3)"
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Show detailed output from codegen commands"
    )
    parser.add_argument(
        "--only",
        type=str,
        default="",
        help=("Comma-separated list of generator commands to run. "
              "Options: python, typescript (default: both)")
    )
    args = parser.parse_args()

    # Define generator commands.
    # We separate build and generate steps.
    generator_build = {
        "python": run_python_build,
        "typescript": run_ts_build,
    }
    generator_generate = {
        "python": run_python_generate,
        "typescript": run_ts_generate,
    }

    available_generators = set(generator_build.keys())
    if args.only:
        selected = {lang.strip().lower() for lang in args.only.split(",")}
        generators_to_run = available_generators.intersection(selected)
        if not generators_to_run:
            print(f"{BOLD}{RED}No valid generators selected. Available options: {', '.join(available_generators)}{RESET}")
            sys.exit(1)
    else:
        generators_to_run = available_generators

    # Check all targets defined in target_clients.
    targets_to_check = list(target_clients.keys())

    # Dictionaries to store per-run results:
    # results[generator][target] = list of hash strings (one per iteration)
    # snapshots[generator][target] = list of snapshots (each a dict mapping relative file path to content)
    results = {gen: {target: [] for target in targets_to_check} for gen in generators_to_run}
    snapshots = {gen: {target: [] for target in targets_to_check} for gen in generators_to_run}
    table_rows = []  # List of tuples: (Generator, Target, Iteration, Hash)
    overall_fail = 0

    print(f"\n{BOLD}{CYAN}Starting code generation stability check{RESET}\n")

    # For each generator command...
    for gen in sorted(generators_to_run):
        print(f"{BOLD}{YELLOW}Testing '{gen}' generator command{RESET}")

        # Run the build step once
        build_func = generator_build[gen]
        print(f"{CYAN}Running build step for '{gen}' generator...{RESET}")
        try:
            build_func(args.verbose)
        except subprocess.CalledProcessError as e:
            print(f"{BOLD}{RED}Build error during '{gen}' codegen.{RESET}")
            sys.exit(e.returncode)

        generate_func = generator_generate[gen]
        for iteration in range(1, args.iterations + 1):
            print(f"  {CYAN}Iteration {iteration}/{args.iterations}...{RESET}")
            try:
                generate_func(args.verbose)
            except subprocess.CalledProcessError as e:
                print(f"{BOLD}{RED}Error during '{gen}' codegen on iteration {iteration}.{RESET}")
                sys.exit(e.returncode)
            # For each target, record the hash and capture a snapshot.
            for target in targets_to_check:
                target_path = target_clients[target]
                h = get_baml_client_hash(target_path)
                results[gen][target].append(h)
                snap = capture_snapshot(target_path)
                snapshots[gen][target].append(snap)
                table_rows.append((gen, target, str(iteration), h))

    print("\nDetailed Run Results:\n")
    print_table(table_rows)
    print("")

    # Now summarize stability per generator/target.
    for gen in sorted(generators_to_run):
        for target in targets_to_check:
            runs = args.iterations
            unique_versions = {}
            for idx, h in enumerate(results[gen][target]):
                if h not in unique_versions:
                    unique_versions[h] = snapshots[gen][target][idx]
            version_count = len(unique_versions)
            if version_count == 1:
                print(f"{BOLD}{GREEN}✅ {target} codegen is stable for generator '{gen}': in {runs} runs, generated 1 version.{RESET}\n")
            else:
                print(f"{BOLD}{RED}❌ {target} codegen is unstable for generator '{gen}': in {runs} runs, generated {version_count} versions:{RESET}")
                for h in unique_versions:
                    print(f"  Version hash: {h}")
                overall_fail += 1
                # Show diffs: use the first unique version as baseline.
                baseline_hash, baseline_snapshot = next(iter(unique_versions.items()))
                for h, snap in unique_versions.items():
                    if h == baseline_hash:
                        continue
                    diff_text = diff_snapshots(baseline_snapshot, snap)
                    if diff_text:
                        print(f"{YELLOW}Diff between baseline version ({baseline_hash}) and version ({h}):{RESET}")
                        print(diff_text)
                print("")

    if overall_fail > 0:
        print(f"{BOLD}{RED}Failed stability checks for {overall_fail} generator/target combination(s).{RESET}")
        sys.exit(1)
    else:
        print(f"{BOLD}{GREEN}All codegen stability checks passed!{RESET}")


if __name__ == "__main__":
    main()
