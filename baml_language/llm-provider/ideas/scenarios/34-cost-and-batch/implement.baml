// =============================================================================
// implement.baml — Cost & tokens + batch / async inference
//
// Library / std-lib author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, Streaming, ResponseMeta) and
// 01-providers-clients-capabilities.md. Two intertwined problems live here:
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
// The teaching points are: (1) usage is a PRODUCT SIDECAR (value AND metadata),
// so it rides the canonical `call_with` + `ResponseMeta` seam owned by the
// `HttpProvider` capability (the marker `Provider` carries no interaction — see
// ../provider-as-marker.md) — NOT a bespoke `Metered<T>`/`call_metered` companion
// (the prior draft invented one; the value+sidecar model retired it); (2) usage normalization is
// per-provider, so it lives INSIDE the provider's `ResponseMeta` impl (OpenAIResponse/
// AnthropicResponse) like the four structured-output encodings; (3) batch is
// server-authoritative state — a SUM outcome (the handle REPLACES the answer), so
// it is explicitly NOT `call_with`: it stays a submit/poll/download lifecycle the
// existential `Provider` cannot promise at compile time; (4) a Budget SUMS usage
// across a fan-out tree against mutable host state — also explicitly NOT a sidecar
// (`call_with` projects ONE response's meta; a running total over many calls is
// stateful), so it stays a hand-threaded combinator.
//
// Net-new surface this file introduces (flagged in evaluation.md):
//   - `Usage` (normalized) + `PriceTable` + `Cost`            (metering vocab)
//   - usage as a `ResponseMeta` dimension, projected via `call_with` (NO `Metered<T>`)
//   - a `Budget` combinator that wraps a Provider and SUMS over a fan-out tree
//     (mutable host state — a SUM/stateful outcome, NOT a `call_with` sidecar)
//   - the `Batch` capability interface (submit/poll/download lifecycle — a SUM
//     outcome: the handle replaces the answer, NOT a `call_with` sidecar)
//   - `BatchHandle` / `BatchJob` / `BatchState` server-authoritative types
//   - host seams: baml.sys.now_ms, baml.sys.sleep, baml.json.{from_json,stringify}, file upload
// =============================================================================


// -----------------------------------------------------------------------------
// Error model. Usage itself is a PRODUCT SIDECAR now: it rides `call_with` +
// `ResponseMeta` on the `HttpProvider` capability, so reading usage fails on the
// canonical `CallError` channel like any other `.call`/`call_with` — there is no separate
// "metering call" channel anymore (the bespoke `call_metered` it once justified is
// gone). What remains genuinely off to the side is the MONEY path and the BATCH
// lifecycle, so two capability error interfaces survive (flagged net-new in
// evaluation.md), shaped like the canonical CallError — independent interface,
// classifier methods, declared as the channel `baml.ExtendUnknownError<baml.errors.<Cap>Error>`.
//
//   - MeteringError: the PRICING / BUDGET path (PriceTable.cost on a stale table,
//     and the Budget combinator's ceiling-blown abort). This is NOT the usage-
//     carrier path (that is now CallError, via call_with); it is the cost arithmetic
//     and the running-total enforcement, which a wire call never sees. A
//     BudgetExceeded surfaces on the plain `.call` Budget forwards, so it is ALSO a
//     CallError there.
//   - BatchError: the submit/poll/download lifecycle, which is NOT one HTTP round
//     trip — its own rate pool, its own terminal states, its own failure modes.
// -----------------------------------------------------------------------------

// MeteringError is now the MONEY channel only: the two failure modes that live in
// the pricing/budget layer, not on the wire (usage-carrier failures are CallError).
// We keep network/rate-limit classifiers so a BudgetExceeded that ALSO crosses into
// the plain `.call` path can be triaged uniformly with transport errors there.
interface baml.errors.MeteringError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
  function is_budget_exceeded(self) -> bool   // the ceiling-blown case, the Budget combinator's abort
  function is_unknown_price(self) -> bool     // price drift / no row for this model (PriceTable.cost)
}

interface baml.errors.BatchError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_wrong_owner(self) -> bool       // an id minted by a different provider
  function is_partial_failure(self) -> bool   // some lines errored while others landed
}

// NET-NEW REQUIREMENT on a spec type: because the pricing/budget and Batch paths
// keep their own capability channels, `baml.errors.Unsupported` (the capability-
// negotiation error a `_` arm throws) must ALSO implement `MeteringError` and
// `BatchError` to be legal on those channels — its canonical definition in
// _conventions.md only covers Call/Stream/Tool/Realtime. usage.baml throws
// `baml.errors.Unsupported` on the BatchError channel (the `_` arms of
// `match (p) { let b: Batch => ... }`), and `Budget.charge_one` throws it on the
// MeteringError channel when the inner provider cannot be priced. (Usage itself is
// not a bespoke capability to negotiate — any `HttpProvider` answers `meta.usage()`
// via `call_with`, matched by the basic-call/`call_with` companion like any other
// request/response, so there is no `Unsupported` on a "metering" channel anymore.) So the
// spec's `Unsupported` is extended here. Every classifier is false: an unsupported
// provider has no network, rate-limit, budget, price, ownership, or partial-failure
// error — it simply can't.
extend baml.errors.Unsupported {
  implements baml.errors.MeteringError {
    function is_network_error(self)   -> bool { return false; }
    function is_rate_limit(self)      -> bool { return false; }
    function is_parse_error(self)     -> bool { return false; }
    function is_budget_exceeded(self) -> bool { return false; }
    function is_unknown_price(self)   -> bool { return false; }
  }
  implements baml.errors.BatchError {
    function is_network_error(self)   -> bool { return false; }
    function is_rate_limit(self)      -> bool { return false; }
    function is_wrong_owner(self)     -> bool { return false; }
    function is_partial_failure(self) -> bool { return false; }
  }
}

// Price drift / unknown model. We refuse to silently bill $0 (background "a hard
// coded table goes stale"); this is a deliberate domain failure, not a foreign one.
class UnknownPriceError {
  model: string
  implements baml.errors.MeteringError {
    function is_network_error(self)   -> bool { return false; }
    function is_rate_limit(self)      -> bool { return false; }
    function is_parse_error(self)     -> bool { return false; }
    function is_budget_exceeded(self) -> bool { return false; }
    function is_unknown_price(self)   -> bool { return true; }
  }
}

// The ceiling was blown mid-fan-out. A deliberate stop, carrying the attribution
// the app reports. Surfaces on both the metered path and the plain `.call` Budget
// forwards, so it implements CallError too.
class BudgetExceeded {
  tenant: string
  spent_usd: float
  limit_usd: float
  implements baml.errors.MeteringError {
    function is_network_error(self)   -> bool { return false; }
    function is_rate_limit(self)      -> bool { return false; }
    function is_parse_error(self)     -> bool { return false; }
    function is_budget_exceeded(self) -> bool { return true; }
    function is_unknown_price(self)   -> bool { return false; }
  }
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// A batch id minted by one provider being handed to another (a Fallback cannot
// poll an id a different member created). A deliberate ownership-violation error.
class WrongBatchOwner {
  actual: string
  expected: string
  implements baml.errors.BatchError {
    function is_network_error(self)   -> bool { return false; }
    function is_rate_limit(self)      -> bool { return false; }
    function is_wrong_owner(self)     -> bool { return true; }
    function is_partial_failure(self) -> bool { return false; }
  }
}

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

// NOTE: the prior draft invented a bespoke `Metered<T> { value, usage, model, tier }`
// wrapper here and a parallel `call_metered<T> -> Metered<T>` companion to carry it.
// The value+sidecar model retires both: usage is a PRODUCT SIDECAR (you get the value
// AND its usage), so it rides the canonical `call_with` + `ResponseMeta` seam owned by
// the `HttpProvider` capability. The (value, usage) pair is just the tuple `call_with` already
// returns; `model` and `tier` are two more `ResponseMeta` dimensions (which model
// actually served — a Fallback may have swapped — and the processing tier the wire
// response reflects). See `MeteredMeta` below for those two extra dimensions, and
// `Budget.charge_one` for the projection that reads all three in one pass.


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
  function cost(self, model: string, u: Usage, mode: PricingMode) -> Cost
      throws baml.ExtendUnknownError<baml.errors.MeteringError> {
    // Price drift / unknown model: we refuse to silently bill $0 (background
    // "a hard-coded table goes stale"). `Map.get` returns `PriceRow?`; a null
    // (no row) becomes a deliberate UnknownPriceError. The caller decides whether
    // to treat an unknown price as fatal or estimate.
    let row: PriceRow = match (self.rows.get(model)) {
      let r: PriceRow => r,
      null => { throw UnknownPriceError { model: model }; },
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
// 2. Usage as a ResponseMeta dimension (NO bespoke capability).
//
//    Usage is a PRODUCT SIDECAR: a response is "value AND its usage." The plain
//    `call<T> -> T` cannot carry it — but the `HttpProvider` capability that owns
//    `call` ALSO ships `call_with` + `ResponseMeta` exactly for this (_conventions.md
//    "Value + sidecar model"; ../provider-as-marker.md moves both onto HttpProvider).
//    So there is no `Metering` capability, no `call_metered`, no `Metered<T>`:
//    any `HttpProvider` already answers `meta.usage()`, and the app reads it by
//    matching the client's provider for `HttpProvider` and projecting through `call_with`:
//
//        let (ticket, usage): (Ticket, Usage) = Classify.call_with(text, m => m.usage());
//
//    Because `call_with` lives on the SAME capability as `call`, a Fallback/Retry/Cache
//    that forwards request/response FORWARDS it too — the usage of the winning member
//    is NOT silently dropped (the bug the bespoke companion had: a Fallback forwarding
//    `.call` degraded to zero usage).
//
//    The per-provider normalizer — the analog of the four structured-output
//    encodings — is now the `usage()` body INSIDE each provider's `ResponseMeta`
//    impl (OpenAIResponse / AnthropicResponse below). It reads the SAME wire
//    response `call_with` already parsed the value out of, lazily, so a caller who
//    projects only `m.usage()` never parses anything else.
//
//    Two cost-specific dimensions — WHICH model actually served (a Fallback may
//    have swapped) and the processing TIER the response reflects — are not in the
//    canonical `ResponseMeta`, so they ride a small EXTERNAL-impl interface
//    `MeteredMeta` (the §7-of-the-proposal pattern: a novel dimension added by
//    runtime `match`). A Budget projects all three (usage, model, tier) in ONE pass.
// -----------------------------------------------------------------------------

// The two pricing-specific dimensions a wire response carries beyond canonical usage.
// External impl on each provider's ResponseMeta (OpenAIResponse / AnthropicResponse),
// reached by `match (m) { let mm: MeteredMeta => ... }` — exactly like `Grounded` in
// the value+sidecar proposal §7.
interface MeteredMeta {
  function served_model(self) -> string     // which model actually answered (Fallback-swap aware)
  function tier(self) -> PricingMode         // the processing tier the response reflects
}


// -----------------------------------------------------------------------------
// 4. The Budget combinator. Wraps a Provider and SUMS usage across a fan-out tree.
//
//    This is the cost-attribution piece, and it is the canonical case the
//    value+sidecar model leaves OFF `call_with` on purpose: a SUM / STATEFUL
//    round-trip, not a product sidecar. `call_with` projects ONE response's
//    `ResponseMeta`; a running total over many calls (tool steps, sub-agents,
//    retries) is mutable state across the tree, which no return-shape carries.
//    So Budget stays a hand-threaded combinator — a request/response forwarder that
//    `implements HttpProvider` (so it owns `call`/`call_with`) and is still a
//    `Provider` via the marker (NOT itself expressed as `call_with`).
//
//    What DID convert: Budget no longer reaches into a bespoke `Metered<T>` /
//    `call_metered`. It reads each sub-call's (usage, model, tier) by PROJECTING
//    the inner provider's `call_with` (matched for `HttpProvider`) — the same seam an
//    app uses. Because `call_with` is forwarded by Fallback/Retry/Cache, an inner Fallback's model
//    swap still flows through the ONE accumulator (the model is read off
//    `ResponseMeta`, not assumed).
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

// What a Budget projects off ONE response in a single `call_with` pass: the three
// pricing inputs. `model`/`tier` come from the `MeteredMeta` external dimension;
// `usage` from canonical `ResponseMeta`. Defaults guard a provider whose response
// carries no MeteredMeta — Unsupported is folded into a refusal-to-bill below.
class Charge {
  usage: Usage
  model: string
  tier: PricingMode
  priced: bool               // false iff MeteredMeta was absent (cannot honestly bill)
}

class Budget {
  inner: Provider
  prices: PriceTable
  cell: BudgetCell           // shared across the whole fan-out tree (NOT a client field)
  tenant: string             // for chargeback attribution

  // Budget forwards the request/response surface, so it `implements HttpProvider`
  // (the marker `Provider` carries no `call`/`call_with` — those belong to the
  // capability that owns the request/response shape, see ../provider-as-marker.md).
  // It composes with every other combinator because `requires Provider` (inherited
  // via HttpProvider) still gives it the combinator factories. `call` delegates to
  // the inner provider's `call_with`, projecting the (usage, model, tier) it needs
  // to charge — so the usage the request/response capability DOES carry is no longer
  // thrown away on the budgeted path (the old draft's `.call` discarded it). `call_with` is the
  // P1 fix: Budget does NOT fabricate codec methods (build_request/send/parse/
  // meta_of) — it is a pure forwarder, so it routes call_with to the inner member's
  // call_with rather than running a codec pass of its own.
  implements HttpProvider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (value, _): (T, Charge) = self.charge_one<T>(prompt);
      value
    } catch (e) {
      // charge_one charges on the MeteringError channel; BudgetExceeded implements
      // BOTH MeteringError and CallError so it passes through as a known CallError.
      // Anything else foreign to CallError is normalized.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "budgeted call failed");
    }

    // Budget ALSO forwards `call_with` (the request/response sidecar seam), so an app
    // that wants both the answer AND its OWN projected sidecar while the budget
    // protects the run gets it — and the winning member's usage is not dropped (the
    // P1 fix: a combinator FORWARDS call_with to the member it routes to). ONE inner
    // call: we project BOTH the Charge (to bill) and the caller's U (to return) off
    // the SAME winning ResponseMeta in a single pass — the combinator-forwards-
    // call_with rule (no silent drop, no double request).
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The marker `Provider` carries no call_with, so we match the inner member for
      // the request/response capability and forward to it (the P1 combinator-forward).
      match (self.inner) {
        let h: HttpProvider => {
          let (value, both): (T, (Charge, U)) =
            h.call_with<T, (Charge, U)>(prompt, m => (self.charge_of(m), project(m)));
          self.enforce(both.0);     // price, accumulate, and throw BudgetExceeded if blown
          return (value, both.1);
        },
        _ => throw baml.errors.Unsupported {
          message: "Budget inner provider is not request/response (HttpProvider)" };
      }
    } catch (e) {
      // inner.call_with charges on the CallError channel; BudgetExceeded implements
      // BOTH MeteringError and CallError so it passes through as a known CallError.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "budgeted call_with failed");
    }
  }

  // The shared charge-and-check step backing `.call`. ONE inner `call_with` whose
  // projection reads (usage, model, tier); we then price, accumulate, and throw
  // BEFORE returning if the ceiling is blown — so a runaway agent loop stops
  // mid-tree. Returns (value, Charge) so the caller keeps the answer too.
  function charge_one<T>(self, prompt: baml.llm.PromptAst) -> (T, Charge)
      throws baml.ExtendUnknownError<baml.errors.MeteringError> {
    // The marker `Provider` carries no call_with; match the inner member for the
    // request/response capability and project (usage, model, tier) off its call_with.
    let (value, ch): (T, Charge) = match (self.inner) {
      let h: HttpProvider => h.call_with<T, Charge>(prompt, m => self.charge_of(m)),
      _ => throw baml.errors.Unsupported {
        message: "Budget inner provider is not request/response (HttpProvider)" },
    } catch (e) {
      // inner.call_with declares CallError; foreign to MeteringError, so normalize.
      _ => throw baml.UnknownError.with_message<baml.errors.MeteringError>(e, "budgeted sub-call failed");
    };
    self.enforce(ch);
    (value, ch)
  }

  // Project the three pricing inputs off ONE ResponseMeta: usage off canonical
  // ResponseMeta, model/tier off the MeteredMeta external dimension. A provider
  // whose response is not MeteredMeta (e.g. a bare harness) cannot be honestly
  // priced — priced=false, refused in `enforce` rather than billed $0.
  function charge_of(self, m: ResponseMeta) -> Charge {
    match (m) {
      let mm: MeteredMeta => Charge { usage: m.usage(), model: mm.served_model(),
                                      tier: mm.tier(), priced: true },
      _ => Charge { usage: m.usage(), model: "", tier: PricingMode.SyncDefault, priced: false },
    }
  }

  // Price a Charge, fold it into the shared cell, and abort if the ceiling is blown.
  function enforce(self, ch: Charge) -> null
      throws baml.ExtendUnknownError<baml.errors.MeteringError> {
    if (ch.priced == false) {
      throw baml.errors.Unsupported {
        message: "Budget requires a provider whose response carries pricing meta (model/tier)" };
    }
    let c = self.prices.cost(ch.model, ch.usage, ch.tier);
    let running = self.cell.add(c);
    if (running.usd > self.cell.limit()) {
      throw BudgetExceeded {
        tenant: self.tenant, spent_usd: running.usd, limit_usd: self.cell.limit() };
    }
    null
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
  function submit_batch<T>(self, requests: BatchRequest[]) -> BatchHandle<T>
      throws baml.ExtendUnknownError<baml.errors.BatchError>

  // Read server-authoritative status by id. A fresh GET; survives process death.
  function poll_batch<T>(self, handle: BatchHandle<T>) -> BatchState
      throws baml.ExtendUnknownError<baml.errors.BatchError>

  // Download the (UNORDERED) results once terminal. The caller rejoins by
  // custom_id. Each line carries its own usage and may carry an error.
  function download_batch<T>(self, handle: BatchHandle<T>) -> BatchResult<T>[]
      throws baml.ExtendUnknownError<baml.errors.BatchError>

  // DEFAULT: the blocking convenience — submit, poll on a cadence until terminal,
  // download, then rejoin into a map keyed by custom_id so the caller gets back a
  // dictionary instead of an unordered list. This is the ergonomic surface most
  // offline jobs want; a progress UI calls submit/poll/download itself.
  function run_batch<T>(self, requests: BatchRequest[], poll_ms: int)
      -> map<string, BatchResult<T>>
      throws baml.ExtendUnknownError<baml.errors.BatchError> {
    let handle: BatchHandle<T> = self.submit_batch<T>(requests);
    while (true) {
      let st = self.poll_batch<T>(handle);
      if (st.terminal) {
        let results = self.download_batch<T>(handle);
        let out: map<string, BatchResult<T>> = {};
        for (let r in results) {
          out.set(r.custom_id, r);     // rejoin: unordered list -> keyed map
        }
        return out;
      }
      baml.sys.sleep(poll_ms);
    }
    baml.sys.panic("unreachable")
  }
}


// -----------------------------------------------------------------------------
// 6. OpenAI — concrete provider: HttpProvider (with ResponseMeta) + Batch.
//
//    Shows the OpenAI usage dialect (prompt_tokens / completion_tokens, with
//    reasoning broken out of completion) — now normalized INSIDE the provider's
//    `ResponseMeta` (OpenAIResponse.usage()) and reached via `call_with`, not a
//    bespoke `call_metered`. Also the FILE-based batch submit.
// -----------------------------------------------------------------------------

// Typed wire shape for the OpenAI usage envelope. We decode the response body
// into this class with `baml.json.from_json<OAIWire>` and read fields directly —
// the canonical "read a response" move (NO `Response.json_path`, which is invented;
// see _conventions.md). Every field is optional-with-default because providers omit
// fields and a streamed response may carry no `usage` block at all.
class OAITokenDetails {
  reasoning_tokens: int?
  cached_tokens: int?
}
class OAIUsageWire {
  prompt_tokens: int?
  completion_tokens: int?
  completion_tokens_details: OAITokenDetails?
  prompt_tokens_details: OAITokenDetails?
}
class OAIChoiceWire {
  finish_reason: string?
}
class OAIWire {
  usage: OAIUsageWire?
  choices: OAIChoiceWire[]?
}

// Typed wire shape for ONE downloaded OpenAI batch-output line. `response.body` is
// left as raw `json` so it can be re-coerced both to the caller's `T` and to
// `OAIWire` (for per-line usage) without committing the line class to a generic.
class OAIBatchResponse {
  status_code: int?
  body: json?
}
class OAIBatchError {
  message: string?
}
class OAIBatchLine {
  custom_id: string?
  response: OAIBatchResponse?
  error: OAIBatchError?
}

// Small null-coalescers so the lazy readers stay one-liners.
function oai_int_or(v: int?, fallback: int) -> int {
  match (v) { let n: int => n, null => fallback }
}

// Normalize the OpenAI usage dialect off a decoded wire envelope. Shared by the
// sync path (OpenAIResponse.usage) and the batch download path (which decodes each
// per-line body into the SAME OAIWire). One normalizer, two callers.
function oai_usage_of(w: OAIWire) -> Usage {
  let u: OAIUsageWire = match (w.usage) {
    let uu: OAIUsageWire => uu,
    null => OAIUsageWire { prompt_tokens: null, completion_tokens: null,
                           completion_tokens_details: null, prompt_tokens_details: null },
  };
  let comp_details: OAITokenDetails = match (u.completion_tokens_details) {
    let d: OAITokenDetails => d, null => OAITokenDetails { reasoning_tokens: null, cached_tokens: null },
  };
  let prompt_details: OAITokenDetails = match (u.prompt_tokens_details) {
    let d: OAITokenDetails => d, null => OAITokenDetails { reasoning_tokens: null, cached_tokens: null },
  };
  Usage {
    input_tokens:       oai_int_or(u.prompt_tokens, 0),
    output_tokens:      oai_int_or(u.completion_tokens, 0),
    // reasoning is a SUBSET already inside completion_tokens — keep it a sub-field.
    reasoning_tokens:   oai_int_or(comp_details.reasoning_tokens, 0),
    cache_read_tokens:  oai_int_or(prompt_details.cached_tokens, 0),
    cache_write_tokens: 0,    // OpenAI does not itemize cache writes
  }
}

// The provider's normalized response sidecar. Holds the wire response and the two
// pricing dimensions the provider config supplies (which model, which tier), and
// normalizes the OpenAI usage dialect LAZILY in usage(). `call_with`'s `project`
// runs over this; if it only asks for usage(), nothing else is parsed.
class OpenAIResponse {
  raw: baml.http.Response
  model_served: string
  tier_mode: PricingMode

  // Decode the wire body into the typed OAIWire envelope. Coercion / IO failure
  // degrades to an empty envelope so the lazy ResponseMeta readers stay total
  // (their signatures declare no throws); a missing field then reads as 0/"".
  function wire(self) -> OAIWire {
    baml.json.from_json<OAIWire>(self.raw.text()) catch (e) {
      _ => OAIWire { usage: null, choices: null }
    }
  }

  implements ResponseMeta {
    function usage(self) -> Usage {
      // prompt_tokens/completion_tokens, with reasoning broken out of
      // completion_tokens_details and cached pulled from prompt_tokens_details.
      oai_usage_of(self.wire())
    }
    function finish_reason(self) -> string {
      match (self.wire().choices) {
        let cs: OAIChoiceWire[] => match (cs.at(0)) {
          let c: OAIChoiceWire => match (c.finish_reason) { let r: string => r, null => "" },
          null => "",
        },
        null => "",
      }
    }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "not requested" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "openai chat" } }
    function reasoning(self)      -> Supported<string>    { Unsupported { reason: "gpt-4" } }
  }
  // the cost-specific dimensions — external impl, reached by `match (m) { let mm: MeteredMeta => ... }`.
  implements MeteredMeta {
    function served_model(self) -> string { self.model_served }
    function tier(self) -> PricingMode    { self.tier_mode }
  }
}

class OpenAI {
  model: string
  api_key: string
  service_tier: string?      // null | "flex" | "priority" — the sync pricing axis (§4 of proposal: a field)

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Opt into usage on streaming AND set service_tier here — both are wire fields.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
      }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
    // Normalize the sidecar (the value+sidecar seam). The same wire response
    // `parse` consumes is wrapped as an OpenAIResponse carrying the two pricing
    // dimensions; usage normalization is LAZY inside OpenAIResponse.usage(). The
    // inherited HttpProvider.call_with does ONE codec pass: (parse<T>, meta_of).
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAIResponse { raw: from, model_served: self.model, tier_mode: self.pricing_mode() }
    }
  }

  // service_tier -> the processing-mode axis. A plain helper (no capability).
  function pricing_mode(self) -> PricingMode {
    match (self.service_tier) {
      "flex"     => PricingMode.Flex,
      "priority" => PricingMode.Priority,
      _          => PricingMode.SyncDefault,
    }
  }

  // Batch: OpenAI is the FILE path — write a JSONL of {custom_id, method, url,
  // body}, upload it, then create a batch over the file id.
  implements Batch {
    function submit_batch<T>(self, requests: BatchRequest[]) -> BatchHandle<T>
        throws baml.ExtendUnknownError<baml.errors.BatchError> {
      // 1. render each request to a JSONL line carrying its custom_id. Each line
      // is a native `map<string, json>` literal serialized with baml.json.stringify;
      // the request body (itself a JSON string) is parsed back into a json value so
      // it nests as an object, not a re-escaped string.
      let lines: string[] = [];
      for (let r in requests) {
        let body = self.build_request<T>(r.prompt);   // reuse the SAME request builder as sync
        let line_obj: map<string, json> = {
          "custom_id": r.custom_id,
          "method":    "POST",
          "url":       "/v1/chat/completions",
          "body":      baml.json.parse(body.body),
        };
        lines.push(baml.json.stringify(line_obj));
      }
      // 2. upload the file (purpose: "batch"), 3. create the batch over it. JSONL is
      // just newline-joined object lines.
      let file_id = openai_upload_jsonl(self.api_key, lines.join("\n"));
      let batch_id = openai_create_batch(self.api_key, file_id, "/v1/chat/completions", "24h");
      BatchHandle<T> { id: batch_id, owner: "OpenAI", count: requests.length() }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BatchError>(e, "openai submit_batch failed");
    }
    function poll_batch<T>(self, handle: BatchHandle<T>) -> BatchState
        throws baml.ExtendUnknownError<baml.errors.BatchError> {
      assert_owner(handle.owner, "OpenAI");
      openai_retrieve_batch(self.api_key, handle.id)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BatchError>(e, "openai poll_batch failed");
    }
    function download_batch<T>(self, handle: BatchHandle<T>) -> BatchResult<T>[]
        throws baml.ExtendUnknownError<baml.errors.BatchError> {
      assert_owner(handle.owner, "OpenAI");
      // fetch the output file, then parse each JSONL line by decoding it into the
      // typed OAIBatchLine envelope (NO Response.json_path): rejoin key is custom_id,
      // value is coerced from the per-line response body, usage normalized by the
      // SAME oai_usage_of the sync path uses (over the inner body decoded as OAIWire).
      let raw_lines = openai_download_output(self.api_key, handle.id);
      let out: BatchResult<T>[] = [];
      for (let line in raw_lines) {
        let parsed: OAIBatchLine = baml.json.from_json<OAIBatchLine>(line);
        let custom_id: string = match (parsed.custom_id) { let s: string => s, null => "" };
        let resp: OAIBatchResponse = match (parsed.response) {
          let rr: OAIBatchResponse => rr, null => OAIBatchResponse { status_code: null, body: null },
        };
        let status: int = match (resp.status_code) { let n: int => n, null => 0 };
        let result: BatchResult<T> = match (resp.body) {
          let body: json => {
            // the per-line response body is structured JSON; coerce it to T and to
            // OAIWire for usage. Reuse the sync-path normalizer.
            BatchResult<T> {
              custom_id: custom_id,
              value:     baml.json.from_json<T>(body),
              error:     null,
              usage:     oai_usage_of(baml.json.from_json<OAIWire>(body)),
            }
          },
          null => BatchResult<T> {
            custom_id: custom_id, value: null,
            error: match (parsed.error) {
              let e: OAIBatchError => match (e.message) { let m: string => m, null => "openai batch line failed" },
              null => "openai batch line failed (status " + status.to_string() + ")",
            },
            usage: Usage.zero(),
          },
        };
        out.push(result);
      }
      out
    } catch (e) {
      // JSON coercion failures (JsonParseError/JsonDecodeError from from_json) are
      // foreign to BatchError; normalize. A WrongBatchOwner passes through untouched
      // (it is a known BatchError).
      _ => throw baml.UnknownError.with_message<baml.errors.BatchError>(e, "openai download_batch failed");
    }
  }
}


// -----------------------------------------------------------------------------
// 7. Anthropic — concrete provider: HttpProvider (with ResponseMeta) + Batch.
//
//    Shows the DIVERGENT usage dialect (input_tokens/output_tokens +
//    cache_creation_input_tokens/cache_read_input_tokens; reasoning is NOT
//    itemized — it is folded into output_tokens) — normalized INSIDE
//    AnthropicResponse.usage(), reached via `call_with`. Also the INLINE-array
//    batch submit (no file upload). This proves portability of the Usage
//    normalizer across two ResponseMeta impls and exposes where it leaks
//    (reasoning is invisible here — see evaluation).
// -----------------------------------------------------------------------------

// Typed wire shape for the Anthropic usage envelope. Same "decode the body, read
// fields" move as OAIWire (NO Response.json_path). `stop_reason` rides along for
// finish_reason().
class AntUsageWire {
  input_tokens: int?
  output_tokens: int?
  cache_read_input_tokens: int?
  cache_creation_input_tokens: int?
}
class AntWire {
  usage: AntUsageWire?
  stop_reason: string?
}

// Typed wire shape for ONE streamed Anthropic batch result. `result.message` is
// left as raw `json` so it can be re-coerced both to `T` and to `AntWire` (usage).
class AntBatchResultWire {
  type: string?
  message: json?
}
class AntBatchLine {
  custom_id: string?
  result: AntBatchResultWire?
}

// Normalize the Anthropic usage dialect off a decoded wire envelope. Shared by the
// sync path (AnthropicResponse.usage) and the batch download path.
function ant_usage_of(w: AntWire) -> Usage {
  let u: AntUsageWire = match (w.usage) {
    let uu: AntUsageWire => uu,
    null => AntUsageWire { input_tokens: null, output_tokens: null,
                           cache_read_input_tokens: null, cache_creation_input_tokens: null },
  };
  Usage {
    input_tokens:       oai_int_or(u.input_tokens, 0),
    output_tokens:      oai_int_or(u.output_tokens, 0),
    // LEAK: Anthropic does NOT break out reasoning. We cannot fill this in;
    // it is folded into output_tokens. So `reasoning_tokens` is 0 here even
    // when extended thinking ran — the normalized field silently under-reports
    // (see evaluation "Where it leaks").
    reasoning_tokens:   0,
    // Anthropic itemizes BOTH a cache write (creation) and a cache read.
    cache_read_tokens:  oai_int_or(u.cache_read_input_tokens, 0),
    cache_write_tokens: oai_int_or(u.cache_creation_input_tokens, 0),
  }
}

// Anthropic's normalized response sidecar — the second ResponseMeta impl, proving
// the per-provider normalizer ports. Anthropic is always SyncDefault on the wire
// (no flex/priority axis), so tier_mode is fixed at construction.
class AnthropicResponse {
  raw: baml.http.Response
  model_served: string
  tier_mode: PricingMode

  // Decode the wire body into the typed AntWire envelope; IO/coercion failure
  // degrades to an empty envelope so the lazy readers stay total.
  function wire(self) -> AntWire {
    baml.json.from_json<AntWire>(self.raw.text()) catch (e) {
      _ => AntWire { usage: null, stop_reason: null }
    }
  }

  implements ResponseMeta {
    function usage(self) -> Usage { ant_usage_of(self.wire()) }
    function finish_reason(self) -> string {
      match (self.wire().stop_reason) { let r: string => r, null => "" }
    }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "anthropic: none" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "not requested" } }
    function reasoning(self)     -> Supported<string>     { Unsupported { reason: "not itemized" } }
  }
  implements MeteredMeta {
    function served_model(self) -> string { self.model_served }
    function tier(self) -> PricingMode    { self.tier_mode }
  }
}

class Anthropic {
  model: string
  api_key: string
  max_tokens: int            // bespoke, REQUIRED by Anthropic (background)
  service_tier: string?      // null | "auto" | "standard_only" (different value set than OpenAI)

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
      }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      AnthropicResponse { raw: from, model_served: self.model, tier_mode: PricingMode.SyncDefault }
    }
  }

  // Batch: Anthropic is the INLINE path — post an array of {custom_id, params}.
  // No file upload; results are STREAMED back rather than a file id.
  implements Batch {
    function submit_batch<T>(self, requests: BatchRequest[]) -> BatchHandle<T>
        throws baml.ExtendUnknownError<baml.errors.BatchError> {
      // Build a native `json[]` of inline objects, then serialize the whole array.
      // Each element is a `map<string, json>` literal; the request body (a JSON
      // string) is parsed back so it nests as an object, not a re-escaped string.
      let objs: json[] = [];
      for (let r in requests) {
        let body = self.build_request<T>(r.prompt);
        let inline: map<string, json> = {
          "custom_id": r.custom_id,
          "params":    baml.json.parse(body.body),
        };
        objs.push(inline);
      }
      let batch_id = anthropic_create_batch(self.api_key, baml.json.stringify(objs));
      BatchHandle<T> { id: batch_id, owner: "Anthropic", count: requests.length() }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BatchError>(e, "anthropic submit_batch failed");
    }
    function poll_batch<T>(self, handle: BatchHandle<T>) -> BatchState
        throws baml.ExtendUnknownError<baml.errors.BatchError> {
      assert_owner(handle.owner, "Anthropic");
      anthropic_retrieve_batch(self.api_key, handle.id)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BatchError>(e, "anthropic poll_batch failed");
    }
    function download_batch<T>(self, handle: BatchHandle<T>) -> BatchResult<T>[]
        throws baml.ExtendUnknownError<baml.errors.BatchError> {
      assert_owner(handle.owner, "Anthropic");
      let raw_lines = anthropic_stream_results(self.api_key, handle.id);
      let out: BatchResult<T>[] = [];
      for (let line in raw_lines) {
        let parsed: AntBatchLine = baml.json.from_json<AntBatchLine>(line);
        let custom_id: string = match (parsed.custom_id) { let s: string => s, null => "" };
        let res: AntBatchResultWire = match (parsed.result) {
          let rr: AntBatchResultWire => rr, null => AntBatchResultWire { type: null, message: null },
        };
        // Anthropic result type: "succeeded" | "errored" | "canceled" | "expired"
        let kind: string = match (res.type) { let k: string => k, null => "errored" };
        let result: BatchResult<T> = match (kind) {
          "succeeded" => match (res.message) {
            let msg: json => BatchResult<T> {
                    custom_id: custom_id,
                    value: baml.json.from_json<T>(msg),
                    error: null,
                    // reuse the SAME normalizer as the sync path off the decoded body.
                    usage: ant_usage_of(baml.json.from_json<AntWire>(msg)),
                  },
            null => BatchResult<T> {
                    custom_id: custom_id, value: null,
                    error: "batch line succeeded but carried no message", usage: Usage.zero(),
                  },
          },
          _ => BatchResult<T> {
                    custom_id: custom_id, value: null,
                    error: "batch line " + kind, usage: Usage.zero(),
                  },
        };
        out.push(result);
      }
      out
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.BatchError>(e, "anthropic download_batch failed");
    }
  }
}


// -----------------------------------------------------------------------------
// 8. Shared host seams. Genuine boundaries only — usage normalization lives in the
//    ResponseMeta impls above (pure BAML: decode the body with `baml.json.from_json`
//    into a typed wire class and read fields); the JSONL/file transport below is
//    genuinely host-level.
// -----------------------------------------------------------------------------

function assert_owner(actual: string, expected: string) -> null
    throws baml.ExtendUnknownError<baml.errors.BatchError> {
  if (actual != expected) {
    throw WrongBatchOwner { actual: actual, expected: expected };
  }
  null
}

// host: OpenAI batch transport
function openai_upload_jsonl(api_key: string, jsonl: string) -> string { $rust_io_function }
function openai_create_batch(api_key: string, file_id: string, endpoint: string, window: string) -> string { $rust_io_function }
function openai_retrieve_batch(api_key: string, batch_id: string) -> BatchState { $rust_io_function }
function openai_download_output(api_key: string, batch_id: string) -> string[] { $rust_io_function }

// host: Anthropic batch transport
function anthropic_create_batch(api_key: string, requests_json: string) -> string { $rust_io_function }
function anthropic_retrieve_batch(api_key: string, batch_id: string) -> BatchState { $rust_io_function }
function anthropic_stream_results(api_key: string, batch_id: string) -> string[] { $rust_io_function }
