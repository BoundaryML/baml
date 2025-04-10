#!/usr/bin/env python3
"""
Cross-Platform Build Script
===========================
A beautiful, feature-rich build script for compiling your project
across multiple target platforms using tqdm for progress.
"""

import argparse
from io import TextIOWrapper
import os
import platform
import shutil
import subprocess
import threading
import queue
import sys
import time
from dataclasses import dataclass
from enum import Enum
from typing import List, Optional, Dict

# Try importing tqdm, provide guidance if missing
try:
    from tqdm import tqdm
except ImportError:
    print("Error: 'tqdm' package not found. Please install it:")
    print("  pip install tqdm")
    sys.exit(1)


# ANSI Color Codes for beautiful terminal output
class Color:
    RESET = "\033[0m"
    BOLD = "\033[1m"
    UNDERLINE = "\033[4m"
    BLACK = "\033[30m"
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    MAGENTA = "\033[35m"
    CYAN = "\033[36m"
    WHITE = "\033[37m"
    BRIGHT_BLACK = "\033[90m"
    BRIGHT_RED = "\033[91m"
    BRIGHT_GREEN = "\033[92m"
    BRIGHT_YELLOW = "\033[93m"
    BRIGHT_BLUE = "\033[94m"
    BRIGHT_MAGENTA = "\033[95m"
    BRIGHT_CYAN = "\033[96m"
    BRIGHT_WHITE = "\033[97m"
    BG_BLACK = "\033[40m"
    BG_RED = "\033[41m"
    BG_GREEN = "\033[42m"
    BG_YELLOW = "\033[43m"
    BG_BLUE = "\033[44m"
    BG_MAGENTA = "\033[45m"
    BG_CYAN = "\033[46m"
    BG_WHITE = "\033[47m"
    DIM = "\033[2m"

    @staticmethod
    def disable_if_not_supported():
        """Disable colors if terminal doesn't support them or if NO_COLOR is set"""
        if (
            not sys.stdout.isatty()
            or os.environ.get("NO_COLOR")
            or (platform.system() == "Windows" and not os.environ.get("TERM"))
        ):
            for attr in dir(Color):
                if not attr.startswith("__") and isinstance(getattr(Color, attr), str):
                    setattr(Color, attr, "")


class TargetType(Enum):
    LINUX = "linux"
    MACOS = "darwin"
    WINDOWS = "windows"
    MUSL = "musl"


@dataclass
class Target:
    """Target platform configuration"""

    triple: str
    description: str
    type: TargetType

    @property
    def display_name(self) -> str:
        """Returns a nicely formatted display name"""
        return f"{self.description} ({self.triple})"

    @property
    def color(self) -> str:
        """Returns an appropriate color based on target type"""
        if self.type == TargetType.LINUX:
            return Color.YELLOW
        elif self.type == TargetType.MACOS:
            return Color.BRIGHT_CYAN
        elif self.type == TargetType.WINDOWS:
            return Color.BRIGHT_BLUE
        elif self.type == TargetType.MUSL:
            return Color.BRIGHT_GREEN
        return Color.WHITE


# Define available build targets
TARGETS = [
    Target("aarch64-unknown-linux-gnu", "Linux ARM64", TargetType.LINUX),
    Target("x86_64-unknown-linux-gnu", "Linux x86_64", TargetType.LINUX),
    Target("x86_64-unknown-linux-musl", "Linux MUSL x86_64", TargetType.MUSL),
    Target("x86_64-pc-windows-msvc", "Windows x86_64", TargetType.WINDOWS),
    Target("aarch64-pc-windows-msvc", "Windows ARM64", TargetType.WINDOWS),
    Target("x86_64-apple-darwin", "macOS x86_64", TargetType.MACOS),
    Target("aarch64-apple-darwin", "macOS ARM64", TargetType.MACOS),
]

# Default enabled targets
DEFAULT_ENABLED_TARGETS = [
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    # "x86_64-pc-windows-msvc",
    # "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
]


class BuildEnvironment:
    """Information about the current build environment"""

    def __init__(self):
        self.system = platform.system()
        self.machine = platform.machine()
        self.release = platform.release()
        self.version = platform.version()
        self.processor = platform.processor() or self.machine

        # Normalize architecture names
        if self.machine == "arm64":
            self.machine = "aarch64"

    @property
    def display_info(self) -> str:
        """Returns formatted environment information"""
        return (
            f"{Color.BOLD}System:{Color.RESET} {self.system} {self.release}\n"
            f"{Color.BOLD}Architecture:{Color.RESET} {self.machine} ({self.processor})\n"
            f"{Color.BOLD}Version:{Color.RESET} {self.version}"
        )

    @property
    def is_windows(self) -> bool:
        return self.system == "Windows"

    @property
    def is_mac(self) -> bool:
        return self.system == "Darwin"

    @property
    def is_linux(self) -> bool:
        return self.system == "Linux"

    @property
    def is_musl(self) -> bool:
        if not self.is_linux:
            return False
        # Check if the system is using musl
        try:
            # Check linked libc using ldd on a common binary like /bin/sh or python itself
            executable = sys.executable or "/bin/sh"
            ldd_output = subprocess.run(
                ["ldd", executable],
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,  # Capture both to check for musl messages
                text=True,
                check=True,
            ).stdout
            return "musl" in ldd_output.lower()
        except (FileNotFoundError, subprocess.CalledProcessError, Exception):
            # Fallback: Check for known musl paths or indicators if ldd fails/not present
            if os.path.exists("/lib/ld-musl-x86_64.so.1") or os.path.exists(
                "/lib/ld-musl-aarch64.so.1"
            ):
                return True
            return False  # Assume glibc otherwise

    @property
    def is_x86_64(self) -> bool:
        return self.machine == "x86_64"

    @property
    def is_aarch64(self) -> bool:
        return self.machine == "aarch64"


# --- Removed ProgressSpinner Class ---


class Builder:
    """Main build coordinator"""

    def __init__(self, options: "BuildOptions"):
        self.options = options
        self.env = BuildEnvironment()
        self.results: Dict[str, bool] = {}
        self._output_lock = threading.Lock()  # Lock for thread-safe tqdm.write

    def print_header(self):
        """Print a beautiful header for the build process"""
        header = f"""
{Color.BRIGHT_MAGENTA}╔{'═' * 60}╗
║{Color.BRIGHT_WHITE} ♦ Cross-Platform Build Tool {' ' * 34}║
{Color.BRIGHT_MAGENTA}╠{'═' * 60}╣{Color.RESET}
"""
        print(header)
        print(f"{Color.BOLD}Build Environment:{Color.RESET}")
        print(self.env.display_info)
        print(f"\n{Color.BOLD}Build Configuration:{Color.RESET}")
        print(f"  {Color.BOLD}Verbose:{Color.RESET} {self.options.verbose}")
        print(f"  {Color.BOLD}Debug Mode:{Color.RESET} {not self.options.release}")

        selected_targets = self.options.targets
        print(f"\n{Color.BOLD}Selected Targets:{Color.RESET}")
        targets_to_print = [t for t in TARGETS if t.triple in selected_targets]
        if not targets_to_print:
            print(f"  {Color.YELLOW}No targets selected.{Color.RESET}")
        else:
            for target in targets_to_print:
                print(f"  {target.color}● {target.display_name}{Color.RESET}")

        print(f"\n{Color.BRIGHT_MAGENTA}{'─' * 62}{Color.RESET}\n")

    def _write_output(self, message: str, indent: int = 0):
        """Safely write output using tqdm.write"""
        prefix = "  " * indent
        # Use lock to prevent interleaved output from multiple threads
        with self._output_lock:
            tqdm.write(f"{prefix}{message}", file=sys.stdout)

    def build_target(self, target: Target) -> bool:
        """Build for a specific target, writing output via tqdm.write"""
        start_time = time.time()
        self._write_output(
            f"🚀 Starting build for {target.color}{target.display_name}{Color.RESET}"
        )

        # Prepare environment variables
        env_vars = os.environ.copy()
        # if interactive, use always color
        if os.isatty(sys.stdout.fileno()):
            env_vars["CARGO_TERM_COLOR"] = "always"
        # Special handling for cross-compiling from Mac to Linux
        # Ensure the target architecture matches the platform request if possible
        if self.env.is_mac and target.type in (TargetType.LINUX, TargetType.MUSL):
            arch = "arm64" if "aarch64" in target.triple else "amd64"
            env_vars["CROSS_CONTAINER_OPTS"] = f"--platform linux/{arch}"
            # Add Docker Engine check maybe?

        # Add any user-specified environment variables
        for key, value in self.options.env_vars.items():
            env_vars[key] = value

        # Prepare command
        cmd = ["cross", "build"]
        if self.options.release:
            cmd.append("--release")
        if self.options.verbose:
            cmd.append("--verbose")  # Let cross handle verbose output directly
        cmd.extend(["--target", target.triple])

        # Add any extra build flags
        cmd.extend(self.options.extra_flags)

        # Log the command being run if verbose enough (or for debugging)
        # self._write_output(f"   {Color.BRIGHT_BLACK}Running: {' '.join(cmd)}{Color.RESET}", indent=1)

        success = False
        captured_output: list[str] = []
        process: Optional[subprocess.Popen[str]] = None
        try:
            # Use Popen to stream output if not verbose, otherwise let it print directly
            should_capture = not self.options.verbose
            process = subprocess.Popen(
                cmd,
                env=env_vars,
                stdout=subprocess.PIPE if should_capture else None,
                stderr=subprocess.PIPE if should_capture else None,
                text=True,
                bufsize=1,  # Line buffered
                universal_newlines=True,
            )

            # --- Output Reading Threads (only if capturing) ---
            output_queue: queue.Queue[Optional[str]] = queue.Queue()
            stdout_thread = None
            stderr_thread = None

            def read_stream(stream: Optional[TextIOWrapper], stream_name: str):
                if stream:
                    try:
                        for line in iter(stream.readline, ""):
                            output_queue.put(line)
                    except Exception as e:
                        output_queue.put(f"Error reading {stream_name}: {e}\n")
                    finally:
                        stream.close()  # Ensure stream is closed
                output_queue.put(None)  # Sentinel value to indicate stream end

            if should_capture and process.stdout and process.stderr:
                stdout_thread = threading.Thread(
                    target=read_stream, args=(process.stdout, "stdout"), daemon=True
                )
                stderr_thread = threading.Thread(
                    target=read_stream, args=(process.stderr, "stderr"), daemon=True
                )
                stdout_thread.start()
                stderr_thread.start()

                streams_ended = 0
                while streams_ended < 2:
                    try:
                        # Timeout helps prevent hanging if threads die unexpectedly
                        line = output_queue.get(timeout=60)
                        if line is None:
                            streams_ended += 1
                        else:
                            line_stripped = line.rstrip()
                            if line_stripped:  # Avoid printing empty lines
                                captured_output.append(
                                    line_stripped
                                )  # Store for potential error reporting
                                self._write_output(
                                    f"{Color.DIM}{line_stripped}{Color.RESET}",
                                    indent=1,
                                )  # Indent build output
                            output_queue.task_done()
                    except queue.Empty:
                        # Check if process died unexpectedly
                        if process.poll() is not None:
                            self._write_output(
                                f"{Color.YELLOW}Warning: Output stream ended prematurely.{Color.RESET}",
                                indent=1,
                            )
                            break  # Exit loop if process is dead
                        # Otherwise, continue waiting for output
                        continue

                # Wait briefly for threads to finish cleanly
                if stdout_thread:
                    stdout_thread.join(0.5)
                if stderr_thread:
                    stderr_thread.join(0.5)

            # Wait for the process to complete and get return code
            return_code = process.wait()
            success = return_code == 0

        except FileNotFoundError:
            self._write_output(
                f"{Color.RED}Error: 'cross' command not found. Is cross-rs installed and in PATH?{Color.RESET}",
                indent=1,
            )
            self._write_output(
                f"{Color.YELLOW}See: https://github.com/cross-rs/cross{Color.RESET}",
                indent=1,
            )
            success = False
        except Exception as e:
            self._write_output(
                f"{Color.RED}Build execution error: {e}{Color.RESET}", indent=1
            )
            success = False
        finally:
            # Ensure process is terminated if something went wrong
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=5)  # Wait a bit for termination
                if process.poll() is None:  # Force kill if still running
                    process.kill()

        # --- Log Result ---
        elapsed = time.time() - start_time
        self.results[target.triple] = success

        if success:
            self._write_output(
                f"{Color.GREEN}✓ Success{Color.RESET} building {target.color}{target.display_name}{Color.RESET} ({elapsed:.2f}s)"
            )
        else:
            self._write_output(
                f"{Color.RED}✗ Failed{Color.RESET} building {target.color}{target.display_name}{Color.RESET} ({elapsed:.2f}s)"
            )
            # Optionally print captured output again on failure if it wasn't verbose
            # if not self.options.verbose and captured_output:
            #     self._write_output(f"{Color.BRIGHT_BLACK}--- Captured Output ---{Color.RESET}", indent=1)
            #     # Print last N lines
            #     for line in captured_output[-20:]:
            #         self._write_output(line, indent=2)
            #     self._write_output(f"{Color.BRIGHT_BLACK}---------------------{Color.RESET}", indent=1)

        return success

    def build_all(self):
        """Build all selected targets using tqdm for overall progress."""
        self.print_header()

        targets_to_build = [t for t in TARGETS if t.triple in self.options.targets]
        if not targets_to_build:
            print(
                f"{Color.YELLOW}No targets specified for building. Exiting.{Color.RESET}"
            )
            return True  # No work to do is considered success

        start_time = time.time()
        build_path = "release" if self.options.release else "debug"
        build_dir = os.path.abspath(
            os.path.join(os.path.dirname(__file__), "..")
        )  # Assuming script is in a subdir
        target_dir = os.path.join(build_dir, "target")
        output_lib_dir = os.path.join(os.path.dirname(__file__), "lib")

        # Ensure output directory exists
        os.makedirs(output_lib_dir, exist_ok=True)

        overall_success = True

        # --- TQDM Progress Bar ---
        # Disable bar if verbose, as direct output might interfere
        # Use leave=True to keep the final bar state visible
        with tqdm(
            total=len(targets_to_build),
            desc="Overall Build Progress",
            unit="target",
            ncols=100,  # Adjust width as needed
            bar_format="{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}{postfix}]",
            disable=self.options.verbose,  # Disable bar in verbose mode
            leave=True,
        ) as pbar:

            for target in targets_to_build:
                pbar.set_postfix_str(f"Building {target.triple}...", refresh=True)
                target_success = self.build_target(target)
                self.results[target.triple] = target_success
                pbar.update(1)  # Increment progress bar

                if target_success:
                    # Copy the built artifact
                    try:
                        match target.type:
                            case TargetType.MACOS:
                                extension = "dylib"
                            case TargetType.LINUX | TargetType.MUSL:
                                extension = "so"
                            case TargetType.WINDOWS:
                                extension = "dll"

                        source_file = os.path.join(
                            target_dir,
                            target.triple,
                            build_path,
                            f"libbaml_cffi.{extension}",
                        )
                        dest_file = os.path.join(
                            output_lib_dir, f"libbaml_cffi-{target.triple}.{extension}"
                        )

                        if os.path.exists(source_file):
                            shutil.copy2(
                                source_file, dest_file
                            )  # copy2 preserves metadata
                            self._write_output(
                                f"{Color.GREEN}✓  Copied{Color.RESET} {os.path.basename(source_file)}{Color.BRIGHT_BLACK} to {Color.RESET}{os.path.relpath(dest_file, build_dir)}{Color.RESET}"
                            )
                        else:
                            self._write_output(
                                f"  {Color.YELLOW}Warning: Built artifact not found at {os.path.relpath(source_file, build_dir)}{Color.RESET}"
                            )
                            # Mark target as failed if artifact missing, even if build command succeeded
                            self.results[target.triple] = False
                            target_success = False

                    except Exception as e:
                        self._write_output(
                            f"  {Color.RED}Error copying artifact for {target.triple}: {e}{Color.RESET}"
                        )
                        self.results[target.triple] = False
                        target_success = False

                if not target_success:
                    overall_success = False
                    pbar.set_postfix_str(f"{target.triple} FAILED", refresh=True)
                    if self.options.fail_fast:
                        self._write_output(
                            f"\n{Color.RED}Build failed for {target.triple}. Stopping due to --fail-fast.{Color.RESET}"
                        )
                        # Fill remaining progress
                        pbar.update(len(targets_to_build) - pbar.n)
                        break  # Exit the loop
                else:
                    pbar.set_postfix_str(f"{target.triple} OK", refresh=True)

        # --- Final Summary ---
        elapsed = time.time() - start_time

        # Ensure final postfix is cleared or shows overall status
        if not self.options.verbose:
            pbar.set_postfix_str("Completed", refresh=True)

        success_count = sum(1 for success in self.results.values() if success)
        total_count = len(self.results)  # Use results dict size

        print(f"\n{Color.BRIGHT_MAGENTA}{'─' * 62}{Color.RESET}")

        if success_count == total_count and overall_success:
            result_color = Color.GREEN
            result_text = "SUCCESS"
        elif success_count == 0 or not overall_success:
            result_color = Color.RED
            result_text = "FAILED"
        else:
            result_color = Color.YELLOW
            result_text = "PARTIAL SUCCESS"  # Some targets succeeded, some failed

        print(
            f"\n{Color.BOLD}Build Summary:{Color.RESET} {result_color}{result_text}{Color.RESET}"
        )
        print(f"  {Color.BOLD}Total Time:{Color.RESET} {elapsed:.2f} seconds")
        print(
            f"  {Color.BOLD}Targets Built:{Color.RESET} {len(self.results)}/{len(targets_to_build)}"
        )
        print(
            f"  {Color.BOLD}Successful:{Color.RESET} {success_count}/{len(self.results)}"
        )

        # Print individual results
        if self.results:
            print(f"\n{Color.BOLD}Target Results:{Color.RESET}")
            # Sort results for consistent ordering
            sorted_triples = sorted(self.results.keys())
            target_map = {t.triple: t for t in TARGETS}
            for triple in sorted_triples:
                target = target_map.get(triple)
                if not target:
                    continue  # Should not happen
                success = self.results[triple]
                status_icon = (
                    f"{Color.GREEN}✓{Color.RESET}"
                    if success
                    else f"{Color.RED}✗{Color.RESET}"
                )
                print(
                    f"  {status_icon} {target.color}{target.display_name}{Color.RESET}"
                )
        else:
            print(
                f"\n{Color.BOLD}Target Results:{Color.RESET} No targets were processed."
            )

        print(f"\n{Color.BRIGHT_MAGENTA}{'═' * 62}{Color.RESET}\n")

        return overall_success


class BuildOptions:
    """Options for the build process"""

    def __init__(
        self,
        targets: Optional[List[str]] = None,
        release: bool = True,
        verbose: bool = False,
        fail_fast: bool = False,
        extra_flags: Optional[List[str]] = None,
        env_vars: Optional[Dict[str, str]] = None,
        list_targets: bool = False,  # Added list_targets option here
    ):
        self.targets = (
            targets if targets is not None else list(DEFAULT_ENABLED_TARGETS)
        )  # Ensure it's a list
        self.release = release
        self.verbose = verbose
        self.fail_fast = fail_fast
        self.extra_flags = extra_flags or []
        self.env_vars = env_vars or {}
        self.list_targets = list_targets


def parse_args() -> BuildOptions:
    """Parse command line arguments"""
    parser = argparse.ArgumentParser(
        description="Cross-platform build script using 'cross-rs' and 'tqdm'.",
        formatter_class=argparse.RawTextHelpFormatter,  # Allows better formatting in help
        epilog=f"""
Examples:
  Build default targets in release mode:
    {sys.argv[0]}
  Build only Linux x86_64 in debug mode:
    {sys.argv[0]} --targets x86_64-unknown-linux-gnu --debug
  List available targets:
    {sys.argv[0]} --list-targets
  Build with extra cargo features and verbose output:
    {sys.argv[0]} --verbose --extra-flags "--features=feature1 no-default-features"
  Pass environment variable for linking:
    {sys.argv[0]} --env 'MY_LIB_PATH=/opt/custom/lib'
""",
    )

    # Target options
    target_group = parser.add_argument_group("Target Selection")
    target_group.add_argument(
        "--targets",
        nargs="*",
        metavar="TARGET_TRIPLE",
        default=argparse.SUPPRESS,  # Don't show default here, handle manually later
        help="Specific target triples to build (space-separated).\n"
        f"Default: {' '.join(DEFAULT_ENABLED_TARGETS)}",
    )
    target_group.add_argument(
        "--list-targets",
        action="store_true",
        help="List all available target triples and exit.",
    )

    # Build options
    build_group = parser.add_argument_group("Build Configuration")
    build_group.add_argument(
        "--release",
        action="store_true",
        default=True,
        help="Build in release mode (default).",
    )
    build_group.add_argument(
        "--debug",
        action="store_false",
        dest="release",  # Set release to False if --debug is used
        help="Build in debug mode (disables release mode).",
    )
    build_group.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose output from this script and 'cross'.\n"
        "Disables the main progress bar for cleaner logs.",
    )
    build_group.add_argument(
        "--fail-fast",
        action="store_true",
        help="Stop the entire build process after the first target fails.",
    )
    build_group.add_argument(
        "--extra-flags",
        nargs=argparse.REMAINDER,  # Consume remaining args
        metavar="FLAGS",
        default=[],
        help="Remaining arguments are passed directly to 'cross build'.\n"
        "Example: --extra-flags --no-default-features --features=foo",
    )

    # Environment options
    env_group = parser.add_argument_group("Environment")
    env_group.add_argument(
        "--env",
        nargs="*",
        metavar="KEY=VALUE",
        default=[],
        help="Set additional environment variables for the build process.\n"
        "Example: --env CFLAGS=-O3 LDFLAGS=-L/path/to/libs",
    )

    args = parser.parse_args()

    # Handle --list-targets immediately
    if args.list_targets:
        print(f"{Color.BOLD}Available Build Targets:{Color.RESET}")
        print(
            f"{Color.BRIGHT_BLACK}(Targets marked with ✓ are enabled by default){Color.RESET}"
        )
        for target in TARGETS:
            enabled = "✓" if target.triple in DEFAULT_ENABLED_TARGETS else " "
            print(f"  [{enabled}] {target.color}{target.display_name}{Color.RESET}")
        sys.exit(0)

    # Parse environment variables
    env_vars = {}
    for env_var in args.env:
        if "=" in env_var:
            key, value = env_var.split("=", 1)
            env_vars[key.strip()] = value.strip()
        else:
            print(
                f"{Color.YELLOW}Warning:{Color.RESET} Ignoring malformed environment variable (expected KEY=VALUE): '{env_var}'"
            )

    # Determine targets: use specified list or default
    build_targets = (
        args.targets if hasattr(args, "targets") else DEFAULT_ENABLED_TARGETS
    )

    # Validate selected targets
    valid_targets = [t.triple for t in TARGETS]
    invalid_targets = [t for t in build_targets if t not in valid_targets]
    if invalid_targets:
        print(
            f"{Color.RED}Error:{Color.RESET} Invalid target(s) specified: {', '.join(invalid_targets)}"
        )
        print(f"Use `{sys.argv[0]} --list-targets` to see available options.")
        sys.exit(1)

    return BuildOptions(
        targets=build_targets,
        release=args.release,
        verbose=args.verbose,
        fail_fast=args.fail_fast,
        extra_flags=args.extra_flags,
        env_vars=env_vars,
        list_targets=args.list_targets,  # Pass through just in case
    )


def main():
    """Main entry point"""
    # Initialize color support early
    Color.disable_if_not_supported()

    # Check for 'cross' command existence
    if shutil.which("cross") is None:
        print(
            f"{Color.RED}Error: Required command 'cross' not found in your PATH.{Color.RESET}"
        )
        print("Please install cross-rs: https://github.com/cross-rs/cross")
        print(
            "Example installation: cargo install cross --git https://github.com/cross-rs/cross"
        )
        sys.exit(1)

    # Parse command line arguments
    options = parse_args()

    # Run the build process
    builder = Builder(options)
    success = builder.build_all()

    # Return appropriate exit code
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print(f"\n\n{Color.YELLOW}Build interrupted by user.{Color.RESET}")
        # Attempt to clean up tqdm bar if it's active
        try:
            # This might not always work perfectly depending on interrupt point
            # but it's better than leaving a broken bar state sometimes.
            tqdm.write("")  # Try to ensure cursor is on a new line
        except Exception:
            pass  # Ignore errors during cleanup
        sys.exit(130)  # Standard exit code for Ctrl+C
    except Exception as e:
        # General error handler
        print(f"\n\n{Color.RED}An unexpected error occurred:{Color.RESET}")
        print(f"{Color.BRIGHT_RED}{type(e).__name__}: {str(e)}{Color.RESET}")
        # Optionally print traceback if requested or in debug mode
        if "--verbose" in sys.argv or os.environ.get("BUILD_DEBUG"):
            import traceback

            print("\n--- Traceback ---")
            traceback.print_exc()
            print("-----------------\n")
        sys.exit(1)
