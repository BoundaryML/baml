interface AgentEvent {
  function kind(self) -> string throws never
  function run_id(self) -> string? throws never { null }
  function provider_metadata(self) -> json throws never { null }
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

class ReasoningBlock {
    summary: string?,
    redacted: bool,
    provider: string,
    metadata: json,
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
