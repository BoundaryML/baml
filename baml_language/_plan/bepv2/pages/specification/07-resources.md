# 7. Resources

Some operations leave state behind after the first request: a running job, a
stored conversation, an open audio socket, or a billed prompt cache. These
operations return a resource. The resource keeps the remote ID, owning
provider, parser, and lifecycle methods together.

## Why not a bare handle

A plain `{ id, owner }` record is not enough. Every caller would need to know
how to poll it, cancel it, clean it up, and parse its result. A resource keeps
those rules with the ID and prevents code from using it with the wrong
provider.

## Background jobs

```baml
interface Job<T> requires Resource {
  function status(self) -> JobStatus throws never
  function poll(self) -> Response<T>? throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
  function token(self) -> JobToken throws never
}
```

`poll()` returns the completed typed response or `null` while no result is
available. `status()` distinguishes pending, complete, failed, and cancelled
jobs. A failed poll throws and sets the status to `Failed`.

Producing one is the `drivers.submit_background` driver
([Drivers](./03-drivers.md)); consuming one is
matching on `poll`:

```baml
let job = ai.drivers.submit_background(DeepResearch.task(topic), ai.BackgroundOptions {
  idempotency_key: "research-" + ticket_id,
})
defer { job.cleanup() }

let report = wait_for(job)

function wait_for(job: ai.Job<Report>) -> Report
    throws baml.errors.UnknownError {
  while (true) {
    match (job.poll()) {
      let response: ai.Response<Report> => { return response.value; },
      null => match (job.status()) {
        ai.JobStatus.Pending => baml.sys.sleep(default_delay),
        ai.JobStatus.Cancelled => throw baml.errors.UnknownError {
          data: job.token(), message: ["job was cancelled"],
        },
        ai.JobStatus.Failed => throw baml.errors.UnknownError {
          data: job.token(), message: ["job failed"],
        },
        ai.JobStatus.Complete => baml.sys.sleep(default_delay),
      },
    }
  }
}
```

The provider returns its own implementation — e.g. an OpenAI-Responses job
object holding the response id, the owning provider value, and the parser
for `Report`. You program against the `Job<T>` interface.

## Batches

A batch groups several tasks of the same result type into one provider-owned
operation:

```baml
interface Batch<T> requires Resource {
  function status(self) -> JobStatus throws never
  function results(self) -> Response<T>[] throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
}

interface BatchProvider requires Provider {
  function submit_batch<T>(self, tasks: Task<T>[], options: BatchOptions)
    -> Batch<T> throws baml.errors.UnknownError
}
```

The explicit provider owns the batch. Keeping one `T` across the submitted
tasks lets `results()` stay typed. `cancel()` expresses the intent to stop
unfinished remote work; `cleanup()` releases the resource and remains
idempotent.

## Crossing processes: job tokens

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
  function cleanup(self) -> null throws never
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
and the task takes `ChatMessage[]` ([Tasks](./01-tasks-and-philosophy.md)).
Sessions are only for state
the *provider* holds.

`session.token()` / `SessionModel.resume_session(token)` follow the job
pattern for durable conversations.

## Realtime

A live connection is the most stateful resource: it owns the socket, the
event ordering, and the interruption controls. The intended entry point is an
explicit realtime driver, not a direct LLM-function call: the caller creates a
`Task<null>` with `.task(...)`, supplies a `Channel`, and retains the returned
`Live` resource. `null` is intentional: a live session has many observable
events, not one final application value.

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
adapter, not the identity of the session. Returning `null` from a direct call
would not replace either object: it would define no completion boundary and
would leave the caller unable to drive or clean up the live interaction.

If an application needs a typed value after the session, it runs a separate
typed task over the conversation it collected. A future bounded realtime
operation could instead return a distinct `LiveRun<T>` with an explicit
terminal result; the open-ended `Live` resource should remain non-generic.

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

Every concrete resource defines the language's magic direct-method shape:

```baml
function cleanup(self) -> null throws never
```

The runtime invokes that finalizer at most once after the resource becomes
unreachable and a garbage collection discovers it. Calling `cleanup()`
explicitly consumes the same at-most-once latch, so a later GC finalization is
a no-op. The method must be non-throwing because it may run outside the
application's ordinary control flow.

GC finalization is a safety net, not a deterministic scope-exit guarantee.
Production acquisition sites that need prompt release pair the resource with
`defer`:

```baml
let session = SessionModel.open_session(opts)
defer { session.cleanup() }
// every exit — return, throw, break — runs the cleanup
```

Tests and runtime diagnostics may force the finalization boundary explicitly:

```baml
create_and_abandon_resource(audit_log)
baml.sys.collect_garbage()
assert.equal(audit_log, ["cleaned"])
```

`baml.sys.collect_garbage() -> null throws never` performs a full collection
and drains queued `cleanup()` finalizers before returning. It does not finalize
reachable values, and normal production lifecycle code should not use it as a
substitute for `defer` or an explicit `cleanup()` call.

For a resource that must outlive the scope (a job held across HTTP
requests), skip `defer`, persist the token, and transfer cleanup ownership to
the longer-lived component. Do not rely on process teardown to run a finalizer.
`cleanup()`'s exact semantics per resource (release local state vs cancel
remote work) are part of each capability's contract; `cancel()` is always
explicit and separate.

## The uniform pattern

Notice the same shape across agent and resource lifecycles:

```baml
ai.drivers.drive(task)                // provider-default context
ai.drivers.run_agent(task)            // tool-loop context
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
