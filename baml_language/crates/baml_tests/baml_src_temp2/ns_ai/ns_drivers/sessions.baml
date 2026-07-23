function save_conversation(
  provider: root.ai.Provider,
  conversation: root.ai.Conversation,
) -> root.ai.ConversationToken throws baml.errors.UnknownError | baml.errors.Unsupported {
  //# Require resumable conversation support
  match (provider) {
    let resumable: root.ai.ResumableToolCallingProvider => resumable.save_conversation(conversation),
    _ => throw baml.errors.Unsupported { message: "provider cannot save conversations" },
  }
}

function restore_conversation(
  provider: root.ai.Provider,
  token: root.ai.ConversationToken,
) -> root.ai.Conversation throws baml.errors.UnknownError | baml.errors.Unsupported {
  //# Require resumable conversation support
  match (provider) {
    let resumable: root.ai.ResumableToolCallingProvider => resumable.restore_conversation(token),
    _ => throw baml.errors.Unsupported { message: "provider cannot restore conversations" },
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
