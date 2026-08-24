"""Unified `on_event` host coverage (spec: unified on_event across Agent,
direct calls, and streams — Host section).

Every generated LLM callable carries an injected optional `on_event=`
parameter that receives `baml_sdk.ai.events.*` values, fired synchronously
during the host's own call/pull. Two call paths are exercised here:

  * plain call — `on_event_probe` is bound to a scripted client (canned
    parseable reply + fixed usage) in the fixture, so the full Agent journal
    (RunStarted, AssistantMessage, Usage, FinalProduced) is deterministic and
    keyless.
  * stream — the keyless replay harness (see `replay_harness.py`); settle
    fires AssistantMessage then Usage exactly once, on either drain path.

A raising listener must never fail or alter the run — its error is swallowed
engine-side by `ai.events.guard`.
"""
from replay_harness import replay_server


def _raising_listener(_event):
    raise RuntimeError("listener boom")


# ---------------------------------------------------------------------------
# Plain (Agent-path) call.
# ---------------------------------------------------------------------------


# SDK_PARITY_LINT(skip): host on_event listener coverage lands Python-first; other SDKs port separately
def test_on_event_plain_call_delivers_events():
    import baml_sdk.ai.events as events
    from baml_sdk.lorem import Resume, on_event_probe

    seen = []
    result = on_event_probe("ignored-by-scripted-client", on_event=seen.append)
    assert isinstance(result, Resume)
    assert result.name == "Ada"
    # Journal order: one attempt, no tools, no repair re-asks.
    assert [type(e) for e in seen] == [
        events.RunStarted,
        events.AssistantMessage,
        events.Usage,
        events.FinalProduced,
    ]
    usage = seen[2]
    assert (usage.input_tokens, usage.output_tokens) == (3, 5)


# SDK_PARITY_LINT(skip): host on_event listener coverage lands Python-first; other SDKs port separately
def test_on_event_plain_call_raising_listener_does_not_fail():
    from baml_sdk.lorem import Resume, on_event_probe

    result = on_event_probe(
        "ignored-by-scripted-client", on_event=_raising_listener
    )
    assert isinstance(result, Resume)
    assert result.name == "Ada"


# ---------------------------------------------------------------------------
# Stream ($stream companion) against the replay fixture.
# ---------------------------------------------------------------------------


# SDK_PARITY_LINT(skip): host on_event listener coverage lands Python-first; other SDKs port separately
@replay_server(recording_path="replay_extract_string")
async def test_on_event_stream_delivers_settle_events():
    import baml_sdk.ai.events as events
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import stream_e2e_extract_stream_async

    seen = []
    stream = await stream_e2e_extract_stream_async(
        "ignored-by-replay-server", on_event=seen.append
    )
    while True:
        if isinstance(await stream.next_async(), Done):
            break
    # Extra pulls after Done must not re-emit settle events.
    assert isinstance(await stream.next_async(), Done)
    assert isinstance(await stream.final_async(), str)

    assert isinstance(seen[0], events.RunStarted)
    assistant = [e for e in seen if isinstance(e, events.AssistantMessage)]
    usage = [e for e in seen if isinstance(e, events.Usage)]
    assert len(assistant) == 1, f"expected one AssistantMessage, got {seen!r}"
    assert len(usage) == 1, f"expected one Usage, got {seen!r}"
    # Streams deliberately emit no FinalProduced (settle must not depend on
    # which drain path the host chose).
    assert not any(isinstance(e, events.FinalProduced) for e in seen)


# SDK_PARITY_LINT(skip): host on_event listener coverage lands Python-first; other SDKs port separately
@replay_server(recording_path="replay_extract_string")
def test_on_event_stream_raising_listener_does_not_fail():
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import stream_e2e_extract_stream

    stream = stream_e2e_extract_stream(
        "ignored-by-replay-server", on_event=_raising_listener
    )
    while True:
        if isinstance(stream.next(), Done):
            break
    assert isinstance(stream.final(), str)
