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

Producing one is the `.background` modifier (page 3); consuming one is
matching on `poll`:

```baml
let job = DeepResearch.background(topic, baml.ai.BackgroundOptions {
  idempotency_key: "research-" + ticket_id,
})
defer { job.cleanup() }

let report = wait_for(job)

function wait_for(job: baml.ai.Job<Report>) -> Report
    throws baml.errors.BackgroundError | baml.errors.UnknownError {
  while (true) {
    match (job.poll()) {
      let d: baml.ai.Done<Report> => { return d.value; },
      let p: baml.ai.Pending      => baml.sys.sleep(p.retry_after ?? default_delay),
      let f: baml.ai.Failed       => throw f.error,
      let c: baml.ai.Cancelled    => throw baml.errors.UnknownError {
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
let job = DeepResearch.background(topic, opts)
db.save(ticket_id, baml.json.to_string(job.token()))

// process 2 (hours later): rehydrate on a CONFIGURED provider
let token: baml.ai.JobToken = baml.sap.parse<baml.ai.JobToken>(db.load(ticket_id))
let job = LongRunningModel.resume_job<Report>(token)
let report = wait_for(job)
```

Tokens are serializable and non-secret: an opaque provider-instance name
plus the minimal remote identifier. They never contain credentials — which
is why resumption is explicit, on a provider you configured, and the
provider validates ownership before use.

## Sessions

A provider-stored conversation is a resource that *executes requests* in its
context:

```baml
let session = SessionModel.open_session(baml.ai.SessionOptions {})
defer { session.cleanup() }

let greeting = session.run(Greet.request(name))
let followup = session.run(AskFollowup.request(question))
```

`session.run` takes any task's `.request` — the whole reason requests exist
as values. **The execution context wins, unconditionally**: `session.run(request)`
means `session.run(request.for_provider(session.provider()))`, always. The
request's own provider binding is just its default for context-free
execution; distinguishing "default" from "explicit override" would require
storing binding provenance in every request for the sake of an error message
— complexity without a matching benefit. If you handed a session the wrong
request, the re-rendered prompt is still exactly the task's prompt, run
where you said to run it.

Forking and compaction are session operations, because they mutate
provider-held state — but not every provider's sessions can do them, and
that difference is **static**, not a runtime surprise. Refinement interfaces
model it:

```baml
interface Session {
  function run<T>(self, request: Request<T>) -> T throws SessionError | UnknownError
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
function explore_both(session: baml.ai.ForkableSession) -> (Choice, Choice) {
  let alt = session.fork()
  let a = session.run(Choose.request("conservative"))
  let b = alt.run(Choose.request("experimental"))
  (a, b)
}

// dynamic negotiation — an existential Session may or may not fork:
match (session) {
  let f: baml.ai.ForkableSession => explore_both(f),
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
event ordering, and the interruption controls.

```baml
let live = baml.ai.open_live(
  VoiceAssistant.request(instructions, client = RealtimeModel),
  audio_channel,
)
defer { live.cleanup() }

for (let event in live.events()) {
  match (event) {
    let t: baml.ai.TranscriptDelta => ui.append(t.text),
    let barge: baml.ai.UserSpeechStarted => {
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
adapter, not the identity of the session.

## Managed caches

Provider-managed context caches are billed while alive; that lifecycle is
the resource:

```baml
let cache = CacheModel.create_cache(reference_material, baml.ai.CacheOptions {
  ttl: baml.time.Duration.from_hours(1),
})
defer { cache.cleanup() }

let answer = cache.run(AskReference.request(question))
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
baml.ai.run(req)          // no context
agent.run(req)            // tool-loop context  (page 5)
session.run(req)          // conversation context
cache.run(req)            // cached-prefix context
```

**`<context>.run(request)`** — one verb, learned once. Execution contexts
differ; the currency does not.

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

**`await`-shaped background** (make `.background` return a `Future<T>`).
Rejected: futures model in-process concurrency and die with the process; a
background job is remote, billable, pollable, and resumable. Where in-process
concurrency is wanted, `spawn { ExtractInvoice(doc) }` already exists and
composes — the two address different lifetimes.
