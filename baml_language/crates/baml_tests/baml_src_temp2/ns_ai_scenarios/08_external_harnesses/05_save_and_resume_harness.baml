test "guide 08.05: harness sessions save and resume with opaque tokens" {
  let harness = fake_model_harness();
  let session = harness.open(harness_options());
  let token = harness.save_session(session);
  let restored = harness.restore_session(token);
  assert.equal(restored.id(), session.id())
  session.cleanup();
  session.cleanup();
  match (session) {
    let local: root.ai.LocalHarnessSession => assert.equal(local.cleanup_count, 1),
    _ => assert.is_true(false),
  }
}


testset "integ-test-guide-08-harness-resume" {
  test "guide 08.05 live OpenAI: serialize a token, restore ownership, and continue" {
    let harness = live_openai_model_harness();
    let session = harness.open(harness_options());
    let first = harness.run<Resolution>(
      session,
      InvestigateRepository_task(sample_ticket(), harness.inner),
    );
    //# Cross a request-shaped serialization boundary with no credentials
    let encoded = baml.json.to_string(first.token) catch (e) {
      _ => baml.sys.panic("harness token serialization failed"),
    };
    let token = baml.sap.parse<root.ai.HarnessSessionToken>(encoded) catch (e) {
      _ => baml.sys.panic("harness token parse failed"),
    };
    let restored = harness.restore_session(token);
    assert.equal(restored.id(), session.id())
    harness.steer(restored, "The Resolution.reply field must include the exact token RESUMED-8505.");
    //# Continue through the restored provider-owned session
    let second = harness.run<Resolution>(
      restored,
      InvestigateRepository_task(sample_ticket(), harness.inner),
    );
    log.info(baml.json.stringify({
      "guide": "08.05",
      "session_id": restored.id(),
      "token_bytes": encoded.length(),
      "contains_credentials": encoded.includes("OPENAI_API_KEY") || encoded.includes("sk-"),
      "continued": second.value.reply.includes("RESUMED-8505"),
    }));
    assert.is_true(!encoded.includes("OPENAI_API_KEY"))
    assert.is_true(!encoded.includes("sk-"))
    assert.contains(second.value.reply, "RESUMED-8505")
  }
}
