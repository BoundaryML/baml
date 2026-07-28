// Continue later work inside a provider-owned session without manually
// reconstructing its conversation state.
function provider_owned_sessions(
) -> root.ai.ResponseWithMetadata<Resolution>[] throws root.ai.CallError
    | baml.errors.UnknownError
{
  let provider = root.ai.FakeSessionProvider {
    inner: fake_resolution(),
    sessions: [],
  };
  let opened = provider.open_session(root.ai.SessionOptions {
    session_id: "support-T-100",
    attributes: { "team": "support" },
  });
  let first = ResolveTicket_task(sample_ticket(), provider).run(
    runner = root.ai.run.InSession<Resolution>.new(opened),
  );

  let resumed = provider.resume_session(opened.token());
  let second = ResolveTicket_task(sample_ticket(), provider).run(
    runner = root.ai.run.InSession<Resolution>.new(resumed),
  );

  let responses = [first, second];
  log.info({
    "opened_session": opened.id(),
    "resumed_session": resumed.id(),
    "responses": responses.map((response) -> { response.value }),
  });
  responses
}
