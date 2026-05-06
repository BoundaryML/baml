"""Typer app for `bctl oncall` subcommands."""

from __future__ import annotations

import datetime
from pathlib import Path

import typer
from rich.console import Console

from oncall.notify import compose_handoff
from oncall.parser import ScheduleFile, emit, parse
from oncall.schedule import canonicalize, fill_horizon, validate

app = typer.Typer(help="On-call scheduler")
console = Console()


def _schedule_path() -> Path:
    return Path(__file__).parent / "data" / "schedule.oncall"


def _parse_or_die(path: Path) -> tuple[str, ScheduleFile]:
    text = path.read_text()
    try:
        sched = parse(text)
    except ValueError as e:
        console.print(f"[red]parse error[/]: {e}")
        raise typer.Exit(1)
    return text, sched


@app.command()
def check(fix: bool = typer.Option(False, "--fix", help="Rewrite file to canonical form")) -> None:
    """Validate the schedule file; with --fix, rewrite to canonical form."""
    path = _schedule_path()
    text, sched = _parse_or_die(path)
    errors = validate(sched, text)

    blocking = [e for e in errors if not e.fixable]
    fixable = [e for e in errors if e.fixable]

    if blocking:
        for e in blocking:
            loc = f"L{e.line}: " if e.line else ""
            console.print(f"[red]error[/] {loc}{e.message}")
        raise typer.Exit(1)

    if fixable and not fix:
        for e in fixable:
            loc = f"L{e.line}: " if e.line else ""
            console.print(f"[yellow]fixable[/] {loc}{e.message}")
        console.print("Run `bctl oncall check --fix` to auto-repair.")
        raise typer.Exit(1)

    if fix:
        canonical = canonicalize(sched)
        path.write_text(emit(canonical))
        console.print(f"[green]fixed[/] {path}")
        return

    console.print(f"[green]ok[/] {path}")


@app.command(name="fill-schedule")
def fill_schedule() -> None:
    """Lazily extend the schedule horizon to ~4 months when it drops below ~1 month."""
    path = _schedule_path()
    text, sched = _parse_or_die(path)
    errors = validate(sched, text)
    blocking = [e for e in errors if not e.fixable]
    if blocking:
        for e in blocking:
            loc = f"L{e.line}: " if e.line else ""
            console.print(f"[red]error[/] {loc}{e.message}")
        raise typer.Exit(1)

    # `date.today()` is local-tz; UTC drift can shift the perceived day by up
    # to ~1h around midnight. That's fine here — we're picking which Friday
    # to extend toward, and a one-day jitter doesn't matter.
    today = datetime.date.today()
    filled = fill_horizon(sched, today)
    if filled.shifts == sched.shifts:
        console.print("Horizon already >=1 month; no changes.")
        raise typer.Exit(0)

    canonical = canonicalize(filled)
    path.write_text(emit(canonical))
    added = len(filled.shifts) - len(sched.shifts)
    console.print(f"[green]appended {added} weeks[/]")


@app.command()
def notify(
    post_to_slack: bool = typer.Option(False, "--post-to-slack", help="Actually post to Slack"),
) -> None:
    """Compose and (optionally) post the weekly on-call handoff."""
    path = _schedule_path()

    wc = None
    if post_to_slack:
        from oncall.slack import client as slack_client

        wc = slack_client()

    text, sched = _parse_or_die(path)
    errors = validate(sched, text)
    blocking = [e for e in errors if not e.fixable]
    if blocking:
        for e in blocking:
            loc = f"L{e.line}: " if e.line else ""
            console.print(f"[red]error[/] {loc}{e.message}")
        raise typer.Exit(1)
    try:
        # `date.today()` is local-tz; UTC drift can shift the perceived day
        # by up to ~1h around midnight. Fine — handoffs run on a weekly cron
        # well away from any boundary.
        msgs = compose_handoff(sched, datetime.date.today(), wc)
    except RuntimeError as e:
        console.print(f"[red]error[/]: {e}")
        raise typer.Exit(1)

    if post_to_slack:
        from oncall.slack import post as slack_post

        for channel, body in msgs:
            slack_post(wc, channel, body)
        console.print(f"[green]posted {len(msgs)} message(s)[/]")
    else:
        for channel, body in msgs:
            console.print(f"[bold]→ {channel}[/]")
            console.print(body)
            console.print()
