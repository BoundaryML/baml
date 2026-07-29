interface Channel {
  function send(self, frame: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
}

interface LiveEvent {
  function kind(self) -> string throws never
}

class TranscriptDelta {
    text: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "transcript_delta"
        }
    }
}

class UserTranscript {
    item_id: string,
    text: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_transcript"
        }
    }
}

class UserTranscriptFailed {
    item_id: string,
    message: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_transcript_failed"
        }
    }
}

class UserSpeechStarted {
    item_id: string,
    audio_start_ms: int,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_speech_started"
        }
    }
}

class UserSpeechStopped {
    item_id: string,
    audio_end_ms: int,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_speech_stopped"
        }
    }
}

class AssistantAudioStarted {
    item_id: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "assistant_audio_started"
        }
    }
}

class AssistantAudioDelta {
    audio: uint8array,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "assistant_audio_delta"
        }
    }
}

class LiveToolCalls {
    calls: root.ai.tools.ToolCall[],
    implements LiveEvent {
        function kind(self) -> string throws never {
            "tool_calls"
        }
    }
}

class LiveToolResults {
    results: root.ai.tools.ToolResult[],
    implements LiveEvent {
        function kind(self) -> string throws never {
            "tool_results"
        }
    }
}

class LiveTurnCompleted {
    implements LiveEvent {
        function kind(self) -> string throws never {
            "turn_completed"
        }
    }
}

class LiveClosed {
    implements LiveEvent {
        function kind(self) -> string throws never {
            "closed"
        }
    }
}

interface LiveSession requires root.ai.Resource {
  function receive(self) -> LiveEvent[] throws baml.errors.UnknownError
  function send_text(self, text: string) -> null throws baml.errors.UnknownError
  function input_audio_format(self) -> RealtimeAudioFormat throws never
  function output_audio_format(self) -> RealtimeAudioFormat throws never
  // Append one microphone frame without ending the user's turn. This is the
  // normal path for continuous input with provider-side VAD. Bytes use the
  // format reported by input_audio_format().
  function send_audio(self, audio: uint8array) -> null
    throws baml.errors.UnknownError
  // End a manually-delimited audio turn. Do not call this when server VAD is
  // configured: the provider commits speech at its detected boundary.
  function commit_audio(self) -> null throws baml.errors.UnknownError
  // Convenience for bounded clips; equivalent to clear + send_audio + commit.
  function send_audio_turn(self, audio: uint8array) -> null
    throws baml.errors.UnknownError
  function submit_tool_results(self, results: root.ai.tools.ToolResult[]) -> null throws baml.errors.UnknownError
  // Start a response for the committed conversation state. This is used with
  // server VAD when create_response=false so a runner can own turn policy.
  function request_response(self) -> null throws baml.errors.UnknownError
  function cancel_response(self) -> null throws baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError
  function close(self) -> null throws never
}

// Runner-owned policy wrapper for ordinary function-backed task tools. The
// provider session remains responsible for wire correlation and transcript
// mutation; this wrapper only dispatches calls and submits their results.
