interface SessionProvider requires Provider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
    throws baml.errors.UnknownError
  function restore_transcript(self, token: TranscriptToken) -> Transcript
    throws baml.errors.UnknownError
}

class FakeSessionProvider {
  inner: Provider,
  saved: ProviderTranscript?,
}

implements Provider for FakeSessionProvider {
  function name(self) -> string throws never { "fake-session" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements SessionProvider for FakeSessionProvider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken throws never {
    let snapshot = ProviderTranscript {
      owner: self.inner,
      history: transcript.conversation(),
      sealed_state: { "saved": true },
    };
    self.saved = snapshot;
    TranscriptToken { provider: self.name(), version: 1, sealed: "fake-session-token" }
  }

  function restore_transcript(self, token: TranscriptToken) -> Transcript
      throws baml.errors.UnknownError {
    match (self.saved) {
      let transcript: ProviderTranscript => transcript,
      null => throw baml.errors.UnknownError { data: token, message: ["session token not found"] },
    }
  }
}

