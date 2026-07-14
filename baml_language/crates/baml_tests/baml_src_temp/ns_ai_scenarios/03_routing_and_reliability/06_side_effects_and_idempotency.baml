test "guide 03.06: retry rejects a drive operation without replay evidence" {
  let effectful = EffectfulProvider { inner: fake_resolution() };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(effectful, 2)) catch (e) {
    let unsupported: baml.errors.Unsupported => "unsupported",
    _ => "unexpected",
  };
  assert.equal(label, "unsupported")
}

