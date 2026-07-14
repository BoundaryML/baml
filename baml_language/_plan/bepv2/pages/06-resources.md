# 6. Resources

Some operations create state that outlives the call: a job running on the
provider's servers, a stored conversation, an open audio socket, a billed
prompt cache. This page defines the rule for all of them: **an operation
whose follow-ups depend on provider-owned state returns a resource object**
— a value that owns the remote identifier, the provider, the parser, and
the lifecycle methods.

## Why not a bare handle

A `{ id, owner }` record forces every caller to reconstruct, from folklore:
which provider can use the id, how to poll or continue it, what the terminal
states are, whether cancel exists, whether cleanup is required, and how to
parse the eventual `T`. The provider implementation knew all of this at
creation time. A resource keeps that knowledge attached — and makes
wrong-provider misuse (polling an OpenAI job id against a fake) impossible
rather than a runtime owner-check.

## Background jobs

```baml
interface Job<T> {
  function poll(self) -> Done<T> | Pending | Failed | Cancelled
    throws baml.errors.BackgroundError | baml.errors.UnknownError
  function cancel(self) -> void throws baml.errors.BackgroundError | baml.errors.UnknownError
  function token(self) -> JobToken throws never
  function cleanup(self) -> void
}
```

Background operations get their own error channel — `BackgroundError`
(page 9) — because their failures have their own taxonomy: "the server
killed the job" (resubmit with the same key) is a different decision than
"polling timed out" (poll again), and neither is a `CallError`.

Producing one is the `drivers.submit_background` driver (page 3); consuming one is
matching on `poll`:

```baml
let job = ai.drivers.submit_background(DeepResearch.task(topic), ai.BackgroundOptions {
  idempotency_key: "research-" + ticket_id,
})
defer { job.cleanup() }

let report = wait_for(job)

function wait_for(job: ai.Job<Report>) -> Report
    throws baml.errors.BackgroundError | baml.errors.UnknownError {
  while (true) {
    match (job.poll()) {
      let d: ai.Done<Report> => { return d.value; },
      let p: ai.Pending      => baml.sys.sleep(p.retry_after ?? default_delay),
      let f: ai.Failed       => throw f.error,
      let c: ai.Cancelled    => throw baml.errors.UnknownError {
        data: c, message: ["job was cancelled"],
      },
    }
  }
}
```

The provider returns its own implementation — e.g. an OpenAI-Responses job
object holding the response id, the owning provider value, and the parser
for `Report`. You program against the `Job<T>` interface.

### Crossing processes: tokens

A resource is an in-process value; a web app must survive restarts. The two
jobs are different and get different types:

```baml
// process 1: submit, persist the claim
let job = ai.drivers.submit_background(DeepResearch.task(topic), opts)
db.save(ticket_id, baml.json.to_string(job.token()))

// process 2 (hours later): rehydrate on a CONFIGURED provider
let token: ai.JobToken = baml.sap.parse<ai.JobToken>(db.load(ticket_id))
let job = LongRunningModel.resume_job<Report>(token)
let report = wait_for(job)
```

Tokens are serializable and non-secret: an opaque provider-instance name
plus the minimal remote identifier. They never contain credentials — which
is why resumption is explicit, on a provider you configured, and the
provider validates ownership before use.

## Sessions

A provider-stored conversation is a resource that *executes tasks* in its
context:

```baml
let session = SessionModel.open_session(ai.SessionOptions {})
defer { session.cleanup() }

let greeting = ai.drivers.run_in_session(session, Greet.task(name))
let followup = ai.drivers.run_in_session(session, AskFollowup.task(question))
```

`run_in_session` takes any task's `.task` value. **The execution context wins,
unconditionally**: running a task in a session means executing
`task.with_provider(session.provider())`. The task's provider binding is its default for context-free
execution; distinguishing "default" from "explicit override" would require
storing binding provenance in every task for the sake of an error message
— complexity without a matching benefit. If you handed a session the wrong
task, the re-rendered prompt is still exactly the task's prompt, run
where you said to run it.

Forking and compaction are session operations, because they mutate
provider-held state — but not every provider's sessions can do them, and
that difference is **static**, not a runtime surprise. Refinement interfaces
model it:

```baml
interface Session {
  function run<T>(self, task: Task<T>) -> Response<T> throws SessionError | UnknownError
  function token(self) -> SessionToken throws never
  function cleanup(self) -> void
}

interface ForkableSession requires Session {
  function fork(self) -> ForkableSession throws SessionError | UnknownError
}

interface CompactableSession requires Session {
  function compact(self, policy: CompactionPolicy) -> CompactionResult
    throws SessionError | UnknownError
}
```

Code that *needs* forking demands it in its signature; code holding an
existential `Session` narrows first:

```baml
// static requirement — the type says what this function needs:
function explore_both(session: ai.ForkableSession) -> (Choice, Choice) {
  let alt = session.fork()
  let a = ai.drivers.run_in_session(session, Choose.task("conservative")).value
  let b = ai.drivers.run_in_session(alt, Choose.task("experimental")).value
  (a, b)
}

// dynamic negotiation — an existential Session may or may not fork:
match (session) {
  let f: ai.ForkableSession => explore_both(f),
  _ => run_sequentially(session),
}
```

A provider whose sessions cannot fork returns a plain `Session` and the
`match` falls through — no method that exists-but-throws. Contrast the
*application-owned* history case: if you send the full message array each
turn, there is no provider state and no session — history is ordinary data
and the task takes `ChatMessage[]` (page 1). Sessions are only for state
the *provider* holds.

`session.token()` / `SessionModel.resume_session(token)` follow the job
pattern for durable conversations.

## Realtime

A live connection is the most stateful resource: it owns the socket, the
event ordering, and the interruption controls. The intended entry point is an
explicit realtime driver, not a direct LLM-function call: the caller creates a
task value with `.task(...)`, supplies a `Channel`, and retains the returned
`Live` resource.

```baml
let live = ai.drivers.open_live(
  VoiceAssistant.task(instructions, $provider = RealtimeModel),
  audio_channel,
)
defer { live.cleanup() }

for (let event in live.events()) {
  match (event) {
    let t: ai.TranscriptDelta => ui.append(t.text),
    let barge: ai.UserSpeechStarted => {
      live.cancel_response()
      live.truncate_assistant_audio(barge.played_ms)
    },
    _ => {},
  }
}
```

`cancel_response` and `truncate_assistant_audio` are methods on the live
resource — they target *this* provider session, not a channel that might or
might not correspond to one. The channel remains what it is: an input/output
adapter, not the identity of the session. Returning `void` from a direct call
would not replace either object: it would define no completion boundary and
would leave the caller unable to drive or clean up the live interaction.

## Managed caches

Provider-managed context caches are billed while alive; that lifecycle is
the resource:

```baml
let cache = ai.drivers.create_cache(CacheModel, reference_material, ai.CacheOptions {
  ttl: baml.time.Duration.from_hours(1),
})
defer { cache.cleanup() }

let answer = cache.run(AskReference.task(question)).value
```

Implicit provider-side caching (automatic prefix reuse) is *not* a resource
— it has no lifecycle you own. It surfaces as response metadata, if at all.

## Cleanup discipline

Every resource implements the language's magic `cleanup()` (guaranteed
at-most-once), and acquisition sites pair it with `defer` so error paths
release too:

```baml
let session = SessionModel.open_session(opts)
defer { session.cleanup() }
// every exit — return, throw, break — runs the cleanup
```

For a resource that must outlive the scope (a job held across HTTP
requests), skip `defer`, persist the token, and make the *process* the
scope. `cleanup()`'s exact semantics per resource (release local state vs
cancel remote work) are part of each capability's contract; `cancel()` is
always explicit and separate.

## The uniform pattern

Notice the shape that has emerged across pages 5 and 6:

```baml
ai.drivers.drive(task)                // provider-default context
ai.drivers.run_agent(task)            // tool-loop context (page 5)
ai.drivers.run_in_session(s, task)    // conversation context
cache.run(task)                             // cached-prefix resource context
```

Drivers are the public lifecycle vocabulary. Resource methods remain available
to custom driver authors, but application code does not need to learn a second
calling convention.

## Alternatives considered

**Data handles + provider methods** (`provider.poll(job)`,
`provider.chat(prompt, session)`). Rejected: the caller must carry the
handle *and* the right provider *and* the lifecycle rules; wrong-owner
misuse becomes a runtime check in every method; discoverability dies (the
operations live on the provider, far from the thing they operate on).
Handles survive only in their honest role — as serializable tokens.

**Serializable resources** (make `Job<T>` itself the persistence format).
Rejected: a resource holds a provider value (credentials-adjacent) and a
parser; serializing that is either unsafe or lossy. Splitting
resource/token keeps in-process safety and cross-process explicitness.

**Channel-identified realtime** (`cancel(channel)` on the provider).
Rejected: control must target the provider session; a channel is plumbing
and can be reattached, multiplexed, or wrong.

**`await`-shaped background** (make `submit_background` return a `Future<T>`).
Rejected: futures model in-process concurrency and die with the process; a
background job is remote, billable, pollable, and resumable. Where in-process
concurrency is wanted, `spawn { ExtractInvoice(doc) }` already exists and
composes — the two address different lifetimes.
