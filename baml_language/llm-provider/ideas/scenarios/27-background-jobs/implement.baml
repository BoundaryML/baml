// =============================================================================
// implement.baml — Async jobs: background + poll (the FOURTH request lifecycle)
//
// Library / std-lib author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, Streaming). Sync is one blocking
// response; Streaming is one held connection; Realtime is a live duplex session.
// Background+poll is NONE of these:
//
//   1. `submit` returns an OPAQUE job id IMMEDIATELY, holding no connection.
//   2. the work runs server-side; the client process may die.
//   3. you come back later, by id, and `poll` until the job lands.
//   4. `wait_for` is sugar: submit, then poll with backoff until terminal.
//
// NOTE: this is SERVER-SIDE submit/poll durability — a DIFFERENT thing from
// BEP-034 in-process structured concurrency (`spawn`/`await`/`Future`). A job
// runs on the provider's servers and survives this process dying; a `spawn`
// runs in THIS runtime. The two are orthogonal: you could `spawn` several
// `wait_for` calls to drive many durable jobs concurrently in-process, but the
// durability is the server's, not the future's. We keep that distinction; the
// blocking convenience here is named `wait_for` (NOT `await`, which is now a
// reserved language keyword for awaiting a `baml.future.Future`).
//
// The teaching point is durability/resumability of a SINGLE long call. The job
// id is provider-owned and survives disconnect. The store/ZDR tension, the
// poll cadence/backoff, and "who owns the retry/idempotency key" all live here.
//
// Net-new surface this file introduces (flagged in evaluation.md):
//   - the `Background` capability interface (submit / poll / cancel / wait_for)
//   - a NEW `baml.errors.BackgroundError` capability-error interface (submit/poll
//     are NONE of call/stream/realtime/tools, so they need their own channel)
//   - concrete typed domain errors: JobFailed / JobCancelled / PollDeadlineExceeded
//   - a `Job<T>` handle type carrying an opaque server id + provider owner
//   - a `JobState<T>` reflecting SERVER-AUTHORITATIVE status (we don't invent it)
//   - a `Poller` combinator owning cadence/backoff (NOT a Provider combinator)
//
// Everything else here is REAL stdlib (verified against baml_std): request bodies
// are native `map<string, json>` literals serialized with `baml.json.stringify`;
// responses are decoded ONCE into typed wire classes with `baml.json.from_json<T>`
// (Response has no `.json_path`); strings concat with `+` and compare with `==`;
// backoff arithmetic uses `int.pow` / `int.min` / `int.random`; the dedupe key is
// `baml.id.new()`; `baml.sys.sleep` is the BEP-034 cooperative sleep (reused — NOT
// net-new). The ONLY genuine host seam left is reading Anthropic's JSONL results
// file (we scan the lines ourselves with real string/array methods + parse).
// =============================================================================


// -----------------------------------------------------------------------------
// NET-NEW capability-error interface. submit / poll / cancel / wait_for are NOT on
// the call/stream/realtime/tool paths — they are a durable, fire-and-poll
// lifecycle — so they get their OWN capability-error channel,
// baml.ExtendUnknownError<baml.errors.BackgroundError>, mirroring CallError &
// friends. The classifiers triage transport vs domain without the concrete class.
// `is_terminal_job_failure` is the background-specific axis: did the SERVER kill
// the job (re-submit needed), or did we merely time out polling (re-drive)?
// -----------------------------------------------------------------------------

interface baml.errors.BackgroundError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  // True iff the job itself reached a terminal Failed/Cancelled on the SERVER —
  // a fresh submit is required to make progress. False for a poll timeout, where
  // re-driving the SAME persisted job is the right move.
  function is_terminal_job_failure(self) -> bool
}

// The SERVER failed the job (JobStatus.Failed). A deliberate domain error, NOT a
// transport failure; carries the server's own error string.
class JobFailed {
  reason: string
  implements baml.errors.BackgroundError {
    function is_network_error(self)        -> bool { return false; }
    function is_rate_limit(self)           -> bool { return false; }
    function is_terminal_job_failure(self) -> bool { return true; }
  }
}

// The job was cancelled server-side (JobStatus.Cancelled). Terminal: re-submit to
// retry.
class JobCancelled {
  implements baml.errors.BackgroundError {
    function is_network_error(self)        -> bool { return false; }
    function is_rate_limit(self)           -> bool { return false; }
    function is_terminal_job_failure(self) -> bool { return true; }
  }
}

// We ran out of patience polling — the job is STILL RUNNING server-side. NOT a
// terminal failure: the caller re-drives the same persisted job rather than
// re-submitting (which would double-spend a multi-minute, billed job).
class PollDeadlineExceeded {
  job_id: string
  implements baml.errors.BackgroundError {
    function is_network_error(self)        -> bool { return false; }
    function is_rate_limit(self)           -> bool { return false; }
    function is_terminal_job_failure(self) -> bool { return false; }
  }
}

// A wrong-owner handle reached poll/cancel: a Job<T> minted by one provider can
// never be polled by another. A programming/correlation error, surfaced typed so
// the app can distinguish it from a real job failure.
class WrongOwnerJob {
  expected: string
  actual: string
  implements baml.errors.BackgroundError {
    function is_network_error(self)        -> bool { return false; }
    function is_rate_limit(self)           -> bool { return false; }
    function is_terminal_job_failure(self) -> bool { return false; }
  }
}

// store:false + background:true is the ZDR ⊥ background contradiction. It can be
// raised on the CALL path (build_request) and the JOB path (submit), so it
// implements BOTH CallError and BackgroundError and is legal on either channel.
class ZdrIncompatible {
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.BackgroundError {
    function is_network_error(self)        -> bool { return false; }
    function is_rate_limit(self)           -> bool { return false; }
    function is_terminal_job_failure(self) -> bool { return true; }
  }
}


// -----------------------------------------------------------------------------
// 0. The handle the application holds across disconnects.
//
//    This is the irreducible new value. The base Provider is single-shot: it
//    has nowhere to put "here is an id, come back later". A Job<T> is an opaque
//    pointer to provider-owned, server-stored state. The app persists it (to its
//    own DB, a queue message, a URL param) and re-hydrates after a restart. The
//    `T` rides along so poll/wait_for stay typed end to end.
// -----------------------------------------------------------------------------

class Job<T> {
  // The provider-minted id, e.g. OpenAI "resp_..." or Anthropic Batch "msgbatch_...".
  // OPAQUE to the application exactly like Tools.Transcript — never interpreted.
  id: string
  // Which provider class minted this id. A job id is meaningless to another
  // provider, and crucially to another COMBINATOR: a Fallback cannot poll an id
  // a different member created. We check this on every poll() (see §1, §4).
  owner: string
  // The idempotency key used at submit time. Carried so a RETRIED submit can
  // reuse it (provider dedupes) and so a poller reconciling with the app's own
  // queue can match a job to the request that spawned it (background §"what's hard").
  idempotency_key: string
}

// Server-AUTHORITATIVE status. We mirror the provider's lifecycle verbatim
// (queued / in_progress / completed / failed / cancelled) rather than inventing
// our own — the whole point of background jobs is that the SERVER owns this state
// and we are reading it. `T` is only present on Completed.
enum JobStatus {
  Queued
  InProgress
  Completed
  Failed
  Cancelled
}

// One poll's worth of truth. `terminal` lets a poller stop without re-deriving
// it from the enum each loop. `value` is present iff status == Completed;
// `error` iff Failed. This is the shape `poll` returns and `wait_for` loops on.
class JobState<T> {
  status: JobStatus
  terminal: bool
  // Present only when status == Completed. The typed answer, parsed once.
  value: T?
  // Present only when status == Failed — the SERVER's error, not a transport error.
  error: string?
}


// -----------------------------------------------------------------------------
// 1. The Background capability. A provider that can run a single call as a
//    durable, fire-and-poll server-side job.
//
//    Like every capability it `requires Provider`, so a Background provider also
//    has a best-effort blocking `.call` (Open Question 1: here `.call` is just
//    `wait_for(submit(...))` — a foreground-shaped convenience over the same job).
//
//    submit/poll/cancel are the irreducible verbs; `wait_for` is the DEFAULT method
//    that ties them together with a caller-supplied cadence — mirroring how
//    Tools.run_tools is a default over begin/step/submit. The cadence is a
//    PARAMETER (handed in at the call), never a provider field: backoff policy is
//    a property of the caller's environment, not the model.
// -----------------------------------------------------------------------------

interface Background requires Provider {
  // Create the job. Returns IMMEDIATELY with an id; holds no connection. The
  // idempotency key is generated by the caller (or `wait_for`) so a retried submit
  // is deduped server-side instead of spawning a second job.
  function submit<T>(self, prompt: baml.llm.PromptAst, idem: string) -> Job<T>
      throws baml.ExtendUnknownError<baml.errors.BackgroundError>

  // Read server-authoritative status by id. Survives disconnect: this is a fresh
  // GET, it does not depend on any state the submitting process held. Re-hydrate
  // a persisted Job<T> and poll() works even after a restart.
  function poll<T>(self, job: Job<T>) -> JobState<T>
      throws baml.ExtendUnknownError<baml.errors.BackgroundError>

  // Best-effort server-side cancel. Returns whether the server moved it to
  // Cancelled (a job that already landed cannot be cancelled).
  function cancel<T>(self, job: Job<T>) -> bool
      throws baml.ExtendUnknownError<baml.errors.BackgroundError>

  // DEFAULT: the blocking convenience. submit, then poll on a caller-supplied
  // cadence until terminal. The cadence object owns backoff (§3). This is what a
  // synchronous caller wants; a UI that must surface "queued / running / done"
  // calls submit + poll itself and never uses this. Named `wait_for` (NOT
  // `await`): `await` is the BEP-034 keyword for awaiting an in-process Future;
  // this is a server-side poll loop, a different mechanism entirely.
  function wait_for<T>(self, prompt: baml.llm.PromptAst, cadence: Cadence) -> T
      throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
    let job: Job<T> = self.submit<T>(prompt, baml.id.new());
    let attempt = 0;
    while (true) {
      let st: JobState<T> = self.poll<T>(job);
      match (st.status) {
        JobStatus.Completed => {
          // value is present iff Completed; the `??` is unreachable but keeps the
          // type honest (T? -> T).
          return st.value ?? baml.sys.panic("completed job had no value");
        }
        JobStatus.Failed => {
          // The SERVER failed the job (not a transport error) — a deliberate,
          // typed domain failure the app branches on.
          throw JobFailed { reason: st.error ?? "unknown" };
        }
        JobStatus.Cancelled => {
          throw JobCancelled {};
        }
        _ => {
          // Queued or InProgress: wait the cadence-dictated interval and re-poll.
          // The cadence decides cap/jitter; we just sleep and count. baml.sys.sleep
          // is the BEP-034 cooperative sleep (it yields, it does not peg a thread).
          baml.sys.sleep(cadence.next_delay_ms(attempt));
          attempt = attempt + 1;
        }
      }
    }
    baml.sys.panic("unreachable")
  }
}


// -----------------------------------------------------------------------------
// 2a. OpenAIResponses background — the flagship. `background: true` returns an id
//     at once; you GET /v1/responses/{id} to poll. REQUIRES store: true, which is
//     exactly what Zero-Data-Retention forbids — so a ZDR deployment cannot use
//     this lifecycle at all. We encode that as a HARD construction-time guard.
// -----------------------------------------------------------------------------

class OpenAIResponses {
  model: string
  api_key: string
  // The store/ZDR tension, made physical. background jobs need store:true to be
  // retrievable; ZDR forbids storage. The two cannot coexist. We refuse to even
  // construct a Background-capable provider with store:false, rather than fail
  // mysteriously at the first poll.
  store: bool

  function background(model: string) -> OpenAIResponses {
    OpenAIResponses { model: model, api_key: env.OPENAI_API_KEY, store: true }
  }

  // The stateless face: a normal blocking chat model (store:false is fine here).
  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      self.build_responses<T>(prompt, false, null)
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "responses http failed"); }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Decode the body ONCE into the typed wire class, then SAP-parse the text.
      let body = baml.json.from_json<OpenAIResponsesWire>(baml.json.parse(from.text()));
      baml.sap.parse<T>(first_responses_text(body))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "responses parse failed");
    }
  }

  // ONE request builder; `bg` flips background-mode, `idem` rides the header.
  // This is where the lifecycle divergence physically lives: same logical turn,
  // a different body + a different return contract (id-now vs answer-now).
  //
  // Called from BOTH the call path (build_request -> CallError) and the job path
  // (submit -> BackgroundError), so the ZDR guard error (ZdrIncompatible) is a
  // concrete class that implements BOTH channels — legal on either declared throws.
  function build_responses<T>(self, prompt: baml.llm.PromptAst, bg: bool, idem: string?)
      -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> {
    if (bg && !self.store) {
      // The ZDR tension, caught early. Should be impossible via background() but
      // a hand-built store:false provider could reach here.
      throw ZdrIncompatible {};
    }
    let schema = reflect.type_of<T>();
    // Native `map<string, json>` literal — each value is already `json`
    // (strings/bools, the rendered input, the schema map). No hand-rolled JSON.
    let base: map<string, json> = {
      "model": self.model,
      "input": baml.llm.render_user_turn(prompt),
      "response_format": baml.llm.response_schema_json(schema),
      "background": bg,
      "store": self.store,
    };
    // The idempotency key is a HEADER on the create call (OpenAI's Idempotency-Key).
    // Absent on plain .call; present on submit so a retried create dedupes.
    let headers: map<string, string> = {
      "content-type": "application/json",
      "authorization": "Bearer " + self.api_key,
    };
    match (idem) {
      null          => {},
      let k: string => { headers.set("Idempotency-Key", k); },
    };
    baml.http.Request {
      method: "POST",
      url: "https://api.openai.com/v1/responses",
      headers: headers,
      body: baml.json.stringify(base),
    }
  }

  implements Background {
    function submit<T>(self, prompt: baml.llm.PromptAst, idem: string) -> Job<T>
        throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
      let req = self.build_responses<T>(prompt, true, idem);
      // The create call returns AT ONCE — status "queued", id "resp_...". We read
      // only the id; we deliberately do NOT block for the answer.
      let resp = baml.http.send(req);
      let wire = baml.json.from_json<OpenAIResponsesWire>(baml.json.parse(resp.text()));
      Job<T> { id: wire.id, owner: "OpenAIResponses", idempotency_key: idem }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "openai responses submit failed");
    }

    function poll<T>(self, job: Job<T>) -> JobState<T>
        throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
      if (job.owner != "OpenAIResponses") {
        throw WrongOwnerJob { expected: "OpenAIResponses", actual: job.owner };
      }
      // A fresh GET by id. No held connection, no submit-time state needed — THIS
      // is what makes the job survive a client restart.
      let resp = baml.http.send(baml.http.Request {
        method: "GET",
        url: "https://api.openai.com/v1/responses/" + job.id,
        headers: { "authorization": "Bearer " + self.api_key },
        body: "",
      });
      // The provider's status string is the SOURCE OF TRUTH. We translate, we do
      // not invent. Decode once; only on "completed" do we lift the typed value.
      let wire = baml.json.from_json<OpenAIResponsesWire>(baml.json.parse(resp.text()));
      self.lift_state<T>(wire)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "openai responses poll failed");
    }

    function cancel<T>(self, job: Job<T>) -> bool
        throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
      let resp = baml.http.send(baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/responses/" + job.id + "/cancel",
        headers: { "authorization": "Bearer " + self.api_key },
        body: "",
      });
      let wire = baml.json.from_json<OpenAIResponsesWire>(baml.json.parse(resp.text()));
      resp.ok() && (wire.status == "cancelled")
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "openai responses cancel failed");
    }
  }

  // Translate the SERVER's status string into our enum. This is the only place
  // the wire vocabulary is interpreted; the rest of the system speaks JobStatus.
  // The body was already decoded into the typed wire class by the caller.
  // On the poll path -> BackgroundError channel.
  function lift_state<T>(self, wire: OpenAIResponsesWire) -> JobState<T>
      throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
    if (wire.status == "completed") {
      let v: T = baml.sap.parse<T>(first_responses_text(wire));
      JobState<T> { status: JobStatus.Completed, terminal: true, value: v, error: null }
    } else if (wire.status == "failed") {
      let msg = match (wire.error) {
        let e: OpenAIError => match (e.message) { let m: string => m, _ => "" },
        _ => "",
      };
      JobState<T> { status: JobStatus.Failed, terminal: true, value: null, error: msg }
    } else if (wire.status == "cancelled") {
      JobState<T> { status: JobStatus.Cancelled, terminal: true, value: null, error: null }
    } else if (wire.status == "in_progress") {
      JobState<T> { status: JobStatus.InProgress, terminal: false, value: null, error: null }
    } else {
      // "queued" and any unknown not-yet-terminal string both map here.
      JobState<T> { status: JobStatus.Queued, terminal: false, value: null, error: null }
    }
  } catch (e) {
    // SAP parse of the completed value can fail foreign — normalize.
    _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "openai responses result parse failed");
  }
}

// Typed wire shapes for the OpenAI Responses API — decoded once with
// `baml.json.from_json<…>` (Response has no `.json_path`). Fields we don't touch
// are simply absent; optionals tolerate their being missing across lifecycle states.
class OpenAIResponsesWire {
  id: string?
  status: string?
  output: OpenAIOutputItem[]?
  error: OpenAIError?
}
class OpenAIOutputItem {
  content: OpenAIContentPart[]?
}
class OpenAIContentPart {
  text: string?
}
class OpenAIError {
  message: string?
}

// Extract output[0].content[0].text, defaulting to "" — the path the invented
// `json_path("output.0.content.0.text")` used to walk, now over the typed class.
function first_responses_text(wire: OpenAIResponsesWire) -> string {
  match (wire.output) {
    let items: OpenAIOutputItem[] => match (items.at(0)) {
      let item: OpenAIOutputItem => match (item.content) {
        let parts: OpenAIContentPart[] => match (parts.at(0)) {
          let part: OpenAIContentPart => match (part.text) { let t: string => t, _ => "" },
          _ => "",
        },
        _ => "",
      },
      _ => "",
    },
    _ => "",
  }
}


// -----------------------------------------------------------------------------
// 2b. AnthropicBatch — a DIVERGENT background shape. Same lifecycle verbs, a
//     totally different wire model: a job is a BATCH of one (or many) requests,
//     the result is NOT inline — poll gives you a `results_url` you must fetch
//     SEPARATELY, and there is no per-request id at submit time, only the batch
//     id plus a custom_id you assign. We collapse the batch-of-one into the same
//     Job<T> shape so the application code is identical (portability proof).
//
//     This is the leak the scenario must show: "poll returns the answer inline"
//     is OpenAI-specific. Anthropic needs a SECOND fetch on completion, hidden
//     inside poll(). The Job<T> handle absorbs it; the app never sees it.
// -----------------------------------------------------------------------------

class AnthropicBatch {
  model: string
  api_key: string
  max_tokens: int
  // Anthropic Message Batches are inherently stored (24h) — there is no
  // store:false / ZDR variant of batch at all. So a ZDR shop can't use THIS
  // provider's Background face either, but for a different structural reason.

  function default(model: string) -> AnthropicBatch {
    AnthropicBatch { model: model, api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Foreground .call uses the normal /v1/messages endpoint (not batch).
      let schema = reflect.type_of<T>();
      let body: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,
        "messages": baml.llm.render_messages(prompt),
        "tools": baml.llm.forced_schema_tool(schema),
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: self.anthropic_headers(),
        body: baml.json.stringify(body),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http failed"); }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Decode once into the typed wire class; the tool-use input rides
      // content[0].input as a json sub-tree, SAP-parsed into T.
      let b = baml.json.from_json<AnthropicMessageWire>(baml.json.parse(from.text()));
      baml.sap.parse<T>(first_anthropic_input(b))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }

  // Anthropic's required header trio, inlined (the invented `anthropic_headers`
  // sugar was just this map).
  function anthropic_headers(self) -> map<string, string> {
    {
      "content-type": "application/json",
      "x-api-key": self.api_key,
      "anthropic-version": "2023-06-01",
    }
  }

  implements Background {
    function submit<T>(self, prompt: baml.llm.PromptAst, idem: string) -> Job<T>
        throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
      let schema = reflect.type_of<T>();
      // A batch of EXACTLY ONE request. We use the caller's idempotency key as the
      // custom_id, so the single result inside the batch is correlatable on poll —
      // and so a retried submit reuses the same custom_id (Anthropic dedupes on it).
      // Native nested json: a `json[]` of one request map, each value already json.
      let params: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,
        "messages": baml.llm.render_user_turn(prompt),
        "tools": baml.llm.forced_schema_tool(schema),
      };
      let one_request: map<string, json> = {
        "custom_id": idem,
        "params": params,
      };
      let body: map<string, json> = {
        "requests": [one_request],
      };
      let resp = baml.http.send(baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages/batches",
        headers: self.anthropic_headers(),
        body: baml.json.stringify(body),
      });
      // The batch id is the durable handle; custom_id (= idem) correlates the one
      // result inside it.
      let wire = baml.json.from_json<AnthropicBatchWire>(baml.json.parse(resp.text()));
      Job<T> { id: wire.id ?? "", owner: "AnthropicBatch", idempotency_key: idem }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "anthropic batch submit failed");
    }

    function poll<T>(self, job: Job<T>) -> JobState<T>
        throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
      if (job.owner != "AnthropicBatch") {
        throw WrongOwnerJob { expected: "AnthropicBatch", actual: job.owner };
      }
      let resp = baml.http.send(baml.http.Request {
        method: "GET",
        url: "https://api.anthropic.com/v1/messages/batches/" + job.id,
        headers: self.anthropic_headers(),
        body: "",
      });
      // Anthropic's batch status is "in_progress" | "canceling" | "ended". "ended"
      // is NOT success — it means processing finished; each request may have
      // succeeded/errored independently. So "ended" forces a SECOND fetch of the
      // results file. This extra round-trip is exactly the divergence Job<T> hides.
      let wire = baml.json.from_json<AnthropicBatchWire>(baml.json.parse(resp.text()));
      let proc = wire.processing_status ?? "";
      if (proc == "ended") {
        self.fetch_one_result<T>(wire.results_url ?? "", job.idempotency_key)
      } else if (proc == "canceling") {
        JobState<T> { status: JobStatus.InProgress, terminal: false, value: null, error: null }
      } else {
        JobState<T> { status: JobStatus.InProgress, terminal: false, value: null, error: null }
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "anthropic batch poll failed");
    }

    function cancel<T>(self, job: Job<T>) -> bool
        throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
      let resp = baml.http.send(baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages/batches/" + job.id + "/cancel",
        headers: self.anthropic_headers(),
        body: "",
      });
      resp.ok()
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "anthropic batch cancel failed");
    }
  }

  // The SECOND fetch. The results file is JSONL; we find the line whose custom_id
  // matches our idempotency key and lift just that one. This is the whole "batch
  // result is not inline" divergence, contained. On the poll path -> BackgroundError.
  function fetch_one_result<T>(self, results_url: string, custom_id: string) -> JobState<T>
      throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
    let resp = baml.http.send(baml.http.Request {
      method: "GET", url: results_url, headers: self.anthropic_headers(), body: "",
    });
    // The results body is JSONL — one JSON object per line. Decode each line into
    // the typed result class (real string/array methods + `baml.json.from_json`),
    // and keep the one whose custom_id matches ours. No host `jsonl_find` seam.
    let lines: AnthropicBatchResultLine[] = resp.text().lines()
      .filter((ln: string) -> bool { ln.trim().length() > 0 })
      .map((ln: string) -> AnthropicBatchResultLine {
        baml.json.from_json<AnthropicBatchResultLine>(baml.json.parse(ln))
      });
    let mine = lines.find((l: AnthropicBatchResultLine) -> bool { l.custom_id == custom_id });
    match (mine) {
      null => {
        JobState<T> { status: JobStatus.Failed, terminal: true, value: null, error: "result line not found" }
      }
      let line: AnthropicBatchResultLine => {
        let kind = match (line.result) {
          let r: AnthropicBatchResult => r.type ?? "",
          _ => "",
        };                                       // "succeeded" | "errored" | "canceled" | "expired"
        if (kind == "succeeded") {
          let v: T = baml.sap.parse<T>(first_batch_input(line));
          JobState<T> { status: JobStatus.Completed, terminal: true, value: v, error: null }
        } else if (kind == "canceled") {
          JobState<T> { status: JobStatus.Cancelled, terminal: true, value: null, error: null }
        } else {
          let err = match (line.result) {
            let r: AnthropicBatchResult => match (r.error) {
              let e: AnthropicBatchError => e.type ?? "",
              _ => "",
            },
            _ => "",
          };
          JobState<T> { status: JobStatus.Failed, terminal: true, value: null, error: err }
        }
      }
    }
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.BackgroundError>(e, "anthropic batch results fetch failed");
  }
}

// Typed wire shapes for the Anthropic Message Batches API — decoded with
// `baml.json.from_json<…>`. The tool-use input (the structured answer) rides
// content[0].input as a json sub-tree, SAP-parsed into T downstream.
class AnthropicMessageWire {
  content: AnthropicContentBlock[]?
}
class AnthropicContentBlock {
  type: string?
  input: json?
}
class AnthropicBatchWire {
  id: string?
  processing_status: string?
  results_url: string?
}
class AnthropicBatchResultLine {
  custom_id: string?
  result: AnthropicBatchResult?
}
class AnthropicBatchResult {
  type: string?
  message: AnthropicMessageWire?
  error: AnthropicBatchError?
}
class AnthropicBatchError {
  type: string?
}

// content[0].input as a serialized json string for SAP — both the foreground
// /v1/messages parse and the batch result line walk this same shape.
function first_anthropic_input(msg: AnthropicMessageWire) -> string {
  match (msg.content) {
    let blocks: AnthropicContentBlock[] => match (blocks.at(0)) {
      let blk: AnthropicContentBlock => match (blk.input) {
        let inp: json => baml.json.stringify(inp),
        _ => "",
      },
      _ => "",
    },
    _ => "",
  }
}

function first_batch_input(line: AnthropicBatchResultLine) -> string {
  match (line.result) {
    let r: AnthropicBatchResult => match (r.message) {
      let m: AnthropicMessageWire => first_anthropic_input(m),
      _ => "",
    },
    _ => "",
  }
}


// -----------------------------------------------------------------------------
// 3. Cadence — the poll-backoff policy. NOT a provider field and NOT a Provider
//    combinator: it is the caller's environment talking. The provider owns the
//    job; the CALLER owns the timing of the looking. A worker that polls 50 jobs
//    wants jitter; a CLI that polls one wants a tight loop. So `wait_for` takes a
//    Cadence parameter and asks it for the next delay.
//
//    This is the "polling cadence/backoff is on you" of background §"what's hard"
//    made into a first-class, swappable value instead of a buried sleep(2).
// -----------------------------------------------------------------------------

interface Cadence {
  // Given the zero-based attempt count, how long to wait before the next poll.
  function next_delay_ms(self, attempt: int) -> int
}

// Fixed interval — the naive sleep(2000) loop.
class FixedCadence {
  interval_ms: int
  implements Cadence {
    function next_delay_ms(self, attempt: int) -> int { self.interval_ms }
  }
}

// Capped exponential backoff with jitter — the right default for a shared worker
// so a thundering herd of pollers doesn't hammer the retrieve endpoint.
class BackoffCadence {
  base_ms: int
  cap_ms: int
  implements Cadence {
    function next_delay_ms(self, attempt: int) -> int {
      // base * 2^attempt, capped, plus +/- jitter. `int.pow` / `int.min` are pure
      // int methods; `int.random(lo, hi)` is the static jitter source (it throws on
      // an empty range, so we only draw when the jitter window is non-empty).
      let raw = self.base_ms * (2).pow(attempt);
      let capped = raw.min(self.cap_ms);
      let window = capped / 4;
      if (window <= 0) { return capped; }
      capped - (int.random(0, window) catch (e) { _ => 0 })   // up to 25% downward jitter
    }
  }
}


// -----------------------------------------------------------------------------
// 4. Poller — a JOB combinator (not a Provider combinator). It wraps a Background
//    provider and owns: the cadence, a hard deadline, and reconciliation with the
//    caller's OWN retry/idempotency. The generic Provider combinators (Retry,
//    Fallback, Cache) deliberately do NOT and CANNOT forward submit/poll: a job
//    id is owner-bound, so a Fallback can't poll member-B's id with member-A.
//    That asymmetry is the point — orchestration of a durable job is different
//    from orchestration of a blocking call.
// -----------------------------------------------------------------------------

class Poller {
  inner: Background
  cadence: Cadence
  // Stop polling after this many attempts even if not terminal — the job keeps
  // running server-side; we just stop LOOKING. The handle is returned so the
  // caller can persist it and resume later (durability across the poller itself).
  max_attempts: int

  // Reconciliation with the caller's own infra (background §"what's hard"): the
  // CALLER supplies the idempotency key, so submitting through the same Poller
  // twice with the same key does not spawn a second server job. We never generate
  // it here — that would re-introduce the double-submit hazard.
  function start<T>(self, prompt: baml.llm.PromptAst, idem: string) -> Job<T>
      throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
    self.inner.submit<T>(prompt, idem)
  }

  // Resume polling a PERSISTED job (e.g. re-hydrated after a process restart).
  // Returns either the terminal state, or — if we hit max_attempts — a non-terminal
  // state the caller can act on (keep waiting, give up, surface "still running").
  // This is the durability seam: poll() is a fresh GET, so this works on a job
  // whose submit happened in a different process entirely.
  function drive<T>(self, job: Job<T>) -> JobState<T>
      throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
    let attempt = 0;
    while (attempt < self.max_attempts) {
      let st: JobState<T> = self.inner.poll<T>(job);
      if (st.terminal) { return st; }
      baml.sys.sleep(self.cadence.next_delay_ms(attempt));
      attempt = attempt + 1;
    }
    // Deadline hit, job NOT terminal. Hand back the last non-terminal state. The
    // caller decides: persist `job` and resume, escalate, or abandon. We do not
    // cancel — the server job is fine; only our patience ran out.
    self.inner.poll<T>(job)
  }

  // The full blocking convenience with reconciliation baked in: caller's key in,
  // typed answer out. Mirrors Background.wait_for but with a deadline and the
  // caller-owned key. Throws on Failed/Cancelled/deadline.
  function run<T>(self, prompt: baml.llm.PromptAst, idem: string) -> T
      throws baml.ExtendUnknownError<baml.errors.BackgroundError> {
    let job: Job<T> = self.start<T>(prompt, idem);
    let st: JobState<T> = self.drive<T>(job);
    match (st.status) {
      JobStatus.Completed => { st.value ?? baml.sys.panic("completed without value") }
      JobStatus.Failed    => { throw JobFailed { reason: st.error ?? "job failed" }; }
      JobStatus.Cancelled => { throw JobCancelled {}; }
      _ => {
        // Non-terminal at deadline. This is NOT a failure of the job — it's a
        // failure of our willingness to wait. Distinct typed error (is_terminal_job_failure
        // -> false) so callers can retry by RE-DRIVING the same persisted job rather
        // than re-submitting.
        throw PollDeadlineExceeded { job_id: job.id };
      }
    }
  }
}


// -----------------------------------------------------------------------------
// Real stdlib used above (verified against baml_std — NOT host seams):
// -----------------------------------------------------------------------------
// baml.sys.sleep(ms: int) -> null                       // BEP-034 cooperative sleep (yields)
// baml.sys.panic(msg: string) -> never
// baml.id.new() -> string                               // fresh dedupe key for wait_for()
// int.pow / int.min / int.random(lo, hi)                // pure backoff arithmetic
// `a + b` (string concat) ; `a == b` (string eq)        // string operators/methods
// resp.text().lines().filter(..).map(..).find(..)       // Array/String methods for JSONL
// baml.json.stringify(map) ; baml.json.parse(s) ; baml.json.from_json<T>(j)  // build/read
// headers are plain `map<string, string>` literals      // (no bearer/with_header/anthropic_headers sugar)
//
// Provider-implementation helpers (NOT stdlib primitives — they live inside the
// request builders, like the openai_*/anthropic_* wire-shapers):
// baml.llm.render_user_turn / render_messages / response_schema_json / forced_schema_tool
