# Gap analysis — where the model strains across the landscape

A stand-alone, critical read of the proposed LLM-interface redesign against 47 worked scenarios (a single-turn text call through realtime voice, harnesses, and durable workflows). It names the gaps that **recur** — the same shape of problem showing up across many scenarios — because those, not the one-off quirks, are what the design must answer. Every scenario is *expressible* with additions; none is clean, none is impossible. The interesting content is *which additions, and which gaps no addition fixes*.

> **Reading note.** This doc is self-contained — §0 summarizes the model so you need not open anything
> else. For full detail: the proposal is [`../01-providers-clients-capabilities.md`]; the three resolved
> sub-models are [`../error-model.md`], [`../value-sidecar-model.md`], [`../provider-as-marker.md`];
> shared vocabulary and the real stdlib mapping live in [`_conventions.md`]. Scenario citations like
> "**31**" point to `scenarios/31-*/`. (The prior, changelog-style version is `_gap-analysis-outdated.md`.)

---

## §0 — The model in one screen

- **`Provider` is a bare marker.** It carries no interaction method. *Capabilities* are interfaces that `requires Provider`, and each owns its interaction: **`HttpProvider`** owns request/response (`call`, `call_with`), **`Streaming`** owns `stream`, **`Realtime`** owns `run`, **`Tools`** owns the multi-turn `begin`/`step`/`submit` loop. A provider class implements the marker plus whatever it can do; **the capability set *is* its type**. There is no degenerate `call` forced onto realtime/harness.
- **A `client` is sugar for a function returning a `Provider`** — the existential. Clients compose, take params, select dynamically, and chain combinators because they are ordinary functions.
- **Combinators** (`Fallback`, `Retry`, `Cache`, …) are plain classes that forward each capability by a runtime `match` over their members.
- **Value + metadata.** `HttpProvider.call_with<T,U>(prompt, project: (ResponseMeta) -> U) -> (T, U)` returns the answer *and* a projected sidecar (usage / logprobs / citations / timing); `call` derives from it. `ResponseMeta` is an interface (lazy-normalized per provider); `Supported<T> = T | Unsupported` marks "provider can't" vs "empty this call". **Sum outcomes do not ride `call_with`:** refusal → the `throws` channel; suspend / tool-calls → a sentinel return `T | X`; background/batch → a `submit -> Job<T>`.
- **Errors** are one interface per capability (`CallError` / `StreamError` / `ToolError` / `RealtimeError`, plus net-new `<Cap>Error`), on the channel `E | UnknownError`. Concrete errors `implement` the interface; cross-capability errors stay typed via Rust-like *external* trait impls.
- **Concurrency is shipped** (BEP-034: `spawn`/`await`/`Future`, `baml.future.{all,race,any}`, `TaskGroup`, `CancelToken`). The **stdlib is real** (strings/lists/maps are methods; `baml.json`, etc.).

This spine is genuinely strong for **stateless request/response** and most **tool loops**. It strains the moment a capability is **stateful, server-authoritative, effectful, or non-idempotent** — and at the seam where the **existential client** meets a need for a **static** guarantee. The gaps below are those two fault lines, refracted through the scenarios.

---

## §1 — The recurring gaps

Each is tagged **[fatal]** (cannot be expressed / silently unsound), **[workable]** (expressible but leaks), or **[cosmetic]** (ergonomic only); and as **model-shape** (the design must change), **host-surface** (just needs a primitive, §2), or **inherent** (a real-world hardness no model removes).

### Family A — The per-call, value-oriented model has no home for state

**A1. Server-authoritative / durable state is unexpressible.** *[fatal · model-shape · 19 scenarios: 03, 07, 17–21, 27–29, 31, 34, 36, 37, 40–42, 44, 46]* Conversation handles, server-stored chains, fork cursors, compiled-grammar caches, background jobs, cache resources, warm sockets, durable sessions, memory tiers, and load-balancer cursors are **mutable cross-call state the server owns** — but the model is value-oriented and per-call, so state is smuggled through opaque `$rust_type` fields, stateless combinators, or side-arguments the app must keep aligned, with **no lifecycle** (RAII / TTL / compare-and-set / eviction).
- **31:** Gemini's `cachedContents` handle outlives any call and is **billed while idle**, but every capability is per-call with no finalizer — a handle that must survive across HTTP requests in a long-lived server is simply unexpressible.

  ```baml
  interface ManagedCache requires Provider {
    // Create the resource (a separate API call). Returns the addressable handle.
    function create_cache(self, prefix: baml.llm.PromptAst, ttl_secs: int) -> Handle ...

    // Delete the resource — stops the storage bill. Caller MUST eventually call
    // this or leak money. The proposal has no destructor/RAII, so this is manual.
    function delete_cache(self, handle: Handle) -> null ...   // <-- nothing frees the handle across calls; billed while idle
  }

  class CacheHandle { name: string, _data: $rust_type }   // "cachedContents/abc123", outlives any single call
  ```

- **17:** a client is config-only sugar, so conversation identity (`Session`) can never ride on it — a stateful chat is permanently a `client + Session` pair the app threads to every `.chat` call.

  ```baml
  function Ask$chat(text: string, session: Session) -> string   // <-- client + Session are separate args
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let p = Terse();                  // client = config-only sugar; Session can't ride on it
    match (p) {
      let c: Conversational => c.chat<string>(render_prompt(text), session),  // <-- Session threaded by hand, must match p
      _ => throw baml.errors.Unsupported { message: "client cannot hold a conversation" },
    }
  }

  // every chat threads the Session argument separately from the client:
  let _a = Ask$chat("What's the capital of France?", s);
  let b  = Ask$chat("And its population?", s);   // <-- same s re-passed every turn
  ```


**A2. A combinator over-claims a capability it cannot honestly forward.** *[fatal · model-shape · 21 scenarios: 02, 03, 06, 08–11, 13, 14, 18–20, 22, 24, 25, 27–29, 31, 42, 45]* Generic combinators forward *every* capability by runtime `match`, but for **stateful / effectful / non-idempotent / one-shot** capabilities that forwarding is silently wrong, and the type system permits it.
- **27:** `BigAnalysis().with_retry(3)` **double-submits** a billed multi-minute server job, because `Retry` forwards `submit`.

  ```baml
  // implement.baml — submit fires a billed multi-minute create, returns an id at once
  function submit<T>(self, prompt: baml.llm.PromptAst, idem: string) -> Job<T> {
    let req = self.build_responses<T>(prompt, true, idem);
    let resp = baml.http.send(req);   // creates a server-side, BILLED job
    ...
  }
  // wait_for (Background default) submits, then polls until terminal
  function wait_for<T>(self, prompt: ..., cadence: Cadence) -> T {
    let job: Job<T> = self.submit<T>(prompt, baml.id.new());  // fresh key each call
    ...
  }

  // usage seam — Retry is inherited from Provider and forwards the blocking wait_for:
  BigAnalysis().with_retry(3).wait_for<ArchReview>(prompt, cadence)  // <-- retry re-runs wait_for -> submit() again = 2nd billed job
  ```

- **10:** `Bounded{…}.with_retry(2)` — `Bounded` has no single-turn call to retry, so `Retry` replays the **entire** multi-turn `run_tools` loop, re-dispatching every tool and re-charging side effects.

  ```baml
  class Bounded {
    inner: Tools
    ...
    implements Tools {
      function run_tools<T>(self, prompt: ..., tools: Tool[], ctx: ExecutionContext) -> T ... {
        ...
        while (true) { ... self.inner.step<T>(bt.inner) ... }   // the multi-turn loop
      }
    }
  }

  // usage.baml:
  client ResilientBounded() {
    Bounded { inner: OpenAITools { ... }, stop_when: step_count_is(10), per_turn_tools: null }
      .with_retry(2)   // <-- retries the WHOLE loop, not a single op
  }
  ```

- **22:** `ResilientVoice()` compiles, and `Fallback.run` re-drives the second phone provider **after the first already streamed audio out** and mutated server state — there is no way to mark a provider non-retryable/effectful.

  ```baml
  class Fallback {
    strategy: Provider[]
    implements Realtime  { /* route .run to a realtime member */ } // <-- re-drives a 2nd provider after the 1st streamed audio out
  }

  client ResilientVoice() {
    PhoneAssistant().fallback_to(GeminiPhone())     // first realtime member wins .live
  }
  function VoiceHA(system: string) -> Transcript { client: ResilientVoice() ... }
  // VoiceHA.live("...", io): if PhoneAssistant's connect fails, Fallback re-tries
  // Gemini — but audio already streamed over `io` is gone; channel state is not
  // part of the provider's retryable value. // <-- reconnect-as-retry replays external effects
  ```


**A3. Channel / stream / session lifecycle and ordering is by-convention, not enforced.** *[fatal · model-shape · 7 scenarios: 04, 22, 23, 24, 32, 37, 46]* For realtime/harness/streaming, the `Channel` and `Stream` are stateful objects whose method ordering is pure convention — nothing forces a provider author to consult the cursor, serialize continuations, or close the span, so a **naive-but-compiling** impl ships the race.
- **37:** `DriveBuild` issues every `io.send` *before* the `run` that spawns the subprocess — the whole control sequence is enqueued against a session that doesn't exist yet; the model never says *when* a `Channel` becomes live relative to `run`.

  ```baml
  let io = GuardedChannel.negotiate(raw_io, r);
  io.on(ev => { ... });

  // Drive it. Each of these is a MESSAGE, not a method call on the client.
  io.send(Steer { text: "Use the existing logger, don't add a new one." });
  io.send(FollowUp { text: "When green, open a PR." });
  io.send(SetPermMode { mode: PermissionMode.Default });
  io.send(Interrupt {});
  io.send(RewindFiles { to_user_message_id: "msg_3", dry_run: false });
  io.send(EndSession {});

  r.run(render_prompt(spec), io)   // <-- subprocess spawned here, AFTER every control verb was enqueued against a session that does not exist yet
  ```

- **23:** nothing forces a provider to consult `cursor.on_audio` before playing, so a naive `run()` ships the barge-in race — the user hears audio they were supposed to never hear.

  ```baml
  function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript ... {
    let cursor = PlaybackCursor.fresh();   // tracks interrupting + played_ms
    io.on(ev => {
      match (ev.kind) {
        InKind.Audio => {
          // naive: play every delta directly, never routing through the cursor.
          baml.audio.play_and_measure(match (ev.audio) { let a: string => a, null => "" }); // <-- ... no cursor.on_audio / interrupting check: late audio plays
        },
        InKind.SpeechStarted => { self.barge_in(io, ...) catch (e) { _ => {} }; },
        _ => {},
      }
    });
    baml.realtime.await_transcript(io)
  }
  ```


### Family B — The existential `client` makes every capability a runtime promise

**B1. Capability membership is present-or-absent at runtime — not provable, not graded.** *[workable · model-shape · 22 scenarios: 01–03, 05–09, 12, 13, 21, 22, 24, 26, 30, 33, 34, 36, 40, 41, 43, 45]* Because a client is the existential `Provider`, "can this client stream / run tools / structured-output *this* `T` / score / decode-constrained / batch?" is only a runtime `match` that throws `Unsupported`. Worse, `implements` is **binary**: a provider can satisfy an interface while its impl is a stub or a degraded lie, so the test checks **presence, not quality**. No signature can demand "any provider, *but it must do X for this T*."
- **03:** `Classify.decode` against a non-`Constrained` hosted client **compiles** and only throws at runtime — the by-construction grammar guarantee is invisible to the type checker.

  ```baml
  function Classify$decode(text: string, constraint: Constraint) -> Sentiment
      throws baml.ExtendUnknownError<baml.errors.ConstraintError> {
    let p = Hosted();                 // <-- non-Constrained client wired in; still compiles
    match (p) {
      let c: Constrained => c.decode<Sentiment>(render_prompt(text), constraint),
      _                  => throw Unsatisfiable {   // <-- only throws at RUNTIME, no compile error
        message: "client cannot guarantee shapes; pick a self-hosted provider",
        constraint: constraint,
      },
    }
  }

  client Hosted() {
    OpenAIHosted { model: "gpt-4o", api_key: env.OPENAI_API_KEY }  // no `implements Constrained`
  }
  ```

- **30:** Anthropic *implements* `ConfidenceProvider` but returns `confidence: 1.0` — so `escalate_to` compiles, reads identically to the calibrated cascade, and **silently never escalates**. Membership tests presence, not calibration.

  ```baml
  class Anthropic {
    implements ConfidenceProvider {
      function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T> ... {
        let value: T = self.call<T>(prompt);
        Scored<T> { value: value, confidence: 1.0, source: "none" }   // <-- always 1.0
      }
    }
  }

  class Cascade {
    implements HttpProvider {
      function call<T>(self, prompt: baml.llm.PromptAst) -> T ... {
        match (self.cheap) {
          let c: ConfidenceProvider => {
            let s: Scored<T> = c.call_scored<T>(prompt);
            if (s.confidence >= self.threshold) { return s.value; }   // <-- 1.0>=0.72 -> never escalates
            ...
  }
  ```


**B2. Shallow portability: identical interface and type, divergent meaning or economics.** *[fatal · **inherent** · 18 scenarios: 01, 03, 06–08, 11, 13, 17, 22–25, 28, 30, 34, 36, 40, 44]* Two providers satisfy the same interface and type-check identically, but the value's **meaning, cost, cache economics, latency, durability, or side-effect** differ silently, with no channel to surface it. This is the one family the model *cannot* fix by adding surface — the types are honestly equal; the world is not.
- **13:** both providers implement `SearchableTools` identically, but one busts the prompt-cache prefix on every search and the other doesn't — the **whole economic reason the feature exists** is silently provider-dependent.

  ```baml
  // OpenAI: deferred schemas flagged but never enter the leading prompt -> prefix stays byte-stable
  function encode_tools_searchable(catalog: Catalog) -> string ... {
    entries.push(baml.llm.openai_tool_search_entry());   // { "type": "tool_search" }
    ...
    baml.llm.openai_tool_entry_deferrable(qualified, ct.tool.description, schema, ct.defer)  // defer_loading: provider pages in -> cache prefix unchanged
    ...
  }

  // Anthropic: search loads schemas by MUTATING wire_tools for the next turn
  function submit_impl(t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript ... {
    let with_loaded = Anthropic.splice_loaded_schemas(t, results);  // <-- splices loaded schemas into the leading tool block => prompt-cache prefix INVALIDATED every search
    ...
  }
  ```

- **40:** `ResumeAt` against the same value is **destructive in-place truncation** on one harness and a **non-destructive branch** on another — the unified interface unifies the call but not its observable effect.

  ```baml
  type Pickup = Fresh | Continue | Resume | ResumeAt | Fork

  // ClaudeCode.resolve_pickup:
  let ra: ResumeAt => {
    // keep the SAME id (history after that point is discarded on next save).
    truncate_session_at(dir, ra.session_id, ra.message_id);   // <-- destructive in-place truncation, same id
    return ra.session_id;
  },

  // PiAgent.resolve_pickup:
  let ra: ResumeAt => {
    // there is no in-place truncation. ResumeAt collapses to Fork-without-copy.
    return pi_tree_branch_at(self._tree, ra.session_id, ra.message_id);   // <-- non-destructive new branch leaf
  },
  ```

- **01:** `ResponseMeta.finish_reason` is a bare unnormalized string while the three providers emit three disjoint vocabularies — the interface promises a normalized dimension the type doesn't deliver.

  ```baml
  interface ResponseMeta {
    function finish_reason(self) -> string   // <-- bare string, no normalized vocabulary
  }

  // Anthropic
  function finish_reason(self) -> string {
    let b = baml.json.from_json<AnthropicBodyWire>(baml.json.parse(self.raw.text()));
    match (b.stop_reason) { let s: string => s, _ => "" }   // <-- raw "end_turn"/"refusal"
  }
  // OpenAI
  function finish_reason(self) -> string {
    let b = baml.json.from_json<OpenAiBodyWire>(baml.json.parse(self.raw.text()));
    match (b.choices.at(0)) { let c: OpenAiChoiceWire => match (c.finish_reason) { let r: string => r, _ => "" }, _ => "" }   // <-- raw "stop"
  }
  // Gemini
  function finish_reason(self) -> string {
    let b = baml.json.from_json<GeminiBodyWire>(baml.json.parse(self.raw.text()));
    match (b.candidates.at(0)) { let c: GeminiCandidateWire => match (c.finishReason) { let r: string => r, _ => "" }, _ => "" }   // <-- raw "STOP"
  }
  ```


### Family C — The `(T, U)` sidecar and the single control channel are too narrow

**C1. Aggregate / provenance loss: the sidecar projects one winner, not the chain.** *[workable · model-shape · 18 scenarios: 01, 04–11, 14, 21, 30, 32–34, 41, 46, 47]* `call_with`'s projection runs over exactly **one** `ResponseMeta` — the fallback winner, the last turn, the top-level provider — so tokens, cost, logprobs, and timing burned on **failed-then-retried members, multi-turn loops, sub-calls (STT, judge, guard, summarizer, embedder), and fan-out branches** have no slot. The model cannot express "project over the chain."
- **05:** the value+sidecar fix forwards `call_with` so chat usage survives, but `fold_audio_to_text` calls the STT provider's `call` (not `call_with`), so its tokens/cost/latency are **silently dropped**.

  ```baml
  function fold_audio_to_text(self, prompt: baml.llm.PromptAst) -> baml.llm.PromptAst
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let view = baml.media.view(prompt);
    let extra = "";
    for (let part in view.media) {
      if (part.ref.kind == audio) {
        let text: string = match (self.stt) {
          let h: HttpProvider => h.call<string>(baml.media.audio_only_prompt(part.ref)),  // <-- ... uses call, NOT call_with: STT provider usage/cost silently dropped
          _ => { throw baml.errors.Unsupported { ... }; },
        };
        extra = extra + "\n[audio transcript: " + text + "]";
      }
    }
    ...
  }
  ```

- **47:** `WorkflowProvider.call_with` hardcodes rolled-up usage to all-zero — the promise that workflow-as-tool tokens roll up to the calling turn is **unimplementable** at the tool boundary.

  ```baml
  function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let input: Workflow.Input = baml.llm.tool_args<Workflow.Input>(prompt) catch (...) { ... };
    let out: Workflow.Output = self.wf.run(input, self.ctx.child(self.wf_key()));
    let value: T = baml.cast.checked<T>(out) catch (...) { ... };
    (value, project(DurableMeta {
      rolled_usage: Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, // <-- hardcoded all-zero Usage
                            cache_write_tokens: 0, reasoning_tokens: 0 },
      was_replayed: false,
    }))
  }
  ```


**C2. A non-error control outcome is forced through the error or sentinel channel.** *[fatal · model-shape · 7 scenarios: 02, 10, 11, 12, 14, 16, 27]* A sum outcome that is **not** an error — a budget-hit partial, a handoff, structured-output-as-tool, an in-band tool failure — is forced through `throws`, the single `T | ToolCalls` arm, or a return type the combinator can't specialize.
- **10:** `run_tools` is frozen at `-> T`, so a budget-hit partial can only escape by `throw BudgetHit`, forcing `BudgetHit` to `implement ToolError` with vacuously-false classifiers — any app catching `ToolError` swallows a budget cap as a transport failure.

  ```baml
  function run_tools<T>(self, ...) -> T throws baml.ExtendUnknownError<baml.errors.ToolError> {
    ...
    if (self.stop_when(info)) {
      throw BudgetHit { partial_json: ..., steps_taken: step_no, history: ... };  // <-- partial escapes only via throw
    }
    ...
  }

  class BudgetHit {
    partial_json: string?
    ...
    implements baml.errors.ToolError {           // <-- a control signal forced to be a ToolError
      function is_network_error(self) -> bool { return false; }  // vacuously false
      function is_rate_limit(self)    -> bool { return false; }
      function is_parse_error(self)   -> bool { return false; }
    }
  }
  ```

- **16:** `Quarantined.call` is forced to keep `HttpProvider`'s fixed `-> T`, so the one place the model could re-type a quarantined model's output as `Tainted<T>` is blocked — **silent laundering is a one-line omission.**

  ```baml
  class Quarantined {
    inner: Provider
    implements HttpProvider {
      type Body = unknown
      // "Whatever the quarantined model returns is born TAINTED... We can't change
      //  call's return type generically, so the obligation is on the CALLER to wrap."
      function call<T>(self, prompt: baml.llm.PromptAst) -> T  // <-- fixed return T; can't re-type as Tainted<T>
          throws baml.ExtendUnknownError<baml.errors.CallError> {
        match (self.inner) {
          let h: HttpProvider => { return h.call<T>(prompt); },  // <-- untrusted output escapes as plain T
          _ => { throw baml.errors.Unsupported { message: "quarantined inner cannot produce a value" }; }
        }
      }
    }
  }
  ```


### Family D — Typed structure is erased exactly where it is needed

**D1. Untyped seam: tool args / output / carry / state collapse to `unknown` where typing matters.** *[workable · model-shape · 15 scenarios: 02, 09, 11, 13, 14, 16, 24, 37–39, 43–47]* At the boundary where the app's handler, dispatcher, store, or next step runs, the statically-known schema is erased — `ToolCall.args`/`ToolResult.output` are `map<string, unknown>`; carry/window/journal/resume payloads are `$rust_type`; success and `{error: …}` payloads are byte-indistinguishable.
- **09:** each `Tool` carries `parameters: type`, but `ToolCall.args` is `map<string, unknown>` and `ToolResult.output` is `unknown` — the known schema is erased at exactly the handler boundary.

  ```baml
  class Tool { name: string, description: string, parameters: type }  // first-class typed schema
  class ToolCall   { id: string, name: string, args: map<string, unknown> }   // <-- typed `parameters` collapses to unknown
  class ToolResult { id: string, output: unknown }                            // <-- output is unknown too

  class ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws ... {
      for (let c in calls) {
        let result: ToolResult = self.invoke_one(c) catch (e) {  // <-- must re-validate c.args against the type by hand
          _ => { ToolResult { id: c.id, output: { "error": "tool execution failed" } } }
        };
        ...
      }
      ...
    }
  }
  ```

- **44:** `Suspend.resume_schema` is a runtime `type`, but `reenter` carries the human's answer as untyped `unknown` — the declared `ApprovalDecision` is never tied by the type system to the value that flows back.

  ```baml
  class Suspend {
    payload: unknown
    resume_schema: type   // the schema the resume value MUST validate against
    point: string
  }

  interface Suspendable requires Provider {
    function start<T>(self, prompt: baml.llm.PromptAst) -> T | Suspend ...

    // re-enter a persisted run. `resume` is the human's answer (validated by the
    // caller against snap-recovered resume_schema BEFORE this is called).
    function reenter<T>(self, snap: Snapshot, resume: unknown) -> T | Suspend // <-- resume is untyped unknown, NOT typed by resume_schema
        throws baml.ExtendUnknownError<baml.errors.SuspendError>
  }
  ```


**D2. Opaque prompt / transcript: no typed structural view, so edits bottom out in host pokes.** *[workable · model-shape · 9 scenarios: 10, 14, 15, 17, 18, 21, 32, 36, 40]* `PromptAst` and the provider-owned `Transcript` are deliberately opaque, so prompt-rewriting middleware (memory, compaction, guardrails, truncation) and per-turn tool filtering bottom out in untyped host accessors that reach into another provider's state and **assume a pokeable shape**.
- **14:** `History.thread` truncates an opaque `PromptAst` by `max_chars`, but a char cut can **sever a `tool_call` from its paired `tool_result`** and produce a wire-invalid history — the opaque AST exposes no structural unit to truncate safely.

  ```baml
  class History {
    max_chars: int?
    implements Threading {
      type Carry = baml.llm.PromptAst
      function thread(self, parent_prompt: baml.llm.PromptAst, carry: baml.llm.PromptAst)
          -> baml.llm.PromptAst throws ... {
        let joined = baml.llm.concat_prompts(parent_prompt, carry);
        match (self.max_chars) {
          let n: int => baml.llm.truncate_prompt(joined, n),   // <-- char cut can sever a tool_call from its tool_result
          _ => joined,
        }
      }
    }
  }
  ```

- **10:** per-turn tool filtering calls `inner_set_tools` to mutate the *wrapped* provider's opaque `Transcript` from outside — a shim that silently assumes every provider stores tools in a pokeable field.

  ```baml
  function run_tools<T>(self, ...) -> T ... {
    ...
    match (self.per_turn_tools) {
      let f: (int, History) -> Tool[] => {
        let snap = self.inner.history(bt.inner);
        inner_set_tools(bt.inner, f(step_no, snap));  // <-- mutates the WRAPPED provider's opaque Transcript from outside
      },
      _ => {},
    }
    ...
  }

  function inner_set_tools(t: Tools.Transcript, tools: Tool[]) -> null { $rust_io_function }
  ```


### Family E — Residual frictions of the per-capability error model

(The error model is otherwise a win — see §0; these two are its honest costs.)

**E1. Crossing a capability seam boxes a typed error and drops the *static* signal.** *[workable · model-shape · 17 scenarios: 03, 10, 14, 15, 17, 18, 20, 26, 27, 32, 35, 36, 38, 41, 43, 45, 47]* When code crosses `CallError ↔ ToolError ↔ SessionError ↔ …`, a recoverable typed error is boxed into `UnknownError`. Its data survives and is recoverable via a `from<E>` probe, but **the channel type never advertises that the boxed error is reachable**, so `catch` arms don't fire and recovery is by convention.
- **15:** a judge model's recoverable rate-limit (`CallError`) is boxed inside `Guarded.step`, so the tool-loop channel never advertises that a recoverable `CallError` can be sitting there.

  ```baml
  function step<T>(self, t: InnerTranscript) -> T | ToolCalls
      throws baml.ExtendUnknownError<baml.errors.ToolError> {
    ...
    for (let g in self.output_guards) {
      let v: GuardVerdict = g.inspect<T>(value);  // judge LLM call -> recoverable rate-limit CallError
      if (v.tripped) { throw OutputTripwire { name: g.name(), info: v.info }; }
    }
    ...
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "guarded step: output guard failed");  // <-- recoverable rate-limit flattened to UnknownError, is_rate_limit() lost
  }
  ```

- **20:** `DoubleCountedChain` must `implement` *both* `CallError` and `ChainError` with two synced copies of the classifiers — "one channel per capability" can't express an error that belongs to a *layering* concern.

  ```baml
  class DoubleCountedChain {
    message: string
    implements baml.errors.CallError {
      function is_network_error(self) -> bool { return false; }
      function is_rate_limit(self)    -> bool { return false; }
      function is_parse_error(self)   -> bool { return false; }
    }
    implements baml.errors.ChainError {            // <-- second classifier set, hand-synced with the first
      function is_network_error(self) -> bool { return false; }
      function is_rate_limit(self)    -> bool { return false; }
      function is_handle_error(self)  -> bool { return true; }
    }
  }
  ```


**E2. The classifier vocabulary is request/response-shaped and wrong for the failure axis.** *[workable · model-shape · 16 scenarios: 01, 03, 05, 10, 12, 15–17, 22, 26, 27, 34, 35, 40, 42, 45]* `is_network_error` / `is_rate_limit` / `is_parse_error` answer `false` for the failures that actually drive decisions — budget-hit, policy-refusal, session-not-found, security-denial, transport-drop-vs-server-teardown — so apps lie, fall through to concrete arms, or invent per-capability classifiers that don't generalize.
- **16:** a blocked-exfil denial and a transport failure ride the same `ToolError` channel with all classifiers false, so `catch ToolError => retry()` **re-drives a denied exfil** — there is no `is_policy_refusal` / do-not-retry axis.

  ```baml
  class HumanDeniedCall {
    tool: string
    taint: string[]
    implements baml.errors.ToolError {
      function is_network_error(self) -> bool { return false; }  // <-- all classifiers false: a policy refusal,
      function is_rate_limit(self)    -> bool { return false; }  //     indistinguishable from a transport failure
      function is_parse_error(self)   -> bool { return false; }
    }
  }
  // ... at the usage seam, a generic ToolError catch re-drives the same denied call:
  let answer = t.run_tools<string>(render_for(task, facts), tools, gate)
    catch (e) {
      let te: baml.errors.ToolError => t.run_tools<string>(render_for(task, facts), tools, gate),  // <-- retry re-runs the denial
    };
  ```

- **22:** `RealtimeError` carries only `is_network_error()`, which is **actively wrong**: a dropped socket *is* a network error, so the classifier tells `Fallback` the one error it must **not** retry is safe to retry. No `is_resumable` / `is_effectful` axis.

  ```baml
  interface baml.errors.RealtimeError { /* classifiers as needed */ } // <-- only is_network_error exists; no is_resumable/is_effectful axis

  class RealtimeServerError {
    implements baml.errors.RealtimeError {
      function is_network_error(self) -> bool { return false; } // <-- the single classifier
    }
  }

  // usage catch arm — a dropped socket IS a network error, so this says "safe to retry":
  let r: baml.errors.RealtimeError => {
    if (r.is_network_error()) { ui.toast("connection dropped"); } else { ... } // <-- tells Fallback to retry the one error it must NOT
  }
  ```


### (cosmetic) No normalize-once / memoize seam

*[cosmetic · model-shape · 7 scenarios: 01, 06, 08, 13, 17, 41]* — lazy `ResponseMeta` re-parses the same body once per projected dimension (**01**); a summarizer recomputes `O(N)` over a growing prefix every turn instead of an incremental fold (**17**); the drive-to-value `match` boilerplate is hand-written N times (**41**). Purely ergonomic, but recurs.

*meta_of keeps only raw, so each dimension re-parses the same body*

```baml
function meta_of(self, from: baml.http.Response) -> ResponseMeta { ... }
// meta_of stores only the raw Response; nothing is decoded here.
class AnthropicResponse {
  raw: baml.http.Response   // <-- only raw stored; every accessor re-parses it
  implements ResponseMeta {
    function usage(self) -> Usage {
      let b = baml.json.from_json<AnthropicBodyWire>(baml.json.parse(self.raw.text()));   // <-- parse #1
      ...
    }
    function finish_reason(self) -> string {
      let b = baml.json.from_json<AnthropicBodyWire>(baml.json.parse(self.raw.text()));   // <-- parse #2, same body
      ...
    }
  }
}
```

*summarizer re-runs over the growing prefix every chat turn*

```baml
function build_chat<T>(self, history: Turn[], next: Turn) -> baml.http.Request ... {
  let n: int = history.length();
  if (n <= self.keep) { return self.inner.build_chat<T>(history, next); }
  let old: Turn[] = window_head(history, n - self.keep);   // the growing prefix, O(N)
  let synopsis: string = (match (self.summarizer) {
    let h: HttpProvider => h.call<string>(summarize_prompt(old)),  // <-- re-summarizes whole prefix EVERY turn
    ...
  }) ...;
  ...
}
```

*implement.baml drive_inner: Drivable-else-call-else-Unsupported written by hand thrice*

```baml
function drive_inner<T>(self, prompt: baml.llm.PromptAst) -> T
    throws baml.ExtendUnknownError<baml.errors.DeployError> {
  match (self.inner) {
    let d: Drivable     => d.drive_to_value<T>(prompt),
    let h: HttpProvider => h.call<T>(prompt),
    _ => throw baml.errors.Unsupported { message: "deployed inner cannot produce a value" },  // <-- same 3-arm match also in SubprocessSupervisor.drive_once + Fix companion, no primitive
  }
} catch (e) { ... }
```

---

## §2 — Genuinely-missing host primitives (the net-new list)

These are **host-surface**, not model-shape — a focused set of primitives the runtime would need to add (distinct from the large "invented stdlib that already exists" set, which was a spelling problem, see `_conventions.md`):

1. **Inbound control-inversion** — a webhook / HTTP-handler so a provider can call *us* back: background-job completion (**27**), A2A push notifications (**39**). The client/`Provider` codec is outbound-only.
2. **Typed lowering-failure channel** — schema-dialect lowering (`json_schema`/`gemini_schema`) returns a bare string with no `throws`; a dialect-inexpressible type (`$ref` cycle under Gemini) panics into `UnknownError` instead of a typed `CallError` arm (**02**).
3. **Resumable mid-stream offset** on `baml.llm.Stream`, so a stream that dies after N tokens can retry without re-emitting — streaming retry is connect-only today (**29**).
4. **Scope-bound cleanup (RAII / `defer` / `Drop`)** for span lifecycle, socket lifecycle, and cache-handle TTL — currently hand-managed discipline (**31, 32**).
5. **A capability/fingerprint probe that is honestly a fallible network call** — `system_fingerprint`, a live `supportedModels()` query — has no honest home in a "read config" method (**33, 36**).
6. **A mid-session truncation/budget hook** the contract requires a provider to consult (finish-reason driven) — a host obligation currently unenforced (**07**).

A general **UUID / idempotency key** is a small adjacent gap (`baml.id.new()` is the closest shipped thing).

---

## §3 — One-off gaps (do not cluster, still real)

- **02:** schema-lowering can fail *inside* `build_request<T>` with no typed `CallError` arm to carry it.

  ```baml
  namespace baml.schema {
    function json_schema(t: type, strict: bool) -> string { $rust_io_function }
    function gemini_schema(t: type) -> string { $rust_io_function }
  }

  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let strict = baml.schema.strict_supports(reflect.type_of<T>());
    let schema = baml.schema.json_schema(reflect.type_of<T>(), strict); // <-- lowering can fail; no typed CallError arm carries it
    ...
  }
  ```

- **11:** "charge only if the email succeeds" needs aborting a sibling call, which the `ToolCall`→one- `ToolResult` seam structurally forbids (**inherent**).

  ```baml
  class EffectAwareDispatch {
    implements Dispatcher {
      function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws ... {
        ...
        // run Write calls SEQUENTIALLY, in their original relative order. Each write
        // is independent: there is no place to read a sibling's result and abort.
        let write_results: ToolResult[] = [];
        for (let wc in write_calls) {
          let r: ToolResult = reg.invoke_one(wc) catch (e) {  // <-- charge runs even if the email write already failed; closest to "charge only if email succeeded" is mere sequencing, not a gate
            _ => { ToolResult { id: wc.id, output: { "error": "tool execution failed" } } }
          };
          write_results.push(r);
        }
        ...
      }
    }
  }
  ```

- **16:** MCP tool-**poisoning** via a rug-pulled tool *description* is out of reach — nothing in `Tool{name,description,parameters}` is hashed or pinned at connect time.

  ```baml
  // from usage.baml, the privileged loop's tool records:
  let tools: Tool[] = [
    Tool { name: "web_fetch", description: "fetch a URL", parameters: reflect.type_of<FetchArgs>() },
    Tool { name: "http_post", description: "POST data to a URL", parameters: reflect.type_of<PostArgs>() },  // <-- description is free-floating; nothing hashes/pins it at connect time, so a rug-pulled description is undetectable
    Tool { name: "read_inbox", description: "read the user's inbox", parameters: reflect.type_of<Empty>() },
  ];
  ```

- **37:** a `PermissionAsk` is emitted *up* as an `InEvent` but there is no `OutEvent` carrying an `ask_id` back *down* — the most safety-critical part of the control plane cannot be answered in the closed union.

  ```baml
  // --- session -> driver: PermissionAsk asks UP with an ask_id ---
  class PermissionAsk { ask_id: string, name: string, args: map<string, unknown> }

  type OutEvent =
      Steer
    | FollowUp
    | Interrupt
    | SetModel
    | SetPermMode
    | RewindFiles
    | StopTask
    | EndSession   // <-- no PermissionReply { ask_id } variant: nothing carries the answer back DOWN to the ask_id
  ```

- **43:** **no portable graph IR** — a workflow's topology lives in `let`/`match`/`while`/`spawn`, so there is no reifiable artifact to visualize, diff, statically check for unreachable branches, or hand to a distributed scheduler.

  ```baml
  function DocPipeline(url: string, run_id: string, ckpt: Checkpoint) -> PipelineResult throws ... {
    let doc: Doc = step<Doc>(ckpt, run_id, "fetch", url, () => { FetchDoc(url) });
    let fSummary = spawn { step<Summary>(ckpt, run_id, "summarize", doc.text, () => { Summarize(doc) }) }; // <-- edges are spawn/let
    let fLabel   = spawn { step<Label>(ckpt, run_id, "classify", doc.text, () => { Classify(doc) }) };
    let analyzed = Analyzed { summary: await fSummary, label: await fLabel };  // <-- fan-in is just await
    let review: Reviewed? = match (analyzed.label.label) {  // <-- branch node is a `match`, not a reifiable edge
      "legal" => { step<Reviewed>(ckpt, run_id, "legal_review", doc.text, () => { ... }) }
      _       => { null }
    };
    let refined = step<Refined>(ckpt, run_id, "refine", analyzed.summary.summary, () => {
      while (current.score < 0.9) { ... }  // <-- loop edge is a `while`; topology never reified as a graph IR
    });
    PipelineResult { refined: refined, review: review, label: analyzed.label.label }
  }
  ```

- **35:** nothing stops a `BearerKey{value: env.SECRET}` in browser-targeted code — "must not enter a browser bundle" is a build-time taint property the runtime-existential model has no vocabulary for.

  ```baml
  class BearerKey {
    value: string
    // ... implements Credential: sets "Authorization: Bearer" + self.value
  }

  // Browser-targeted client — nothing forbids the same BearerKey construction
  // the server clients use; no build-time taint marks this home as key-less.
  client BrowserVoiceLeak() {
    OpenAIRealtime {
      voice: "alloy",
      auth: BearerKey { value: env.OPENAI_API_KEY },  // <-- env.SECRET into a browser bundle, no error
      transport: "webrtc",
    }
  }
  ```

- **28:** `SigV4Auth` signs the whole request as the last step of `build_request`, but transport-envelope headers are applied *after* `build_request` returns — any header-injecting combinator silently **invalidates the signature** with nothing catching it.

  ```baml
  class SigV4Auth {
    implements Auth {
      function apply(self, request: baml.http.Request) -> baml.http.Request ... {
        baml.cloud.sigv4_sign(request, self.service, self.region)  // <-- HMAC over the canonical request (headers included)
      }
    }
  }

  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request ... {
    let base = baml.http.Request { ... headers: headers, ... };
    self.auth.apply(base)   // <-- signs as the LAST build step; envelope headers merged AFTER this returns invalidate the SigV4 signature
  }
  ```

- **30:** a `client` body that runs a real billed LLM call (a router/classifier) buries latency, cost, and a `CallError` in "construction" — invisible at the call site, where the model frames `client` as a pure function returning a `Provider`.

  ```baml
  client RoutedByClassifier(q: string) {
    if (classify_difficulty(q) == "hard") { Full() } else { Mini() }   // <-- billed LLM call in client body
  }

  function classify_difficulty(q: string) -> string {
    client: Mini()
    prompt #"Classify the difficulty ... either "easy" or "hard": {{ q }}"#
  }

  function Answer4(q: string) -> Answer {
    client: RoutedByClassifier(q)   // <-- client construction now does a real call (latency/cost/CallError)
    prompt #"{{ q }}"#
  }
  ```


---

## §4 — Verdict

| Verdict | Count |
|---|---|
| **Clean** (no additions) | 0 |
| **Workable-with-additions** | 47 |
| **Awkward** | 0 |
| **Unsupported** | 0 |

**The model is strong where the world is stateless, and strains where the world is stateful.** Every scenario is expressible, and the marker-`Provider` + per-capability-interface spine carries stateless request/response and most tool loops cleanly. But two fault lines recur with force:

1. **The per-call, value-oriented model has no home for cross-call, server-authoritative state, for chain-wide provenance, or for non-error control outcomes** (Families A & C). This is where the *fatal* gaps cluster — jobs, caching, realtime, sessions, durability, multi-agent handoff — because a combinator that forwards a capability by `match` cannot know the capability is stateful or non-idempotent, and a `(T, U)` sidecar cannot carry what the whole *chain* cost or did.
2. **The existential `client` makes every capability a runtime present-or-absent promise** that cannot be proven statically, graded for quality, or specialized in a combinator (Family B1) — and where two providers *are* statically equal but semantically/economically different, **no addition can fix it** (B2, the one inherent family).

The model-shape work the design most needs, in rough priority: a first-class **stateful / server-owned** notion (a session/handle object with lifecycle, and a "non-retryable / effectful" marker so combinators stop silently mis-driving it); a way to **refine the existential** so a signature can demand a capability statically (`Provider & Streaming`-style) without forfeiting swappability; a **chain-aware** projection so provenance/cost survives fallback and sub-calls; **typed seams** for tool args/output and resume payloads; and a richer **error classifier axis** (retryable / effectful / policy) plus a typed structural view over `PromptAst`. The host-surface asks (§2) are a short, concrete list. The inherent residue (B2, plus genuinely server-owned state and a few security/taint properties in §3) is the honest floor: the model can make these *visible* but not *go away*.
