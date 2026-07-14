interface ToolCallingProvider requires GenerationProvider {
  function begin<T>(self, task: Task<T>) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, transcript: Transcript, tools: Tool[]) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError
  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
}

interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
    throws baml.errors.UnknownError
  function restore_transcript(self, token: TranscriptToken) -> Transcript
    throws baml.errors.UnknownError
}

interface TranscriptImportProvider requires ToolCallingProvider {
  function import_conversation(self, conversation: Conversation) -> TranscriptImport
    throws baml.errors.UnknownError
}

