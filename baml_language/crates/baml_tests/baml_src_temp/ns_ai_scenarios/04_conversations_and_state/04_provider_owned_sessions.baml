test "guide 04.04: session tokens are opaque provider coordinates" {
  let provider = root.ai.FakeSessionProvider { inner: fake_resolution(), saved: null };
  let transcript = root.ai.ProviderTranscript {
    owner: provider.inner,
    history: sample_conversation(),
    sealed_state: { "signature": "provider-owned" },
  };
  let token = provider.save_transcript(transcript);
  let restored = provider.restore_transcript(token);
  assert.equal(restored.conversation().entries.length(), 2)
}

