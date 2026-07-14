function save_transcript(
  provider: root.ai.Provider,
  transcript: root.ai.Transcript,
) -> root.ai.TranscriptToken throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (provider) {
    let sessions: root.ai.SessionProvider => sessions.save_transcript(transcript),
    let resumable: root.ai.ResumableToolCallingProvider => resumable.save_transcript(transcript),
    _ => throw baml.errors.Unsupported { message: "provider cannot save transcripts" },
  }
}

function restore_transcript(
  provider: root.ai.Provider,
  token: root.ai.TranscriptToken,
) -> root.ai.Transcript throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (provider) {
    let sessions: root.ai.SessionProvider => sessions.restore_transcript(token),
    let resumable: root.ai.ResumableToolCallingProvider => resumable.restore_transcript(token),
    _ => throw baml.errors.Unsupported { message: "provider cannot restore transcripts" },
  }
}

