interface AgentEvent {
  function kind(self) -> string throws never
  function run_id(self) -> string? throws never { null }
  function provider_metadata(self) -> json throws never { null }
  /// When the runtime emitted this event. Null for events that predate the
  /// telemetry floor; every event the Agent loop emits going forward
  /// carries a timestamp so observers can build spans.
  function emitted_at(self) -> baml.time.Instant? throws never { null }
}

class ModelStartedEvent {
    id: string?,
    provider: string,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "model_started"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

class ToolRosterChangedEvent {
    id: string?,
    step: int,
    tools: string[],
    implements AgentEvent {
        function kind(self) -> string throws never {
            "tool_roster_changed"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

class ProviderChangedEvent {
    id: string?,
    from_provider: string,
    to_provider: string,
    fidelity: ai.ConversationFidelity,
    warnings: string[],
    implements AgentEvent {
        function kind(self) -> string throws never {
            "provider_changed"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

class TextDeltaEvent {
    id: string?,
    text: string,
    metadata: json,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "text_delta"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
        function provider_metadata(self) -> json throws never {
            self.metadata
        }
    }
}

/// Visible assistant text produced alongside a step's outcome, when the
/// provider exposes it. The portable answer to "show the user what the
/// model said while it worked" — no more smuggling narration through
/// synthetic tool arguments.
class AssistantTextEvent {
    id: string?,
    step: int,
    text: string,
    at: baml.time.Instant,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "assistant_text"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
        function emitted_at(self) -> baml.time.Instant? throws never {
            self.at
        }
    }
}

/// Reasoning text the vendor exposes for display: Anthropic thinking text,
/// OpenAI reasoning summaries, Gemini thought summaries. Signed/encrypted
/// continuation blobs never appear here — they stay in the Conversation.
class ReasoningEvent {
    id: string?,
    step: int,
    text: string,
    at: baml.time.Instant,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "reasoning"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
        function emitted_at(self) -> baml.time.Instant? throws never {
            self.at
        }
    }
}

/// One event per completed provider step, carrying the step's OWN response
/// metadata (request id, model, finish reason — for multi-step runs these
/// were previously unrecoverable) and the step's usage delta. `UsageEvent`
/// stays cumulative; the delta lives here.
class StepFinishedEvent {
    id: string?,
    step: int,
    metadata: ai.ResponseMetadata,
    usage_delta: ai.Usage?,
    at: baml.time.Instant,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "step_finished"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
        function emitted_at(self) -> baml.time.Instant? throws never {
            self.at
        }
    }
}

class ToolCallEvent {
    id: string?,
    phase: string,
    call: ai.tools.ToolCall,
    result: ai.tools.ToolResult?,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "tool_call_" + self.phase
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

class UsageEvent {
    id: string?,
    usage: ai.Usage,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "usage_updated"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

class RunFinishedEvent {
    id: string?,
    outcome: string,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "run_finished"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

class RunInterruptedEvent {
    id: string?,
    provider: string,
    steps_taken: int,
    reason: string,
    implements AgentEvent {
        function kind(self) -> string throws never {
            "run_interrupted"
        }
        function run_id(self) -> string? throws never {
            self.id
        }
    }
}

interface AgentObserver {
  function on_event(self, event: AgentEvent) -> null throws never
}

interface AgentRecorder {
  function record(self, event: AgentEvent) -> null throws never
}

function _publish_agent_event(
    event: AgentEvent,
    observers: AgentObserver[],
    recorders: AgentRecorder[],
) -> null throws never {
    for (let observer in observers) {
        observer.on_event(event);
    }
    for (let recorder in recorders) {
        recorder.record(event);
    }
    null
}
