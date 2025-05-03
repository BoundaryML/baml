#!/usr/bin/env python3

import typer
from rich.console import Console
from rich.panel import Panel
from rich.progress import track
from typing import Optional
import time
from enum import Enum

from integ_tests import run_all_integ_tests, run_python_integ_tests, run_typescript_integ_tests, run_ruby_integ_tests

class TestSuite(str, Enum):
    ALL = "all"
    PYTHON = "python"
    TYPESCRIPT = "typescript"
    RUBY = "ruby"

# Initialize Typer app and Rich console
app = typer.Typer(help="BAML CLI tool for development tasks")

# Global state for CI mode
class State:
    ci: bool = False

state = State()
console = Console()

@app.callback()
def main(ci: bool = typer.Option(False, "--ci", help="Run in CI mode (non-interactive)")):
    """
    BAML CLI tool for development tasks.
    """
    state.ci = ci
    global console
    console = Console(force_terminal=not state.ci)

@app.command()
def integ_tests(
    test_suite: TestSuite = typer.Option(TestSuite.ALL, "--suite", "-s", help="Test suite to run"),
    verbose: bool = typer.Option(False, "--verbose", "-v", help="Enable verbose output"),
):
    """
    Run integration tests for BAML.
    """
    # Create console with appropriate interactivity setting
    
    console.print(Panel(f"🧪 Running integration tests for suite: [bold blue]{test_suite.value}[/]"))
    
    if verbose:
        console.print("[yellow]Verbose mode enabled[/]")

    # Simulate test execution with progress bar
    match test_suite:
        case TestSuite.ALL:
            run_all_integ_tests()
        case TestSuite.PYTHON:
            run_python_integ_tests()
        case TestSuite.TYPESCRIPT:
            run_typescript_integ_tests()
        case TestSuite.RUBY:
            run_ruby_integ_tests()
        case _:
            console.print("[bold red]Invalid test suite[/]")
            return
    
    console.print("[bold green]✓[/] Integration tests completed!")

if __name__ == "__main__":
    app()
