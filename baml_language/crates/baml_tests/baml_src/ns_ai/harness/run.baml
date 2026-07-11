class AgentTurn<T> {
  index: int,
  blocks: ModelBlock[],
  tool_calls: ToolCall[],
  tool_results: ToolResult[],
  value: T?,
  meta: Meta,
}

class AgentRun<T> {
  // Exactly one of these fields is populated. Keeping the generic Done<T>
  // outside an inline union prevents T from becoming a nested union in
  // provider parsers while preserving the same honest outcome set.
  done: Done<T>?,
  budget_reached: BudgetReached?,
  handoff: Handoff?,
  turns: AgentTurn<T>[],
  meta: Meta,
  session_token: HarnessSessionToken?,
  conversation: Conversation,
}

function agent_outcome_name<T>(run: AgentRun<T>) -> string throws never {
  if (run.done != null) { return "done"; }
  if (run.budget_reached != null) { return "budget_reached"; }
  if (run.handoff != null) { return "handoff"; }
  "invalid"
}

function agent_run_outcome<T>(run: AgentRun<T>) -> Done<T> | BudgetReached | Handoff throws never {
  match (run.done) {
    let done: Done<T> => return done,
    null => {},
  }
  match (run.budget_reached) {
    let budget: BudgetReached => return budget,
    null => {},
  }
  match (run.handoff) {
    let handoff: Handoff => return handoff,
    null => {},
  }
  baml.sys.panic("AgentRun has no outcome")
}

function agent_run_value<T>(run: AgentRun<T>) -> T throws baml.errors.UnknownError {
  match (run.done) {
    let done: Done<T> => return done.value,
    null => {},
  }
  match (run.budget_reached) {
    let budget: BudgetReached => throw baml.errors.UnknownError {
      data: budget,
      message: ["agent stopped because its budget was reached"],
    },
    null => {},
  }
  match (run.handoff) {
    let handoff: Handoff => throw baml.errors.UnknownError {
      data: handoff,
      message: ["agent requested handoff to " + handoff.to],
    },
    null => {},
  }
  baml.sys.panic("AgentRun has no outcome")
}

interface AgentEventStream<T> {
  function next(self) -> AgentEvent? throws baml.errors.UnknownError
  function final(self) -> AgentRun<T> throws baml.errors.UnknownError
  function cleanup(self) -> null throws never
}

// Reference in-memory implementation for adapters that receive an already
// buffered event list. Real transports implement AgentEventStream directly.
class BufferedAgentEventStream<T> {
  events: AgentEvent[],
  cursor: int,
  result: AgentRun<T>,

  implements AgentEventStream<T> {
    function next(self) -> AgentEvent? throws baml.errors.UnknownError {
      match (self.events.at(self.cursor)) {
        null => null,
        let event: AgentEvent => {
          self.cursor = self.cursor + 1;
          event
        },
      }
    }
    function final(self) -> AgentRun<T> throws baml.errors.UnknownError { self.result }
    function cleanup(self) -> null throws never { null }
  }
}

class AgentEventLog<T> {
  events: AgentEvent[],
  run: AgentRun<T>,
}

function collect_agent_stream<T>(stream: AgentEventStream<T>) -> AgentEventLog<T>
    throws baml.errors.UnknownError {
  let events: AgentEvent[] = [];
  while (true) {
    match (stream.next()) {
      null => { break; },
      let event: AgentEvent => events.push(event),
    }
  }
  let run = stream.final();
  stream.cleanup();
  AgentEventLog<T> { events: events, run: run }
}
