function save_transcript(
  provider: root.ai.Provider,
  transcript: root.ai.Transcript,
) -> root.ai.TranscriptToken throws baml.errors.UnknownError | baml.errors.Unsupported {
  //# Require resumable transcript support
  match (provider) {
    let resumable: root.ai.ResumableToolCallingProvider => resumable.save_transcript(transcript),
    _ => throw baml.errors.Unsupported { message: "provider cannot save transcripts" },
  }
}

function restore_transcript(
  provider: root.ai.Provider,
  token: root.ai.TranscriptToken,
) -> root.ai.Transcript throws baml.errors.UnknownError | baml.errors.Unsupported {
  //# Require resumable transcript support
  match (provider) {
    let resumable: root.ai.ResumableToolCallingProvider => resumable.restore_transcript(token),
    _ => throw baml.errors.Unsupported { message: "provider cannot restore transcripts" },
  }
}

function open_session(
  provider: root.ai.Provider,
  options: root.ai.SessionOptions,
) -> root.ai.Session throws baml.errors.UnknownError | baml.errors.Unsupported {
  //# Require provider-owned session support
  match (provider) {
    let sessions: root.ai.SessionProvider => sessions.open_session(options),
    _ => throw baml.errors.Unsupported { message: "provider cannot open sessions" },
  }
}

function resume_session(
  provider: root.ai.Provider,
  token: root.ai.SessionToken,
) -> root.ai.Session throws baml.errors.UnknownError | baml.errors.Unsupported {
  //# Require provider-owned session support
  match (provider) {
    let sessions: root.ai.SessionProvider => sessions.resume_session(token),
    _ => throw baml.errors.Unsupported { message: "provider cannot resume sessions" },
  }
}

function run_in_session<T>(
  session: root.ai.Session,
  task: root.ai.Task<T>,
) -> root.ai.Response<T> throws baml.errors.CallError | baml.errors.UnknownError {
  session.run<T>(task)
}
