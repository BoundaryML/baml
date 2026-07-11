// Method-only so providers and applications can extend the event vocabulary
// through out-of-body implements without editing this union.
interface AgentEvent {
  function kind(self) -> string throws never
  function run_id(self) -> string? throws never { null }
  function provider_metadata(self) -> json? throws never { null }
}

class ModelStartedEvent {
  id: string?,
  provider: string,
  implements AgentEvent {
    function kind(self) -> string throws never { "model_started" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class TextDeltaEvent {
  id: string?,
  text: string,
  metadata: json?,
  implements AgentEvent {
    function kind(self) -> string throws never { "text_delta" }
    function run_id(self) -> string? throws never { self.id }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}

class ReasoningDeltaEvent {
  id: string?,
  summary: string?,
  redacted: bool,
  provider: string,
  metadata: json?,
  implements AgentEvent {
    function kind(self) -> string throws never { "reasoning_delta" }
    function run_id(self) -> string? throws never { self.id }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}

class ToolCallProposedEvent {
  id: string?,
  call: ToolCall,
  implements AgentEvent {
    function kind(self) -> string throws never { "tool_call_proposed" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class ToolCallStartedEvent {
  id: string?,
  call: ToolCall,
  implements AgentEvent {
    function kind(self) -> string throws never { "tool_call_started" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class ToolCallFinishedEvent {
  id: string?,
  call: ToolCall,
  result: ToolResult,
  implements AgentEvent {
    function kind(self) -> string throws never { "tool_call_finished" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class UsageUpdatedEvent {
  id: string?,
  usage: Usage,
  implements AgentEvent {
    function kind(self) -> string throws never { "usage_updated" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class TurnFinishedEvent {
  id: string?,
  turn: int,
  implements AgentEvent {
    function kind(self) -> string throws never { "turn_finished" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class RunFinishedEvent {
  id: string?,
  outcome: string,
  implements AgentEvent {
    function kind(self) -> string throws never { "run_finished" }
    function run_id(self) -> string? throws never { self.id }
  }
}

class RunFailedEvent {
  id: string?,
  message: string,
  implements AgentEvent {
    function kind(self) -> string throws never { "run_failed" }
    function run_id(self) -> string? throws never { self.id }
  }
}

interface AgentObserver {
  function on_event(self, event: AgentEvent) -> null throws never
}

interface AgentRecorder {
  function record(self, event: AgentEvent) -> null throws never
}

function publish_agent_event(
  event: AgentEvent,
  observers: AgentObserver[],
  recorders: AgentRecorder[],
) -> null throws never {
  for (let observer in observers) { observer.on_event(event); }
  for (let recorder in recorders) { recorder.record(event); }
  null
}
