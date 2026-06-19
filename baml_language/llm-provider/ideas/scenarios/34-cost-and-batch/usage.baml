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
//    `.call` (the plain companion) throws usage away. To get usage the app uses
//    the `.metered` companion, which `match`es the provider for Metering and
//    returns Metered<T>. Note: NO fallback to `.call` is meaningful here — if the
//    provider can't meter, you cannot fabricate usage. So this is an "errors"
//    capability, not a "degrades" one (proposal §5 split).
// -----------------------------------------------------------------------------

function Classify(text: string) -> Ticket {
  client: Cheap()
  prompt #"Classify this support ticket: {{ text }}"#
}

// The metered companion the app actually calls when it cares about cost:
//   Classify.metered(text)  ->  Metered<Ticket>
// Generated like the .stream / .live companions: match on Metering, else error.
function Classify$metered(text: string) -> Metered<Ticket> {
  let p = Cheap();
  match (p) {
    let m: Metering => m.call_metered<Ticket>(render_prompt(text)),
    _ => throw baml.errors.Unsupported { message: "client cannot report usage" },
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
//    swaps models mid-run, because charge_one reads the model off Metered<T>.
//
//    The awkward bit: the cell is constructed imperatively and threaded by hand,
//    because the `client` sugar cannot own mutable cross-call state. So budgeting
//    is a FUNCTION, not a `client` (see evaluation).
// -----------------------------------------------------------------------------

// Build a budgeted provider for one app-request, sharing `cell`. Not a `client`
// because it takes the mutable cell as an argument — sugar can't carry that.
function Budgeted(cell: BudgetCell, tenant: string) -> Provider {
  // inner: try the priority model, fall back to Claude. Both are Metering.
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
function TriageRun(tenant: string, t1: string, t2: string) -> Ticket[] throws baml.errors.LlmClient {
  let cell = BudgetCell.new(0.05);                 // $0.05 ceiling for this whole request
  let p: Provider = Budgeted(cell, tenant);

  let a: Ticket = p.call<Ticket>(render_prompt(t1));   // charged + checked
  let b: Ticket = p.call<Ticket>(render_prompt(t2));   // charged + checked (running total)

  // At the end the app reads the summed usage/cost off the cell for chargeback.
  let spent = cell.total();
  baml.sys.log("tenant " ++ tenant ++ " spent $" ++ fmt_usd(spent.usd)
               ++ " over " ++ int_to_str(cell.usage().output_tokens) ++ " output tokens");
  [a, b]
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
    throws baml.errors.LlmClient {
  let p = Claude();                                // Anthropic batch: inline array path
  match (p) {
    let b: Batch => {
      // build one BatchRequest per row, custom_id = the DB key so we can rejoin.
      let reqs: BatchRequest[] = [];
      for (let key in keys(rows)) {
        reqs = reqs ++ [BatchRequest {
          custom_id: key,
          prompt: render_prompt(rows[key]),
          overrides: {},
        }];
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
function StartBatch(rows: map<string, string>) -> BatchHandle<Ticket> throws baml.errors.LlmClient {
  let p = Cheap();                                 // OpenAI batch: JSONL file path
  match (p) {
    let b: Batch => {
      let reqs: BatchRequest[] = [];
      for (let key in keys(rows)) {
        reqs = reqs ++ [BatchRequest { custom_id: key, prompt: render_prompt(rows[key]), overrides: {} }];
      }
      b.submit_batch<Ticket>(reqs)                 // returns immediately; persist this handle
    },
    _ => throw baml.errors.Unsupported { message: "client does not support batch" },
  }
}

// ...later, possibly a different process, re-hydrating the persisted handle:
function CheckBatch(handle: BatchHandle<Ticket>) -> BatchState throws baml.errors.LlmClient {
  let p = Cheap();
  match (p) {
    let b: Batch => b.poll_batch<Ticket>(handle),
    _ => throw baml.errors.Unsupported { message: "client does not support batch" },
  }
}


// -----------------------------------------------------------------------------
// 6. Host seams used by app code only.
// -----------------------------------------------------------------------------
function keys<V>(m: map<string, V>) -> string[] { $rust_io_function }
function int_to_str(n: int) -> string { $rust_io_function }
