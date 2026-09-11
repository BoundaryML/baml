"""Typer app for `bctl oncall` subcommands."""

from __future__ import annotations

import datetime
import os
import re
from pathlib import Path
from zoneinfo import ZoneInfo

import typer
from rich.console import Console

from oncall.current import current_oncall
from oncall.notify import compose_handoff, deliver_handoff, notification_friday, validate_parent_window
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
def current() -> None:
    """Print the current release on-call name (Pacific time)."""
    try:
        names = current_oncall()
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        typer.echo(f"Could not read current on-call schedule: {error}", err=True)
        raise typer.Exit(1)
    for name in names:
        typer.echo(name)


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
    post_to_slack: bool = typer.Option(False, "--post-to-slack", help="Post and schedule in Slack using a durable GitHub journal"),
    run_started_at: str = typer.Option("", help="Original workflow creation time (ISO 8601 with timezone), stable across reruns"),
) -> None:
    """Notify Thursday's incoming Friday oncaller and schedule two threaded replies."""
    text, sched = _parse_or_die(_schedule_path())
    blocking = [e for e in validate(sched, text) if not e.fixable]
    if blocking:
        for e in blocking:
            console.print(f"[red]error[/] {e.message}")
        raise typer.Exit(1)
    try:
        reference = datetime.datetime.fromisoformat(run_started_at) if run_started_at else datetime.datetime.now(ZoneInfo("America/Los_Angeles"))
        friday = notification_friday(reference)
        if not post_to_slack:
            plan = compose_handoff(sched, friday, None)
            typer.echo(f"→ {plan['channel']} (incoming shift {friday})")
            typer.echo(plan["parent"]["text"])
            for reminder in plan["reminders"]:
                when = datetime.datetime.fromtimestamp(reminder["post_at"], ZoneInfo("America/Los_Angeles"))
                typer.echo(f"{when.isoformat()} in the same thread: {reminder['text']}")
            return

        from oncall.notification_state import GitHubState
        from oncall.slack import client as slack_client

        repository = os.environ.get("GITHUB_REPOSITORY")
        if not repository:
            raise RuntimeError("GITHUB_REPOSITORY must be set for durable notification state")
        journal = GitHubState(repository, friday)
        state = journal.load()
        wc = slack_client(retry_handlers=[])
        if state is None:
            validate_parent_window(friday, datetime.datetime.now(ZoneInfo("America/Los_Angeles")))
            state = compose_handoff(sched, friday, wc)
            journal.save(state)
        if state["friday"] != friday.isoformat():
            raise RuntimeError("journal shift date does not match this run")
        deliver_handoff(wc, state, journal.save)
        typer.echo(f"Handoff posted; both Friday replies scheduled in {state['parent']['channel']} thread {state['parent']['ts']}")
    except (RuntimeError, ValueError) as e:
        console.print(f"[red]error[/]: {e}")
        raise typer.Exit(1)


@app.command(name="test-notify")
def test_notify(
    parent_ts: str = typer.Option(..., help="Existing #sam-sandbox parent thread timestamp"),
    run_id: str = typer.Option(..., help="GitHub Actions run ID; reruns reuse the same test journal"),
) -> None:
    """Exercise Python's actual Slack scheduling path in an existing sandbox thread."""
    from oncall.notification_state import GitHubState
    from oncall.slack import client as slack_client

    # This explicit test entry point can never send to the production channel.
    channel = "C07UTQN7N1X"
    if not re.fullmatch(r"[0-9]+\.[0-9]+", parent_ts):
        raise typer.BadParameter("parent timestamp must be an exact Slack timestamp string")
    repository = os.environ.get("GITHUB_REPOSITORY")
    if not repository:
        raise RuntimeError("GITHUB_REPOSITORY must be set for durable notification state")
    now = datetime.datetime.now(ZoneInfo("America/Los_Angeles"))
    friday = now.date() + datetime.timedelta(days=(4 - now.weekday()) % 7 or 7)
    journal = GitHubState(repository, friday, sandbox_run_id=run_id)
    state = journal.load()
    wc = slack_client(retry_handlers=[])
    if state is None:
        _, sched = _parse_or_die(_schedule_path())
        state = compose_handoff(sched, friday, wc)
        state["channel"] = channel
        state["parent"].update(status="sent", channel=channel, ts=parent_ts)
        for reminder, delay, label in zip(state["reminders"], [90, 180], ["Friday 9am", "Friday 3pm"]):
            reminder["post_at"] = int(now.timestamp()) + delay
            reminder["text"] = f"[Python tooling test: {label} Pacific reminder; accelerated delivery]\n{reminder['text']}"
        journal.save(state)
    if state["channel"] != channel or state["parent"].get("channel") != channel or state["parent"].get("ts") != parent_ts:
        raise RuntimeError("sandbox journal does not match the requested thread")
    deliver_handoff(wc, state, journal.save)
    for reminder in state["reminders"]:
        typer.echo(f"Queued {reminder['scheduled_message_id']} at {reminder['post_at']} in {channel} thread {parent_ts}")


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

    target_channel = channel or (sched.slack_config.notification_channel if sched else "#general")

    oncall_names: list[str] = []
    if sched is not None:
        today = datetime.datetime.now(ZoneInfo("America/Los_Angeles")).date()
        current = _current_shift(sched, today)
        if current is not None:
            name = current.assignments.get("oncall-releases")
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
