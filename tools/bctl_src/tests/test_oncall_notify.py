"""Incoming shift selection, Slack request contracts, and crash-safe retries."""

import copy
import datetime as dt
import subprocess
from unittest.mock import Mock

import pytest
from typer.testing import CliRunner

from oncall.cli import app
from oncall.current import current_oncall
from oncall.notification_state import GitHubState
from oncall.notify import PACIFIC, compose_handoff, deliver_handoff, notification_friday
from oncall.parser import Roster, ScheduleFile, ShiftLine, SlackConfig
from oncall.slack import client

FRIDAY = dt.date(2026, 9, 18)
THURSDAY = dt.datetime(2026, 9, 17, 17, tzinfo=PACIFIC)


@pytest.fixture
def schedule():
    return ScheduleFile(
        slack_config=SlackConfig("#general"),
        roster=Roster({"oncall-releases": ["sam", "vbv"]}),
        shifts=[
            ShiftLine(FRIDAY - dt.timedelta(days=7), {"oncall-releases": "sam"}),
            ShiftLine(FRIDAY, {"oncall-releases": "vbv"}),
        ],
    )


@pytest.fixture
def slack():
    wc = Mock()
    wc.users_lookupByEmail.return_value = {"user": {"id": "UINCOMING"}}
    wc.chat_postMessage.return_value = {"ok": True, "channel": "CGENERAL", "ts": "1789700000.123456"}
    wc.chat_scheduleMessage.side_effect = [
        {"ok": True, "scheduled_message_id": "Q1"},
        {"ok": True, "scheduled_message_id": "Q2"},
    ]
    return wc


class Journal:
    def __init__(self, state):
        self.state = copy.deepcopy(state)
        self.snapshots = []

    def save(self, state):
        self.state = copy.deepcopy(state)
        self.snapshots.append(self.state)


def test_incoming_shift_and_real_mention_without_changing_rotation(schedule, slack, monkeypatch):
    monkeypatch.setattr("oncall.current.parse", lambda _: schedule)
    before = copy.deepcopy(schedule)
    plan = compose_handoff(schedule, FRIDAY, slack)
    assert schedule == before
    slack.users_lookupByEmail.assert_called_once_with(email="vbv@boundaryml.com")
    assert plan["mention"] == "<@UINCOMING>"
    assert plan["channel"] == "#general"
    assert plan["parent"]["text"] == "Hey <@UINCOMING>, you’re going to be oncall tomorrow! You’ll be in charge of putting out a canary release tomorrow and monitoring Discord over the weekend and next week. Your rotation runs Friday through Friday."
    assert [r["text"] for r in plan["reminders"]] == [
        "Hey <@UINCOMING>, reminder to put out the canary release today!",
        "Hey <@UINCOMING>, reminder to make sure the canary release has succeeded and the changelog is posted!",
    ]
    assert current_oncall(dt.date(2026, 9, 17)) == ["sam"]
    assert current_oncall(dt.date(2026, 9, 18)) == ["vbv"]


@pytest.mark.parametrize("friday", [FRIDAY + dt.timedelta(days=7), FRIDAY - dt.timedelta(days=1)])
def test_missing_or_non_friday_shift_fails(schedule, friday):
    with pytest.raises(RuntimeError):
        compose_handoff(schedule, friday, None)


@pytest.mark.parametrize("user_id", ["@vbv", "", "literal-placeholder"])
def test_invalid_slack_identity_fails(schedule, slack, user_id):
    slack.users_lookupByEmail.return_value = {"user": {"id": user_id}}
    with pytest.raises(RuntimeError, match="real user ID"):
        compose_handoff(schedule, FRIDAY, slack)
    slack.chat_postMessage.assert_not_called()


@pytest.mark.parametrize("friday,utc_hours", [
    (dt.date(2026, 3, 6), [17, 23]),
    (dt.date(2026, 3, 13), [16, 22]),
    (dt.date(2026, 10, 30), [16, 22]),
    (dt.date(2026, 11, 6), [17, 23]),
])
def test_dst_local_friday_times(schedule, friday, utc_hours):
    schedule.shifts[:] = [ShiftLine(friday, {"oncall-releases": "vbv"})]
    plan = compose_handoff(schedule, friday, None)
    assert [dt.datetime.fromtimestamp(r["post_at"], dt.timezone.utc).hour for r in plan["reminders"]] == utc_hours
    assert [dt.datetime.fromtimestamp(r["post_at"], PACIFIC).hour for r in plan["reminders"]] == [9, 15]


def test_utc_friday_is_pacific_thursday():
    assert notification_friday(dt.datetime.fromisoformat("2026-09-18T00:00:00Z")) == FRIDAY
    assert notification_friday(dt.datetime.fromisoformat("2026-11-06T01:00:00Z")) == dt.date(2026, 11, 6)
    with pytest.raises(RuntimeError, match="Thursday"):
        notification_friday(dt.datetime(2026, 9, 18, 9, tzinfo=PACIFIC))
    with pytest.raises(RuntimeError, match="timezone"):
        notification_friday(dt.datetime(2026, 9, 17, 17))


def test_threading_checkpoints_and_completed_retry(schedule, slack):
    state = compose_handoff(schedule, FRIDAY, slack)
    journal = Journal(state)
    deliver_handoff(slack, state, journal.save, now=lambda: THURSDAY)
    assert [s["parent"]["status"] for s in journal.snapshots[:2]] == ["pending", "sent"]
    assert journal.snapshots[2]["reminders"][0]["status"] == "pending"
    assert journal.snapshots[4]["reminders"][1]["status"] == "pending"
    for call, reminder in zip(slack.chat_scheduleMessage.call_args_list, state["reminders"]):
        assert call.kwargs == {
            "channel": "CGENERAL", "thread_ts": "1789700000.123456",
            "post_at": reminder["post_at"], "text": reminder["text"],
            "unfurl_links": False, "unfurl_media": False,
        }
    # Completed retries after delivery must not try to recreate scheduled messages.
    deliver_handoff(slack, copy.deepcopy(journal.state), journal.save, now=lambda: THURSDAY + dt.timedelta(days=8))
    assert slack.chat_postMessage.call_count == 1
    assert slack.chat_scheduleMessage.call_count == 2


@pytest.mark.parametrize("failed_step", ["parent", "first", "second"])
def test_uncertain_slack_request_never_replayed(schedule, slack, failed_step):
    state = compose_handoff(schedule, FRIDAY, slack)
    journal = Journal(state)
    if failed_step == "parent":
        slack.chat_postMessage.side_effect = TimeoutError("response lost")
    elif failed_step == "first":
        slack.chat_scheduleMessage.side_effect = TimeoutError("response lost")
    else:
        slack.chat_scheduleMessage.side_effect = [{"ok": True, "scheduled_message_id": "Q1"}, TimeoutError("response lost")]
    with pytest.raises(TimeoutError):
        deliver_handoff(slack, state, journal.save, now=lambda: THURSDAY)
    calls = copy.copy(slack.mock_calls)
    with pytest.raises(RuntimeError, match="Uncertain Slack send"):
        deliver_handoff(slack, copy.deepcopy(journal.state), journal.save, now=lambda: THURSDAY)
    assert slack.mock_calls == calls


@pytest.mark.parametrize("failed_save", range(1, 7))
def test_journal_failure_before_or_after_slack_send(schedule, slack, failed_save):
    state = compose_handoff(schedule, FRIDAY, slack)
    journal = Journal(state)
    n = 0

    def save(value):
        nonlocal n
        n += 1
        if n == failed_save:
            raise RuntimeError("journal unavailable")
        journal.save(value)

    with pytest.raises(RuntimeError, match="journal unavailable"):
        deliver_handoff(slack, state, save, now=lambda: THURSDAY)
    # Pre-request failure leaves ready and is safely resumable; post-request
    # failure leaves pending and cannot duplicate an accepted Slack request.
    if failed_save % 2 == 0:
        calls = copy.copy(slack.mock_calls)
        with pytest.raises(RuntimeError, match="Uncertain Slack send"):
            deliver_handoff(slack, copy.deepcopy(journal.state), journal.save, now=lambda: THURSDAY)
        assert slack.mock_calls == calls
    else:
        deliver_handoff(slack, copy.deepcopy(journal.state), journal.save, now=lambda: THURSDAY)
        assert slack.chat_postMessage.call_count == 1
        assert slack.chat_scheduleMessage.call_count == 2


def test_resume_keeps_original_mention_and_thread_after_swap(schedule, slack):
    state = compose_handoff(schedule, FRIDAY, slack)
    state["parent"].update(status="sent", channel="CGENERAL", ts="123.456789")
    state["reminders"][0].update(status="sent", scheduled_message_id="Q1")
    schedule.shifts[-1].assignments["oncall-releases"] = "sam"
    deliver_handoff(slack, state, Mock(), now=lambda: THURSDAY + dt.timedelta(hours=17))
    slack.chat_postMessage.assert_not_called()
    slack.chat_scheduleMessage.assert_called_once()
    assert slack.chat_scheduleMessage.call_args.kwargs["thread_ts"] == "123.456789"
    assert "<@UINCOMING>" in slack.chat_scheduleMessage.call_args.kwargs["text"]
    assert slack.users_lookupByEmail.call_count == 1


def test_late_or_early_parent_and_overdue_reminder_fail(schedule, slack):
    state = compose_handoff(schedule, FRIDAY, slack)
    for now in [THURSDAY - dt.timedelta(hours=1), THURSDAY + dt.timedelta(days=1)]:
        with pytest.raises(RuntimeError, match="new parent"):
            deliver_handoff(slack, state, Mock(), now=lambda: now)
    slack.chat_postMessage.assert_not_called()
    state["parent"].update(status="sent", channel="CGENERAL", ts="123.456789")
    with pytest.raises(RuntimeError, match="deadline passed"):
        deliver_handoff(slack, state, Mock(), now=lambda: THURSDAY + dt.timedelta(hours=16))
    slack.chat_scheduleMessage.assert_not_called()


def test_sdk_transport_retries_disabled(monkeypatch):
    monkeypatch.setenv("SLACK_BOUNDARY_BOT_TOKEN", "test-token")
    assert client(retry_handlers=[]).retry_handlers == []


def test_github_state_compare_and_swap_and_roundtrip():
    journal = GitHubState("BoundaryML/baml", FRIDAY)
    journal._api = Mock(return_value={"content": {"sha": "first"}})
    journal.save({"friday": str(FRIDAY)})
    assert "sha" not in journal._api.call_args.kwargs["body"]
    journal._api.return_value = {"content": {"sha": "second"}}
    journal.save({"friday": str(FRIDAY)})
    assert journal._api.call_args.kwargs["body"]["sha"] == "first"
    body = journal._api.call_args.kwargs["body"]
    journal._api.side_effect = [{"object": {"sha": "base"}}, {"sha": "second", "content": body["content"]}]
    assert journal.load() == {"friday": str(FRIDAY)}
    assert journal.sha == "second"


def test_github_state_bootstrap_and_missing_file():
    journal = GitHubState("BoundaryML/baml", FRIDAY)
    journal._api = Mock(side_effect=[None, {"default_branch": "canary"}, {"object": {"sha": "base"}}, {}, None])
    assert journal.load() is None
    assert journal._api.call_args_list[3].kwargs["body"] == {"ref": "refs/heads/oncall/notification-state", "sha": "base"}


@pytest.mark.parametrize("error", ["gh: failed (HTTP 403)", "gh: failed (HTTP 409)", "connection lost"])
def test_github_errors_do_not_look_like_missing_state(monkeypatch, error):
    monkeypatch.setattr(subprocess, "run", Mock(return_value=subprocess.CompletedProcess([], 1, "", error)))
    with pytest.raises(RuntimeError, match="state API failed"):
        GitHubState("BoundaryML/baml", FRIDAY).load()


def test_dry_run_has_no_network(monkeypatch, schedule):
    from oncall.parser import emit

    monkeypatch.setattr("oncall.cli._parse_or_die", lambda _: (emit(schedule), schedule))
    monkeypatch.setattr("oncall.slack.client", Mock(side_effect=AssertionError("unexpected Slack call")))
    monkeypatch.setattr("oncall.notification_state.GitHubState", Mock(side_effect=AssertionError("unexpected GitHub call")))
    result = CliRunner().invoke(app, ["notify", "--run-started-at", "2026-09-18T00:00:00Z"])
    assert result.exit_code == 0, result.output
    assert "Hey @vbv" in result.output
    assert "2026-09-18T09:00:00-07:00" in result.output
    assert "2026-09-18T15:00:00-07:00" in result.output


def test_early_dispatch_does_not_pin_assignee_before_shift_swap(monkeypatch, schedule, slack):
    from oncall.parser import emit

    clock = [THURSDAY - dt.timedelta(hours=1)]

    class FrozenDatetime(dt.datetime):
        @classmethod
        def now(cls, tz=None):
            return clock[0].astimezone(tz)

    monkeypatch.setattr(dt, "datetime", FrozenDatetime)
    monkeypatch.setattr("oncall.cli._parse_or_die", lambda _: (emit(schedule), schedule))
    monkeypatch.setenv("GITHUB_REPOSITORY", "BoundaryML/baml")
    journal = Mock()
    journal.load.return_value = None
    monkeypatch.setattr("oncall.notification_state.GitHubState", Mock(return_value=journal))
    monkeypatch.setattr("oncall.slack.client", Mock(return_value=slack))
    runner = CliRunner()
    early = runner.invoke(app, ["notify", "--post-to-slack"])
    assert early.exit_code == 1
    assert "a new parent can only" in early.output
    journal.save.assert_not_called()
    slack.users_lookupByEmail.assert_not_called()
    slack.chat_postMessage.assert_not_called()

    schedule.shifts[-1].assignments["oncall-releases"] = "sam"
    clock[0] = THURSDAY
    on_time = runner.invoke(app, ["notify", "--post-to-slack"])
    assert on_time.exit_code == 0, on_time.output
    slack.users_lookupByEmail.assert_called_once_with(email="sam@boundaryml.com")
    assert slack.chat_postMessage.call_count == 1
    assert slack.chat_scheduleMessage.call_count == 2


def test_sandbox_cli_reuses_thread_and_isolated_journal(monkeypatch, schedule, slack):
    from oncall.parser import emit

    monkeypatch.setattr("oncall.cli._parse_or_die", lambda _: (emit(schedule), schedule))
    monkeypatch.setenv("GITHUB_REPOSITORY", "BoundaryML/baml")

    class FrozenDatetime(dt.datetime):
        @classmethod
        def now(cls, tz=None):
            return THURSDAY.astimezone(tz)

    monkeypatch.setattr(dt, "datetime", FrozenDatetime)
    journal = Mock()
    journal.load.return_value = None
    factory = Mock(return_value=journal)
    monkeypatch.setattr("oncall.notification_state.GitHubState", factory)
    monkeypatch.setattr("oncall.slack.client", Mock(return_value=slack))
    runner = CliRunner()
    result = runner.invoke(app, ["test-notify", "--parent-ts", "1789163124.808429", "--run-id", "12345"])
    assert result.exit_code == 0, result.output
    factory.assert_called_once_with("BoundaryML/baml", FRIDAY, sandbox_run_id="12345")
    slack.chat_postMessage.assert_not_called()
    for call, delay in zip(slack.chat_scheduleMessage.call_args_list, [90, 180]):
        assert call.kwargs["channel"] == "C07UTQN7N1X"
        assert call.kwargs["thread_ts"] == "1789163124.808429"
        assert call.kwargs["post_at"] == int(THURSDAY.timestamp()) + delay
        assert "<@UINCOMING>" in call.kwargs["text"]
        assert "Python tooling test" in call.kwargs["text"]
    journal.load.return_value = copy.deepcopy(journal.save.call_args.args[0])
    retry = runner.invoke(app, ["test-notify", "--parent-ts", "1789163124.808429", "--run-id", "12345"])
    assert retry.exit_code == 0, retry.output
    assert slack.chat_scheduleMessage.call_count == 2
    changed_thread = runner.invoke(app, ["test-notify", "--parent-ts", "999.123456", "--run-id", "12345"])
    assert changed_thread.exit_code != 0
    assert slack.chat_scheduleMessage.call_count == 2


def test_sandbox_state_namespace_cannot_overwrite_production():
    production = GitHubState("BoundaryML/baml", FRIDAY)
    sandbox = GitHubState("BoundaryML/baml", FRIDAY, sandbox_run_id="12345")
    assert production.path.endswith("notifications/2026-09-18.json")
    assert sandbox.path.endswith("notifications/sandbox/12345.json")
    with pytest.raises(RuntimeError, match="numeric run ID"):
        GitHubState("BoundaryML/baml", FRIDAY, sandbox_run_id="../2026-09-18")


def test_new_sandbox_thread_posts_all_three_and_retries_without_duplicates(monkeypatch, schedule, slack):
    from oncall.parser import emit

    monkeypatch.setattr("oncall.cli._parse_or_die", lambda _: (emit(schedule), schedule))
    monkeypatch.setenv("GITHUB_REPOSITORY", "BoundaryML/baml")
    # Friday is intentionally outside the production parent send window.
    test_time = THURSDAY - dt.timedelta(days=6)

    class FrozenDatetime(dt.datetime):
        @classmethod
        def now(cls, tz=None):
            return test_time.astimezone(tz)

    monkeypatch.setattr(dt, "datetime", FrozenDatetime)
    journal = Mock()
    journal.load.return_value = None
    monkeypatch.setattr("oncall.notification_state.GitHubState", Mock(return_value=journal))
    monkeypatch.setattr("oncall.slack.client", Mock(return_value=slack))
    slack.chat_postMessage.return_value = {"ok": True, "channel": "C07UTQN7N1X", "ts": "123.456789"}
    runner = CliRunner()
    result = runner.invoke(app, ["test-notify", "--run-id", "56789"])
    assert result.exit_code == 0, result.output
    slack.chat_postMessage.assert_called_once()
    assert slack.chat_postMessage.call_args.kwargs["channel"] == "C07UTQN7N1X"
    assert "Thursday 5pm" in slack.chat_postMessage.call_args.kwargs["text"]
    assert "<@UINCOMING>" in slack.chat_postMessage.call_args.kwargs["text"]
    for call in slack.chat_scheduleMessage.call_args_list:
        assert call.kwargs["thread_ts"] == "123.456789"
        assert call.kwargs["channel"] == "C07UTQN7N1X"
        assert "<@UINCOMING>" in call.kwargs["text"]
    journal.load.return_value = copy.deepcopy(journal.save.call_args.args[0])
    assert runner.invoke(app, ["test-notify", "--run-id", "56789"]).exit_code == 0
    assert slack.chat_postMessage.call_count == 1
    assert slack.chat_scheduleMessage.call_count == 2


def test_accelerated_parent_cannot_target_production(schedule, slack):
    state = compose_handoff(schedule, FRIDAY, slack)
    with pytest.raises(RuntimeError, match="only use #sam-sandbox"):
        deliver_handoff(slack, state, Mock(), now=lambda: THURSDAY, sandbox=True)
    slack.chat_postMessage.assert_not_called()
    slack.chat_scheduleMessage.assert_not_called()
