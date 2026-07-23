# 11. External Harnesses

An external harness owns a long-running model and tool loop. Coding agents are
the common example: the harness may read files, edit code, run commands, ask
for approval, and keep provider-specific continuation state.

Use a harness when BAML should submit typed work but should not own every tool
step. Use `run_agent` when the application wants BAML's agent driver to own the
loop and dispatch the tools itself.

## Core interface

```baml
interface Harness {
  function label(self) -> string throws never { "harness" }
  function open(self, options: HarnessOptions) -> HarnessSession
    throws baml.errors.UnknownError
  function run<T>(self, session: HarnessSession, task: Task<T>) -> HarnessRun<T>
    throws baml.errors.UnknownError
  function stream<TPartial, T>(self, session: HarnessSession, task: Task<T>)
    -> HarnessEventStream<T> throws baml.errors.UnknownError
  function save_session(self, session: HarnessSession) -> HarnessSessionToken
    throws baml.errors.UnknownError
  function restore_session(self, token: HarnessSessionToken) -> HarnessSession
    throws baml.errors.UnknownError
  function steer(self, session: HarnessSession, instruction: string) -> null
    throws baml.errors.UnknownError
  function interrupt(self, session: HarnessSession) -> null
    throws baml.errors.UnknownError
}
```

`submit_harness` is the short driver for opening a session, running one task,
and returning its typed result:

```baml
let run = ai.drivers.submit_harness(
  CodeHarness,
  FixRepository.task(issue),
  ai.HarnessOptions { cwd: "/workspace" },
)
let patch: Patch = run.value
```

## Sessions and streams are resources

```baml
interface HarnessSession requires Resource {
  function id(self) -> string throws never
  function conversation(self) -> Conversation throws never
  function stopped(self) -> bool throws never
}

interface HarnessEventStream<T> requires Resource {
  function next(self) -> AgentEvent | baml.stream.StreamFinished
    throws baml.errors.UnknownError
  function final(self) -> HarnessRun<T> throws baml.errors.UnknownError
}
```

The session and event stream own live state, so they require cleanup.
`HarnessRun<T>` is not a resource. It is the completed value plus the event
log, conversation projection, and resume token:

```baml
class HarnessRun<T> {
  value: T,
  events: AgentEvent[],
  token: HarnessSessionToken,
  conversation: Conversation,
}
```

## Save, steer, interrupt, and clean up

A session token can cross a process boundary. The session itself cannot. The
token must not contain API credentials.

Steering adds guidance to retained session state. Interruption asks the harness
to stop current work while keeping the session resumable. Cleanup permanently
releases the local session resource. These are separate operations.

## Permissions and adapter responsibilities

`HarnessOptions` carries working-directory, permission, sandbox, and tracing
configuration. The adapter must enforce the selected policy before it performs
host effects.

An adapter is also responsible for:

- preserving native event order and continuation IDs;
- validating the terminal payload as `T`;
- keeping credentials out of conversation data and session tokens;
- making cleanup idempotent; and
- reporting unsupported controls instead of silently ignoring them.

See the [external harness guide](../guide-external-harnesses/01-call-a-coding-harness.md)
for complete usage examples.
