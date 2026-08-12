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


@app.command(name="notify-failure")
def notify_failure(
    jobs: str = typer.Option(..., "--jobs", help="Comma- or space-separated failed job names"),
    run_url: str = typer.Option(..., "--run-url", help="URL of the failed workflow run"),
    channel: str = typer.Option("", "--channel", help="Slack channel (default: schedule's notification_channel)"),
    dry_run: bool = typer.Option(False, "--dry-run", help="Print the message instead of posting"),
) -> None:
    """Post a workflow-failure alert to Slack, @-mentioning the current oncaller(s).

    If no current shift covers today, posts to the channel without an @-mention.
    """
    from oncall.notify import _current_shift
    from oncall.slack import client as slack_client
    from oncall.slack import email_for, lookup_user_id
    from oncall.slack import post as slack_post

    job_list = ", ".join(j for j in jobs.replace(",", " ").split() if j)
    if not job_list:
        console.print("[red]error[/]: --jobs is empty")
        raise typer.Exit(1)

    path = _schedule_path()
    # An unparseable schedule is a likely cause of `review` failing, so don't
    # let it silence the alert — fall back to the channel default and skip the
    # @-mention (we can't know the current oncaller without a valid schedule).
    sched: ScheduleFile | None = None
    parse_error: str | None = None
    try:
        sched = parse(path.read_text())
    except (OSError, ValueError) as e:
        parse_error = str(e)
        console.print(f"[yellow]warn[/]: schedule unparseable ({e}); posting without @-mention")

    target_channel = channel or (sched.slack_config.notification_channel if sched else "#oncall")

    # `oncall-founders` is the escalation rotation — skip it for routine
    # workflow-failure pings; only the primary oncaller(s) get notified.
    escalation_rotations = {"oncall-founders"}

    oncall_names: list[str] = []
    if sched is not None:
        current = _current_shift(sched, datetime.date.today())
        if current is not None:
            for rot in sched.roster.rotations_in_order():
                if rot in escalation_rotations:
                    continue
                name = current.assignments.get(rot)
                if name:
                    oncall_names.append(name)

    def _build(mentions: list[str]) -> str:
        prefix = (" ".join(mentions) + " ") if mentions else ""
        suffix = f" (schedule.oncall is unparseable: {parse_error})" if parse_error else ""
        return f"{prefix}oncall workflow failed: {job_list} — needs a fix. <{run_url}|View run>{suffix}"

    if dry_run:
        text = _build([f"@{n}" for n in oncall_names])
        console.print(f"[bold]→ {target_channel}[/]")
        console.print(text)
        return

    wc = slack_client()
    mentions: list[str] = []
    for name in oncall_names:
        try:
            mentions.append(f"<@{lookup_user_id(wc, email_for(name))}>")
        except Exception as e:
            console.print(f"[yellow]warn[/]: failed to look up Slack user for {name}: {e}")

    slack_post(wc, target_channel, _build(mentions))
    console.print(f"[green]posted to {target_channel}[/]")
