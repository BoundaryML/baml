// =============================================================================
// implement.baml — Cost & tokens + batch / async inference
//
// Library / std-lib author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, Streaming) and 01-providers-clients-
// capabilities.md. Two intertwined problems live here:
//
//   A. METERING. Every response carries a usage object, but in three vocabularies
//      (OpenAI prompt/completion, Anthropic input/output + cache_creation/read,
//      Gemini prompt/candidates/cached). Reasoning tokens are billed as output
//      but invisible. Cost = usage x a pricing matrix with several axes
//      (direction, cache state, reasoning, tier, processing mode). A Budget must
//      SUM usage across a fan-out tree (tool steps, sub-agents, retries).
//
//   B. BATCH. submit -> poll -> download, ~50% off, a SEPARATE rate-limit pool,
//      results come back UNORDERED and are rejoined by custom_id. This is a
//      lifecycle capability, not one HTTP round-trip, and it is a pricing axis
//      (batch ~= 0.5x; service_tier flex/priority shifts the sync price).
//
// The teaching points are: (1) the base `call<T> -> T` has NOWHERE to put usage,
// so metering needs a net-new seam; (2) usage normalization is per-provider, so
// it lives INSIDE the provider like the four structured-output encodings; (3)
// batch is server-authoritative state, which the existential `Provider` cannot
// promise at compile time.
//
// Net-new surface this file introduces (flagged in evaluation.md):
//   - `Usage` (normalized) + `PriceTable` + `Cost`            (metering vocab)
//   - `Metered<T>` — a (value, usage) pair, the thing `call` cannot return
//   - the `Metering` capability interface (call_metered + usage normalize)
//   - a `Budget` combinator that wraps a Provider and SUMS over a fan-out tree
//   - the `Batch` capability interface (submit/poll/download lifecycle)
//   - `BatchHandle` / `BatchJob` / `BatchState` server-authoritative types
//   - host seams: baml.sys.now_ms, baml.sys.sleep_ms, baml.json.*, file upload
// =============================================================================


// -----------------------------------------------------------------------------
// 0. The normalized usage vocabulary.
//
//    The background shows three provider vocabularies for the SAME concept. We
//    pick ONE canonical shape (closest to the Vercel SDK's normalized fields)
//    and make each provider map its wire dialect into it. Crucially:
//
//    - `reasoning_tokens` is broken out but is ALSO counted inside output_tokens
//      on providers that bill it as output (OpenAI o-series). We keep it as a
//      sub-field, not a sibling, so summing never double-counts (see Usage.add).
//    - `cache_read` / `cache_write` are itemized because they price differently
//      (read is discounted, write is a premium over normal input).
//    - everything is nullable-with-default-0 because providers omit fields (and
//      streaming omits usage entirely unless opted in — background "what varies").
// -----------------------------------------------------------------------------

class Usage {
  input_tokens: int          // provider "input" / "prompt" / "prompt_token" count
  output_tokens: int         // provider "output" / "completion" / "candidates" count
                             //   INCLUDES reasoning_tokens where the provider bills it as output
  reasoning_tokens: int      // SUBSET of output_tokens; hidden-but-billed (background "Hidden output")
  cache_read_tokens: int     // discounted input that was a cache hit
  cache_write_tokens: int    // premium input written to cache this call

  // Sum two usage records. This is the load-bearing operation for a Budget over
  // a fan-out tree: every tool step / sub-agent / retry contributes one Usage and
  // they all add. Because reasoning_tokens is a SUBSET of output_tokens, adding
  // both fields is correct and never double-counts at cost time (see PriceTable).
  function add(self, other: Usage) -> Usage {
    Usage {
      input_tokens:       self.input_tokens + other.input_tokens,
      output_tokens:      self.output_tokens + other.output_tokens,
      reasoning_tokens:   self.reasoning_tokens + other.reasoning_tokens,
      cache_read_tokens:  self.cache_read_tokens + other.cache_read_tokens,
      cache_write_tokens: self.cache_write_tokens + other.cache_write_tokens,
    }
  }

  function zero() -> Usage {
    Usage { input_tokens: 0, output_tokens: 0, reasoning_tokens: 0,
            cache_read_tokens: 0, cache_write_tokens: 0 }
  }
}

// The pair the base `call<T> -> T` structurally cannot produce: the typed value
// PLUS its measured cost. This is the single net-new value that forces a new
// capability — there is no field on `Provider` to hang usage on.
class Metered<T> {
  value: T
  usage: Usage
  model: string              // which model actually served it (a Fallback may have swapped)
  tier: PricingMode          // sync-default / flex / priority / batch — the processing axis
}


// -----------------------------------------------------------------------------
// 1. Pricing. A per-1M-token matrix, multiplied along the background's axes.
//
//    The background calls out price drift as a hard problem: a hardcoded table
//    goes stale, and region/cache/tier multiply the matrix. We model the table
//    as DATA (a provider field, see usage.baml) rather than a global, so a tenant
//    can override it, and we make the processing mode a multiplier rather than a
//    separate table — because batch is "same tokens, same model, half price".
// -----------------------------------------------------------------------------

// USD per 1,000,000 tokens, one row per (model). cache_read is the discounted
// rate; cache_write is the premium rate; reasoning is billed at the output rate
// (so there is no separate column — it is already inside output_tokens).
class PriceRow {
  input_per_mtok: float
  output_per_mtok: float
  cache_read_per_mtok: float
  cache_write_per_mtok: float
}

class PriceTable {
  rows: map<string, PriceRow>

  // Cost a Usage at a given processing mode. The four-way split mirrors the
  // background's accumulator: non-cached input, cache reads, cache writes, output.
  // reasoning_tokens is NOT added here — it is already part of output_tokens, so
  // pricing output at the output rate already bills the hidden reasoning.
  function cost(self, model: string, u: Usage, mode: PricingMode) -> Cost throws baml.errors.LlmClient {
    let row = self.rows[model] catch (e) {
      // Price drift / unknown model: we refuse to silently bill $0 (background
      // "a hard-coded table goes stale"). The caller decides whether to treat an
      // unknown price as fatal or estimate.
      _ => { throw baml.errors.LlmClient { message: "no price row for model: " ++ model }; }
    };
    // input_tokens already includes cache_read + cache_write on providers that
    // report them that way (Anthropic); normalize by subtracting the itemized
    // cached portions so we never charge full input rate for a cache hit.
    let fresh_input = u.input_tokens - u.cache_read_tokens - u.cache_write_tokens;
    let raw =
        (to_float(fresh_input)       * row.input_per_mtok
       + to_float(u.cache_read_tokens)  * row.cache_read_per_mtok
       + to_float(u.cache_write_tokens) * row.cache_write_per_mtok
       + to_float(u.output_tokens)      * row.output_per_mtok) / 1000000.0;
    Cost { usd: raw * mode_multiplier(mode), mode: mode }
  }
}

// The processing-mode axis. Batch is ~0.5x; priority is a premium for latency;
// flex is a discount for tolerating latency. Same tokens, same model.
enum PricingMode {
  SyncDefault   // full price, the synchronous path
  Flex          // service_tier: "flex" — discounted, slower sync
  Priority      // service_tier: "priority" — premium, faster sync
  Batch         // the batch API — ~50% off, separate rate pool, async
}

function mode_multiplier(mode: PricingMode) -> float {
  match (mode) {
    PricingMode.SyncDefault => 1.0,
    PricingMode.Flex        => 0.75,
    PricingMode.Priority    => 1.5,
    PricingMode.Batch       => 0.5,
  }
}

class Cost {
  usd: float
  mode: PricingMode
}

// host coercion seam — int -> float for the arithmetic above.
function to_float(n: int) -> float { $rust_io_function }


// -----------------------------------------------------------------------------
// 2. The Metering capability.
//
//    The base `call<T> -> T` throws usage away — there is no field on the return
//    to carry it. Metering is the capability that returns the (value, usage)
//    pair instead. Like every capability it `requires Provider`, so a metered
//    provider ALSO has the plain `.call` (Open Question 1: here `.call` is the
//    convenience that discards usage).
//
//    `usage_from` is the per-provider normalizer — the analog of the four
//    structured-output encodings. It is exposed so a Budget combinator that
//    wraps a stream (which delivers usage out-of-band) can normalize too.
// -----------------------------------------------------------------------------

interface Metering requires Provider {
  // The metered call: same answer as `.call`, plus the usage the base type can't
  // carry. Default-implementable on top of HttpProvider (see ChatBase below),
  // because the usage rides in the SAME HTTP response as the value.
  function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws Error

  // Normalize THIS provider's wire usage dialect into the canonical Usage. Pulled
  // out so it is reusable from streaming (usage arrives in message_start/delta)
  // and from batch (usage arrives in the downloaded result line), not only from a
  // single response body. `raw` is the provider's own usage JSON object.
  function usage_from(self, raw: string) -> Usage throws Error
}


// -----------------------------------------------------------------------------
// 3. A shared chat base that supplies metering for free on top of HttpProvider.
//
//    Most chat providers differ ONLY in (a) how usage is spelled on the wire and
//    (b) which field holds reasoning. So `call_metered` is written ONCE here over
//    HttpProvider: build the request, send, then parse BOTH the value (existing
//    `parse<T>`) and the usage (per-provider `usage_from`) out of the same body.
//
//    This requires the raw body as a string. We add a small capability seam,
//    `RawBody`, so the metering default can reach the un-parsed response text
//    that `parse<T>` consumes — without that, metering would have to re-issue the
//    request. (Flagged net-new in evaluation.md.)
// -----------------------------------------------------------------------------

// A provider whose Body can be re-exposed as a raw string for side-channel
// extraction (usage). Most HttpProviders satisfy this trivially via .text().
interface RawBody requires HttpProvider {
  function body_text(self, from: Body) -> string throws Error
}

// The default metering algorithm, mixed into any provider that is HttpProvider
// + RawBody + supplies a `usage_from`. Concrete providers `implements Metering`
// by delegating to this so the (build -> send -> parse value + parse usage) shape
// is written once.
interface ChatMetering requires Metering, RawBody {
  function model_name(self) -> string
  function pricing_mode(self) -> PricingMode

  function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws Error {
    let req  = self.build_request<T>(prompt);
    let body = self.send(req);
    let text = self.body_text(body);
    // parse the VALUE (existing seam) and the USAGE (new seam) from one body.
    let value: T = self.parse<T>(body);
    let usage    = self.usage_from(text);
    Metered<T> { value: value, usage: usage, model: self.model_name(), tier: self.pricing_mode() }
  }
}


// -----------------------------------------------------------------------------
// 4. The Budget combinator. Wraps a Provider and SUMS usage across a fan-out tree.
//
//    This is the cost-attribution piece. A single app request fans out into many
//    billed calls (tool steps, sub-agents, retries). A Budget is a Provider that
//    delegates `.call`/`call_metered` to its inner provider and accumulates every
//    Usage into one running total, enforcing a ceiling. Because a Budget is a
//    Provider, it slots into a combinator chain (retry/fallback/cache) and the
//    inner Fallback's swaps still flow through ONE accumulator.
//
//    The accumulator is MUTABLE shared state across the tree — which the
//    `client -> function -> Provider` sugar forbids (a function can't own
//    mutable state). So the running total lives in a host cell handed in at
//    construction, NOT a client field. (This is the central gap; see evaluation.)
// -----------------------------------------------------------------------------

// A host-backed mutable accumulator. One per app-request, threaded through the
// whole fan-out so every sub-call adds to the same total. `add` returns the new
// running total so the combinator can enforce the ceiling without a second read.
class BudgetCell {
  _cell: $rust_type
  function new(limit_usd: float) -> BudgetCell { $rust_io_function }
  function add(self, c: Cost) -> Cost { $rust_io_function }          // returns running total
  function total(self) -> Cost { $rust_io_function }
  function limit(self) -> float { $rust_io_function }               // the ceiling set at new()
  function usage(self) -> Usage { $rust_io_function }                // summed tokens, for chargeback
}

class Budget {
  inner: Provider
  prices: PriceTable
  cell: BudgetCell           // shared across the whole fan-out tree (NOT a client field)
  tenant: string             // for chargeback attribution

  // Budget is a Provider, so it composes with every other combinator. But the
  // plain `.call` throws usage away — so to charge, we REQUIRE the inner provider
  // to be Metering. If it isn't, we can't meter, and we say so loudly rather than
  // silently undercounting (this is the honest failure, not a fallback).
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      let m: Metered<T> = self.charge_one<T>(prompt);
      m.value
    }
  }

  // Budget is ALSO Metering: a budgeted call still returns the (value, usage)
  // pair, plus it has folded that usage into the running total and checked the
  // ceiling. This is what an app calls when it wants both the answer and the
  // per-call cost while the budget protects the whole run.
  implements Metering {
    function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws baml.errors.LlmClient {
      self.charge_one<T>(prompt)
    }
    // Budget doesn't own a wire dialect; delegate normalization to the inner
    // provider if it can normalize, else there is nothing to normalize.
    function usage_from(self, raw: string) -> Usage throws baml.errors.LlmClient {
      match (self.inner) {
        let m: Metering => m.usage_from(raw),
        _ => { throw baml.errors.LlmClient { message: "inner provider is not metering" }; }
      }
    }
  }

  // The shared charge-and-check step. Routes to the inner provider's metered
  // call, prices it, adds to the shared cell, and throws BEFORE returning if the
  // ceiling is blown — so a runaway agent loop stops mid-tree.
  function charge_one<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws baml.errors.LlmClient {
    let m: Metered<T> = match (self.inner) {
      let meter: Metering => meter.call_metered<T>(prompt),
      // The inner provider can't meter (e.g. a bare harness). We cannot honestly
      // charge it; refuse rather than bill $0 and corrupt the tenant's total.
      _ => { throw baml.errors.LlmClient {
               message: "Budget requires a Metering provider; got one that only does .call" }; }
    };
    let c = self.prices.cost(m.model, m.usage, m.tier);
    let running = self.cell.add(c);
    if (running.usd > self.cell.limit()) {
      throw baml.errors.LlmClient {
        message: "budget exceeded for tenant " ++ self.tenant
                 ++ ": $" ++ fmt_usd(running.usd) ++ " > $" ++ fmt_usd(self.cell.limit()) };
    }
    m
  }
}

function fmt_usd(x: float) -> string { $rust_io_function }


// -----------------------------------------------------------------------------
// 5. The Batch capability — submit -> poll -> download as a lifecycle.
//
//    Batch is NOT one HTTP round-trip and NOT a delivery refinement of `.call`.
//    It is a server-side lifecycle over MANY prompts at once, drawing on a
//    separate rate-limit pool, ~50% off, with results that come back UNORDERED
//    and must be rejoined by custom_id. So it earns its own capability, with a
//    server-authoritative handle (like background jobs in scenario 27) and a
//    per-request correlation id the loop never interprets.
//
//    The three providers differ at submit (OpenAI uploads a JSONL FILE then
//    creates a batch over it; Anthropic posts an INLINE array; Gemini takes
//    either) and at download (OpenAI returns a result FILE id; Anthropic streams
//    results). All of that divergence stays INSIDE the provider's Batch impl.
// -----------------------------------------------------------------------------

// One request in a batch. `custom_id` is the caller's correlation key — OPAQUE
// to the provider, used only to rejoin the unordered results to inputs. `prompt`
// is the already-rendered ast; per-request overrides (model, max_tokens) ride as
// a small map so a heterogeneous batch is possible.
class BatchRequest {
  custom_id: string
  prompt: baml.llm.PromptAst
  overrides: map<string, unknown>
}

// One result line. `custom_id` rejoins it to its BatchRequest. `value` is parsed
// iff the per-request call succeeded; `error` iff it failed (a batch can partially
// fail — some lines error while others land). Usage is per-line, so a Budget can
// still sum a batch.
class BatchResult<T> {
  custom_id: string
  value: T?
  error: string?
  usage: Usage
}

// Server-authoritative status. We mirror the provider lifecycle verbatim rather
// than invent one (the server owns this state — we only read it). `counts` lets a
// UI show progress without downloading.
enum BatchStatus {
  Validating
  InProgress
  Finalizing
  Completed
  Failed
  Expired      // the 24h window elapsed; partial results may still be downloadable
  Cancelled
}

class BatchCounts {
  total: int
  succeeded: int
  failed: int
}

// The opaque handle the app persists across disconnects — exactly like Job<T> in
// scenario 27. `owner` is the provider class that minted the id; a batch id is
// meaningless to another provider, and a Fallback cannot poll an id a different
// member created. `T` rides along so download stays typed.
class BatchHandle<T> {
  id: string                 // provider-minted, e.g. "batch_abc" / "msgbatch_123"
  owner: string
  count: int                 // how many requests were submitted (for the rejoin check)
}

class BatchState {
  status: BatchStatus
  terminal: bool
  counts: BatchCounts
}

interface Batch requires Provider {
  // Submit many requests at once. Returns IMMEDIATELY with a handle; holds no
  // connection. Per-provider this is "upload JSONL then create" (OpenAI) or
  // "post inline array" (Anthropic) — that divergence lives in the impl.
  function submit_batch<T>(self, requests: BatchRequest[]) -> BatchHandle<T> throws Error

  // Read server-authoritative status by id. A fresh GET; survives process death.
  function poll_batch<T>(self, handle: BatchHandle<T>) -> BatchState throws Error

  // Download the (UNORDERED) results once terminal. The caller rejoins by
  // custom_id. Each line carries its own usage and may carry an error.
  function download_batch<T>(self, handle: BatchHandle<T>) -> BatchResult<T>[] throws Error

  // DEFAULT: the blocking convenience — submit, poll on a cadence until terminal,
  // download, then rejoin into a map keyed by custom_id so the caller gets back a
  // dictionary instead of an unordered list. This is the ergonomic surface most
  // offline jobs want; a progress UI calls submit/poll/download itself.
  function run_batch<T>(self, requests: BatchRequest[], poll_ms: int)
      -> map<string, BatchResult<T>> throws Error {
    let handle: BatchHandle<T> = self.submit_batch<T>(requests);
    while (true) {
      let st = self.poll_batch<T>(handle);
      if (st.terminal) {
        let results = self.download_batch<T>(handle);
        let out: map<string, BatchResult<T>> = {};
        for (let r in results) {
          out[r.custom_id] = r;     // rejoin: unordered list -> keyed map
        }
        return out;
      }
      baml.sys.sleep_ms(poll_ms);
    }
    baml.sys.panic("unreachable")
  }
}


// -----------------------------------------------------------------------------
// 6. OpenAI — concrete provider: HttpProvider + Metering + Batch.
//
//    Shows the OpenAI usage dialect (prompt_tokens / completion_tokens, with
//    reasoning broken out of completion) and the FILE-based batch submit.
// -----------------------------------------------------------------------------

class OpenAI {
  model: string
  api_key: string
  service_tier: string?      // null | "flex" | "priority" — the sync pricing axis (§4 of proposal: a field)

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Opt into usage on streaming AND set service_tier here — both are wire fields.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements RawBody {
    function body_text(self, from: baml.http.Response) -> string throws baml.errors.LlmClient {
      from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } }
    }
  }

  implements ChatMetering {
    function model_name(self) -> string { self.model }
    function pricing_mode(self) -> PricingMode {
      match (self.service_tier) {
        "flex"     => PricingMode.Flex,
        "priority" => PricingMode.Priority,
        _          => PricingMode.SyncDefault,
      }
    }
  }

  // Metering: the algorithm comes from ChatMetering's default; we only owe the
  // OpenAI usage dialect. prompt_tokens/completion_tokens, with reasoning broken
  // out of completion_tokens_details and cached pulled from prompt_tokens_details.
  implements Metering {
    function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws baml.errors.LlmClient {
      // inherit the shared algorithm (build -> send -> parse value + usage_from)
      self.chat_call_metered<T>(prompt)
    }
    function usage_from(self, raw: string) -> Usage throws baml.errors.LlmClient {
      let j = baml.json.parse(raw) catch (e) { _ => { throw baml.errors.LlmClient { message: "bad usage json" }; } };
      Usage {
        input_tokens:       baml.json.int_at(j, "usage.prompt_tokens"),
        output_tokens:      baml.json.int_at(j, "usage.completion_tokens"),
        // reasoning is a SUBSET already inside completion_tokens — keep it as a sub-field.
        reasoning_tokens:   baml.json.int_at_or(j, "usage.completion_tokens_details.reasoning_tokens", 0),
        cache_read_tokens:  baml.json.int_at_or(j, "usage.prompt_tokens_details.cached_tokens", 0),
        cache_write_tokens: 0,    // OpenAI does not itemize cache writes
      }
    }
  }

  // Batch: OpenAI is the FILE path — write a JSONL of {custom_id, method, url,
  // body}, upload it, then create a batch over the file id.
  implements Batch {
    function submit_batch<T>(self, requests: BatchRequest[]) -> BatchHandle<T> throws baml.errors.LlmClient {
      // 1. render each request to a JSONL line carrying its custom_id.
      let lines: string[] = [];
      for (let r in requests) {
        let body = self.build_request<T>(r.prompt);   // reuse the SAME request builder as sync
        lines = lines ++ [baml.json.jsonl_line(r.custom_id, "/v1/chat/completions", body.body)];
      }
      // 2. upload the file (purpose: "batch"), 3. create the batch over it.
      let file_id = openai_upload_jsonl(self.api_key, baml.json.join_lines(lines));
      let batch_id = openai_create_batch(self.api_key, file_id, "/v1/chat/completions", "24h");
      BatchHandle<T> { id: batch_id, owner: "OpenAI", count: len(requests) }
    }
    function poll_batch<T>(self, handle: BatchHandle<T>) -> BatchState throws baml.errors.LlmClient {
      assert_owner(handle.owner, "OpenAI");
      openai_retrieve_batch(self.api_key, handle.id)
    }
    function download_batch<T>(self, handle: BatchHandle<T>) -> BatchResult<T>[] throws baml.errors.LlmClient {
      assert_owner(handle.owner, "OpenAI");
      // fetch the output file, then parse each JSONL line: rejoin key is custom_id,
      // value is SAP-parsed from the per-line response body, usage normalized.
      let raw_lines = openai_download_output(self.api_key, handle.id);
      let out: BatchResult<T>[] = [];
      for (let line in raw_lines) {
        let custom_id = baml.json.str_at(line, "custom_id");
        let ok = baml.json.bool_at_or(line, "response.status_code", 200) == true; // simplification
        let result = match (ok) {
          true => BatchResult<T> {
                    custom_id: custom_id,
                    value: self.parse<T>(baml.http.response_from_json(baml.json.str_at(line, "response.body"))),
                    error: null,
                    usage: self.usage_from(baml.json.str_at(line, "response.body")),
                  },
          false => BatchResult<T> {
                    custom_id: custom_id, value: null,
                    error: baml.json.str_at(line, "error.message"),
                    usage: Usage.zero(),
                  },
        };
        out = out ++ [result];
      }
      out
    }
  }

  // glue so the Metering impl can call the ChatMetering default explicitly.
  function chat_call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws baml.errors.LlmClient {
    let req  = self.build_request<T>(prompt);
    let body = self.send(req);
    let text = self.body_text(body);
    Metered<T> { value: self.parse<T>(body), usage: self.usage_from(text),
                 model: self.model, tier: self.pricing_mode() }
  }
}


// -----------------------------------------------------------------------------
// 7. Anthropic — concrete provider: HttpProvider + Metering + Batch.
//
//    Shows the DIVERGENT usage dialect (input_tokens/output_tokens +
//    cache_creation_input_tokens/cache_read_input_tokens; reasoning is NOT
//    itemized — it is folded into output_tokens) and the INLINE-array batch
//    submit (no file upload). This proves portability of the Usage normalizer and
//    exposes where it leaks (reasoning is invisible here — see evaluation).
// -----------------------------------------------------------------------------

class Anthropic {
  model: string
  api_key: string
  max_tokens: int            // bespoke, REQUIRED by Anthropic (background)
  service_tier: string?      // null | "auto" | "standard_only" (different value set than OpenAI)

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements RawBody {
    function body_text(self, from: baml.http.Response) -> string throws baml.errors.LlmClient {
      from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } }
    }
  }

  implements Metering {
    function call_metered<T>(self, prompt: baml.llm.PromptAst) -> Metered<T> throws baml.errors.LlmClient {
      let req  = self.build_request<T>(prompt);
      let body = self.send(req);
      let text = self.body_text(body);
      Metered<T> { value: self.parse<T>(body), usage: self.usage_from(text),
                   model: self.model, tier: PricingMode.SyncDefault }
    }
    function usage_from(self, raw: string) -> Usage throws baml.errors.LlmClient {
      let j = baml.json.parse(raw) catch (e) { _ => { throw baml.errors.LlmClient { message: "bad usage json" }; } };
      Usage {
        input_tokens:       baml.json.int_at(j, "usage.input_tokens"),
        output_tokens:      baml.json.int_at(j, "usage.output_tokens"),
        // LEAK: Anthropic does NOT break out reasoning. We cannot fill this in;
        // it is folded into output_tokens. So `reasoning_tokens` is 0 here even
        // when extended thinking ran — the normalized field silently under-reports
        // (see evaluation "Where it leaks").
        reasoning_tokens:   0,
        // Anthropic itemizes BOTH a cache write (creation) and a cache read.
        cache_read_tokens:  baml.json.int_at_or(j, "usage.cache_read_input_tokens", 0),
        cache_write_tokens: baml.json.int_at_or(j, "usage.cache_creation_input_tokens", 0),
      }
    }
  }

  // Batch: Anthropic is the INLINE path — post an array of {custom_id, params}.
  // No file upload; results are STREAMED back rather than a file id.
  implements Batch {
    function submit_batch<T>(self, requests: BatchRequest[]) -> BatchHandle<T> throws baml.errors.LlmClient {
      let objs: string[] = [];
      for (let r in requests) {
        let body = self.build_request<T>(r.prompt);
        // inline object: {"custom_id": ..., "params": <the messages body>}
        objs = objs ++ [baml.json.batch_inline_obj(r.custom_id, body.body)];
      }
      let batch_id = anthropic_create_batch(self.api_key, baml.json.array(objs));
      BatchHandle<T> { id: batch_id, owner: "Anthropic", count: len(requests) }
    }
    function poll_batch<T>(self, handle: BatchHandle<T>) -> BatchState throws baml.errors.LlmClient {
      assert_owner(handle.owner, "Anthropic");
      anthropic_retrieve_batch(self.api_key, handle.id)
    }
    function download_batch<T>(self, handle: BatchHandle<T>) -> BatchResult<T>[] throws baml.errors.LlmClient {
      assert_owner(handle.owner, "Anthropic");
      let raw_lines = anthropic_stream_results(self.api_key, handle.id);
      let out: BatchResult<T>[] = [];
      for (let line in raw_lines) {
        let custom_id = baml.json.str_at(line, "custom_id");
        // Anthropic result type: "succeeded" | "errored" | "canceled" | "expired"
        let kind = baml.json.str_at(line, "result.type");
        let result = match (kind) {
          "succeeded" => BatchResult<T> {
                    custom_id: custom_id,
                    value: self.parse<T>(baml.http.response_from_json(baml.json.str_at(line, "result.message"))),
                    error: null,
                    usage: self.usage_from(baml.json.str_at(line, "result.message")),
                  },
          _ => BatchResult<T> {
                    custom_id: custom_id, value: null,
                    error: "batch line " ++ kind, usage: Usage.zero(),
                  },
        };
        out = out ++ [result];
      }
      out
    }
  }
}


// -----------------------------------------------------------------------------
// 8. Shared host seams. Genuine boundaries only — usage normalization above is
//    pure BAML on top of these JSON accessors.
// -----------------------------------------------------------------------------

function assert_owner(actual: string, expected: string) -> null throws baml.errors.LlmClient {
  if (actual != expected) {
    throw baml.errors.LlmClient {
      message: "batch handle owned by " ++ actual ++ " cannot be polled by " ++ expected };
  }
  null
}

function len<T>(xs: T[]) -> int { $rust_io_function }

// host: OpenAI batch transport
function openai_upload_jsonl(api_key: string, jsonl: string) -> string { $rust_io_function }
function openai_create_batch(api_key: string, file_id: string, endpoint: string, window: string) -> string { $rust_io_function }
function openai_retrieve_batch(api_key: string, batch_id: string) -> BatchState { $rust_io_function }
function openai_download_output(api_key: string, batch_id: string) -> string[] { $rust_io_function }

// host: Anthropic batch transport
function anthropic_create_batch(api_key: string, requests_json: string) -> string { $rust_io_function }
function anthropic_retrieve_batch(api_key: string, batch_id: string) -> BatchState { $rust_io_function }
function anthropic_stream_results(api_key: string, batch_id: string) -> string[] { $rust_io_function }
