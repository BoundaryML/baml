// =============================================================================
// usage.baml — APPLICATION-author code for Cost & tokens + batch.
//
// End-user code on top of implement.baml. Shows the ergonomics of: a metered
// call, a Budget that protects a fan-out tree, and a batch job. Uses TWO
// divergent providers (OpenAI, Anthropic) to prove portability and expose where
// it leaks.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. The data the app extracts. (Same schema both providers, per proposal §2.)
// -----------------------------------------------------------------------------
class Ticket {
  category: string
  severity: int
  summary: string
}


// -----------------------------------------------------------------------------
// 1. Plain clients — config only, the proposal's `client = function -> Provider`.
//
//    service_tier is just a provider field (proposal §4): OpenAI takes
//    flex/priority, Anthropic takes auto/standard_only. Different value sets, no
//    shared options blob, no @providers map. This is the clean part.
// -----------------------------------------------------------------------------

client Fast() {
  OpenAI { model: "gpt-4.1", api_key: env.OPENAI_API_KEY, service_tier: "priority" }
}

client Cheap() {
  OpenAI { model: "gpt-4.1-mini", api_key: env.OPENAI_API_KEY, service_tier: "flex" }
}

client Claude() {
  Anthropic { model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY,
              max_tokens: 1024, service_tier: null }
}


// -----------------------------------------------------------------------------
// 2. The shared price table — DATA the app owns (so it can be updated without a
//    code change when prices drift). One row per model used below.
// -----------------------------------------------------------------------------
function Prices() -> PriceTable {
  PriceTable {
    rows: {
      "gpt-4.1":            PriceRow { input_per_mtok: 2.00, output_per_mtok: 8.00,
                                       cache_read_per_mtok: 0.50, cache_write_per_mtok: 2.50 },
      "gpt-4.1-mini":       PriceRow { input_per_mtok: 0.40, output_per_mtok: 1.60,
                                       cache_read_per_mtok: 0.10, cache_write_per_mtok: 0.50 },
      "claude-sonnet-4-5":  PriceRow { input_per_mtok: 3.00, output_per_mtok: 15.00,
                                       cache_read_per_mtok: 0.30, cache_write_per_mtok: 3.75 },
    }
  }
}


// -----------------------------------------------------------------------------
// 3. A single metered call. The app wants the answer AND what it cost.
//
//    `.call` (the plain companion) returns only the value. To ALSO get usage the
//    app matches the client's provider for `HttpProvider` and uses its `call_with`
//    seam, projecting `m.usage()` off the normalized ResponseMeta — usage is a
//    PRODUCT SIDECAR (value AND metadata), so there is no `.metered` companion, no
//    bespoke `Metered<T>`: any `HttpProvider` answers `m.usage()`. Because `call_with`
//    lives on the SAME capability as `call` (HttpProvider — see ../provider-as-marker.md),
//    it FORWARDS through Fallback/Retry/Cache, so a fallback's winning-member usage is
//    not dropped (the bug the old bespoke companion had). The (value, usage) pair is
//    just the tuple `call_with` returns — destructured here.
// -----------------------------------------------------------------------------

function Classify(text: string) -> Ticket {
  client: Cheap()
  prompt #"Classify this support ticket: {{ text }}"#
}

// The metered call the app makes when it cares about cost — no companion, just
// `call_with` projecting usage. Returns the tuple (Ticket, Usage):
//   let (ticket, usage) = Classify.call_with(text, m => m.usage());
// Fails on the canonical CallError channel like any `.call` (usage is not a
// separate capability to negotiate — any HttpProvider has a ResponseMeta; only a
// client with NO request/response capability at all hits the `_` arm below).
function ClassifyMetered(text: string) -> (Ticket, Usage)
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  // `call_with` is an HttpProvider capability now (the marker `Provider` carries no
  // interaction method, see ../provider-as-marker.md), so match the client's provider
  // for it. A pure-realtime/harness client would hit the `_` arm and be Unsupported.
  match (Cheap()) {
    let h: HttpProvider => h.call_with<Ticket, Usage>(render_prompt(text), m => m.usage()),
    _ => throw baml.errors.Unsupported { message: "client has no request/response sidecar" },
  }
}


// -----------------------------------------------------------------------------
// 4. A Budget over a FAN-OUT TREE.
//
//    One app request fans out into many billed calls. The app builds ONE
//    BudgetCell per request and wraps each provider it will touch with a Budget
//    sharing that cell. Every sub-call adds to the same total; the (limit-blown)
//    throw stops the runaway mid-tree.
//
//    Because a Budget IS a Provider, it slots into a normal combinator chain:
//    here the inner provider is itself a Fallback (Fast -> Claude), and BOTH
//    members' usage flows through the single accumulator — even when the Fallback
//    swaps models mid-run, because charge_one reads the model off the projected
//    ResponseMeta (MeteredMeta.served_model), not off a fixed config.
//
//    The awkward bit: the cell is constructed imperatively and threaded by hand,
//    because the `client` sugar cannot own mutable cross-call state. So budgeting
//    is a FUNCTION, not a `client` (see evaluation).
// -----------------------------------------------------------------------------

// Build a budgeted provider for one app-request, sharing `cell`. Not a `client`
// because it takes the mutable cell as an argument — sugar can't carry that.
function Budgeted(cell: BudgetCell, tenant: string) -> Provider {
  // inner: try the priority model, fall back to Claude. Budget charges by projecting
  // each call's usage/model/tier off `call_with`, which the Fallback forwards — so a
  // model swap still flows through the one accumulator. Both providers' responses
  // carry the MeteredMeta dimension, so both are priceable.
  Budget {
    inner: Fast().fallback_to(Claude()),
    prices: Prices(),
    cell: cell,
    tenant: tenant,
  }
}

// The fan-out: one user request -> two sub-classifications + a summary. All three
// billed calls share ONE BudgetCell, so the run has one attributable total and a
// hard ceiling. If the third call would blow $0.05, charge_one throws before it
// returns and the whole pipeline aborts — that is the runaway protection.
// The fan-out propagates on the CallError channel (Budget.call forwards there). A
// blown ceiling arrives as the concrete `BudgetExceeded` — which implements BOTH
// MeteringError and CallError — so the caller of TriageRun can catch it by its
// concrete type (see SafeTriage) even though the static channel here is CallError.
function TriageRun(tenant: string, t1: string, t2: string) -> Ticket[]
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let cell = BudgetCell.new(0.05);                 // $0.05 ceiling for this whole request
  let p: Provider = Budgeted(cell, tenant);

  // `call` is an HttpProvider capability now (the marker `Provider` carries no
  // interaction method, see ../provider-as-marker.md), so match for it. Budget
  // implements HttpProvider, so the budgeted forwarder matches here.
  match (p) {
    let h: HttpProvider => {
      let a: Ticket = h.call<Ticket>(render_prompt(t1));   // charged + checked
      let b: Ticket = h.call<Ticket>(render_prompt(t2));   // charged + checked (running total)

      // At the end the app reads the summed usage/cost off the cell for chargeback.
      let spent = cell.total();
      baml.log.info("tenant " + tenant + " spent $" + fmt_usd(spent.usd)
                   + " over " + cell.usage().output_tokens.to_string() + " output tokens");
      [a, b]
    },
    _ => throw baml.errors.Unsupported {
      message: "budgeted client is not request/response (HttpProvider)" },
  }
}

// The consumer catch, most-specific-first: the concrete BudgetExceeded (price/ceiling
// domain failure), then the CallError interface (network/rate-limit triage without
// knowing the concrete class), then baml.UnknownError as the escape hatch.
function SafeTriage(tenant: string, t1: string, t2: string) -> Ticket[] {
  TriageRun(tenant, t1, t2) catch (e) {
    let be: BudgetExceeded       => {
      baml.log.info("aborted: tenant " + be.tenant + " hit $" + fmt_usd(be.limit_usd));
      []
    },
    let c: baml.errors.CallError => {
      if (c.is_rate_limit()) { baml.log.info("rate limited"); } else { baml.log.info("call failed"); }
      []
    },
    let u: baml.UnknownError     => { baml.log.info("unexpected triage failure"); [] },
  }
}


// -----------------------------------------------------------------------------
// 5. BATCH — offline scoring of a backlog at ~50% off.
//
//    No user is waiting (cron job). The app builds many BatchRequests, each with
//    its own custom_id, and calls the `.batch` companion (or run_batch). The
//    result is a map keyed by custom_id — the rejoin is done for us.
//
//    Note PricingMode.Batch is NOT something the app sets: it is implied by going
//    through the Batch capability. The downloaded per-line usage still flows to a
//    Budget if you want one, because each BatchResult carries its own Usage.
// -----------------------------------------------------------------------------

// Score a backlog. Submit -> poll -> download is hidden behind run_batch; we get
// back a dict keyed by our own custom_id (e.g. the DB row id).
function ScoreBacklog(rows: map<string, string>) -> map<string, BatchResult<Ticket>>
    throws baml.ExtendUnknownError<baml.errors.BatchError> {
  let p = Claude();                                // Anthropic batch: inline array path
  match (p) {
    let b: Batch => {
      // build one BatchRequest per row, custom_id = the DB key so we can rejoin.
      let reqs: BatchRequest[] = [];
      for (let key in rows.keys()) {
        let text: string = match (rows.get(key)) { let s: string => s, null => "" };
        reqs.push(BatchRequest {
          custom_id: key,
          prompt: render_prompt(text),
          overrides: {},
        });
      }
      // run_batch polls every 30s until terminal, downloads, rejoins by custom_id.
      b.run_batch<Ticket>(reqs, 30000)
    },
    _ => throw baml.errors.Unsupported { message: "client does not support batch" },
  }
}

// The progress-UI variant: the app drives submit/poll/download itself so it can
// surface "342/1000 done" without blocking. Shows the lifecycle is first-class,
// not only the blocking convenience.
function StartBatch(rows: map<string, string>) -> BatchHandle<Ticket>
    throws baml.ExtendUnknownError<baml.errors.BatchError> {
  let p = Cheap();                                 // OpenAI batch: JSONL file path
  match (p) {
    let b: Batch => {
      let reqs: BatchRequest[] = [];
      for (let key in rows.keys()) {
        let text: string = match (rows.get(key)) { let s: string => s, null => "" };
        reqs.push(BatchRequest { custom_id: key, prompt: render_prompt(text), overrides: {} });
      }
      b.submit_batch<Ticket>(reqs)                 // returns immediately; persist this handle
    },
    _ => throw baml.errors.Unsupported { message: "client does not support batch" },
  }
}

// ...later, possibly a different process, re-hydrating the persisted handle:
function CheckBatch(handle: BatchHandle<Ticket>) -> BatchState
    throws baml.ExtendUnknownError<baml.errors.BatchError> {
  let p = Cheap();
  match (p) {
    let b: Batch => b.poll_batch<Ticket>(handle),
    _ => throw baml.errors.Unsupported { message: "client does not support batch" },
  }
}

// A batch consumer catch, most-specific-first: the concrete WrongBatchOwner (an id
// minted by a different provider — e.g. a Fallback member), then the BatchError
// interface (network/rate-limit/partial-failure triage), then the escape hatch.
function SafeCheckBatch(handle: BatchHandle<Ticket>) -> BatchState? {
  CheckBatch(handle) catch (e) {
    let w: WrongBatchOwner    => {
      baml.log.info("handle owned by " + w.actual + ", not " + w.expected);
      null
    },
    let b: baml.errors.BatchError => {
      if (b.is_rate_limit()) { baml.log.info("batch pool rate limited"); } else { baml.log.info("batch poll failed"); }
      null
    },
    let u: baml.UnknownError  => { baml.log.info("unexpected batch failure"); null },
  }
}


// -----------------------------------------------------------------------------
// 6. Host seams used by app code only. (Map iteration uses `rows.keys()`, int→string
//    uses `n.to_string()`, and string concat uses `+` — all real stdlib, so no
//    bespoke host shims are needed here anymore.)
// -----------------------------------------------------------------------------
