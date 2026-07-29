class HarnessOptions {
    cwd: string?,
    permission_mode: string?,
    sandbox: string?,
    attributes: map<string, string>,
}

class HarnessSessionToken {
    provider: string,
    runtime: string,
    session_id: string,
    version: int,
    sealed: string,
}

interface HarnessSession requires root.ai.Resource {
  function id(self) -> string throws never
  function messages(self) -> root.ai.Messages throws never
  function stopped(self) -> bool throws never
}

class LocalHarnessSession {
    session_id: string,
    options: HarnessOptions,
    history: root.ai.MessageHistory,
    is_stopped: bool,
    is_cleaned: bool,
    steering: string[],
    cleanup_count: int,

    // function cleanup(self) -> void throws never {
    //   if (!self.is_cleaned) {
    //     self.is_cleaned = true;
    //     self.is_stopped = true;
    //     self.cleanup_count = self.cleanup_count + 1;
    //   }
    // }

    implements HarnessSession {
        function id(self) -> string throws never {
            self.session_id
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
        function stopped(self) -> bool throws never {
            self.is_stopped
        }
    }
    implements root.ai.Resource {}
}

class HarnessRun<T> {
    value: T,
    events: root.ai.observe.AgentEvent[],
    token: HarnessSessionToken,
    history: root.ai.MessageHistory,
}

function _emit_harness_event(
    event: root.ai.observe.AgentEvent,
    events: root.ai.observe.AgentEvent[],
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
) -> null throws never {
    //# Retain every event in the completed run
    events.push(event);
    //# Notify the per-run listener immediately
    match (on_event) {
        null => null,
        let callback: ((root.ai.observe.AgentEvent) -> null throws never) => callback(event),
    };
    null
}

interface Harness {
  function label(self) -> string throws never { "harness" }
  function open(self, options: HarnessOptions) -> HarnessSession
    throws baml.errors.UnknownError
  function run<T>(
    self,
    session: HarnessSession,
    task: root.ai.Task<T>,
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
  ) -> HarnessRun<T> throws baml.errors.UnknownError
  function save_session(self, session: HarnessSession) -> HarnessSessionToken
    throws baml.errors.UnknownError
  function restore_session(self, token: HarnessSessionToken) -> HarnessSession
    throws baml.errors.UnknownError
  function steer(self, session: HarnessSession, instruction: string) -> null
    throws baml.errors.UnknownError
  function interrupt(self, session: HarnessSession) -> null
    throws baml.errors.UnknownError
}
