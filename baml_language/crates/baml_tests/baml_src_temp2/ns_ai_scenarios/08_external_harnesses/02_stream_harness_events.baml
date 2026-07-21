test "guide 08.02: harness results preserve their event stream" {
  let harness = fake_model_harness();
  let run = InvestigateRepository_task(sample_ticket(), harness.inner).drive(
    root.ai.driver.harness<Resolution>(harness, harness_options()),
  );
  assert.equal(
    run.events.map((event: root.ai.AgentEvent) -> string { event.kind() }),
    ["model_started", "usage_updated", "run_finished"],
  )
}

testset "integ-test-guide-08-harness-events" {
  test "guide 08.02 live OpenAI: stream real partials and preserve the terminal event log" {
    let harness = live_openai_model_harness();
    let session = harness.open(harness_options());
    //# Open a harness-owned stream over the real provider stream
    let stream = harness.stream<Resolution$stream, Resolution>(
      session,
      InvestigateRepository_task(sample_ticket(), harness.inner),
    );
    let kinds: string[] = [];
    let partials = 0;
    let done = false;
    //# Consume normalized harness events without reconstructing continuation state
    while (!done) {
      match (stream.next()) {
        let event: root.ai.AgentEvent => {
          kinds.push(event.kind());
          if (event.kind() == "text_delta") { partials = partials + 1; }
        },
        let finished: baml.stream.StreamFinished => { done = true; },
      }
    }
    let run = stream.final();
    log.info(baml.json.stringify({
      "guide": "08.02",
      "event_kinds": kinds.join(","),
      "partial_count": partials,
      "terminal_event": run.events[run.events.length() - 1].kind(),
    }));
    assert.equal(kinds[0], "model_started")
    assert.is_true(partials > 0)
    assert.equal(run.events[run.events.length() - 1].kind(), "run_finished")
    assert.is_true(run.value.reply.length() > 0)
  }
}
