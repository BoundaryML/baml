// Session creation has no task receiver. Options/token configure a driver whose
// exact input is the provider that will own the resulting Session resource.
class OpenSession {
  options: root.ai.SessionOptions,
}

implements root.ai.Driver<root.ai.Provider> for OpenSession {
  type Output = root.ai.Session
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, provider: root.ai.Provider) -> root.ai.Session
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.open_session(provider, self.options)
  }
}

function open_session(options: root.ai.SessionOptions) -> OpenSession throws never {
  OpenSession { options: options }
}

class ResumeSession {
  token: root.ai.SessionToken,
}

implements root.ai.Driver<root.ai.Provider> for ResumeSession {
  type Output = root.ai.Session
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, provider: root.ai.Provider) -> root.ai.Session
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.resume_session(provider, self.token)
  }
}

function resume_session(token: root.ai.SessionToken) -> ResumeSession throws never {
  ResumeSession { token: token }
}

class SaveTranscript {
  transcript: root.ai.Transcript,
}

implements root.ai.Driver<root.ai.Provider> for SaveTranscript {
  type Output = root.ai.TranscriptToken
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, provider: root.ai.Provider) -> root.ai.TranscriptToken
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.save_transcript(provider, self.transcript)
  }
}

function save_transcript(transcript: root.ai.Transcript) -> SaveTranscript throws never {
  SaveTranscript { transcript: transcript }
}

class RestoreTranscript {
  token: root.ai.TranscriptToken,
}

implements root.ai.Driver<root.ai.Provider> for RestoreTranscript {
  type Output = root.ai.Transcript
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, provider: root.ai.Provider) -> root.ai.Transcript
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.restore_transcript(provider, self.token)
  }
}

function restore_transcript(token: root.ai.TranscriptToken) -> RestoreTranscript throws never {
  RestoreTranscript { token: token }
}
