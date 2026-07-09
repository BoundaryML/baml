// scenario 34 — cost & tokens + batch: ../../../../../llm-provider/ideas/scenarios/34-cost-and-batch/README.md
//   implementation.baml — LIBRARY-author-analog fixtures for the OFFLINE tier.
//
// The design's three levers, on the REAL surface:
//   1. COST ACCOUNTING is plain BAML over usage — usage is a PRODUCT SIDECAR
//      (rides `call_with` + `ResponseMeta`), and the price table is DATA the app
//      owns (`s34_PriceTable`/`s34_PriceRow`), so `s34_price(usage, row, mult)`
//      turns a normalized `baml.ai.Usage` into dollars. No host cost-table surface.
//   2. BUDGET ENFORCEMENT is a wrapper Provider (`s34_Budget`) sharing a mutable
//      `s34_BudgetCell` across a fan-out tree: each call projects its usage off
//      `call_with`, charges the cell, and REFUSES with a typed `s34_BudgetExceeded`
//      (implements the `Failure` axis + `CallError`) once the ceiling is blown.
//      Budgeting is a hand-threaded combinator, NOT a `client` — the design's
//      SUM/stateful-round-trip residue (the sugar can't own the accumulator).
//   3. THE BATCH LEVER is `baml.ai.Background` submit/poll (the same lifecycle as
//      scenario 27 — referenced, not duplicated): an offline backlog scored at a
//      discounted rate (`s34_price(..., 0.5)`), the ~50%-off cost lever expressed
//      as a PricingMode MULTIPLIER rather than a parallel price table.
//
// Divergences from the design flagged in the report: the real `baml.ai.Usage` is
// 2-field (no cache/reasoning split); `MeteredMeta` carries no `served_model`, so a
// budget prices against ONE rate row rather than a per-model table lookup; and there
// is no `Batch` capability — the discounted-lifecycle lever reuses `Background`.


// =============================================================================
// 0. The app's extracted shape (the design's `Ticket`).
// =============================================================================

class s34_Ticket {
    category: string,
    severity: int,
    summary: string,
}


// =============================================================================
// 1. COST ACCOUNTING — the price table is DATA; the math is plain BAML over usage.
// =============================================================================

/// One model's per-million-token rates (the design's `PriceRow`).
class s34_PriceRow {
    input_per_mtok: float,
    output_per_mtok: float,
}

/// The app-owned price table: one row per model (the design's `PriceTable`).
/// DATA, so prices drift without a code change.
class s34_PriceTable {
    rows: map<string, s34_PriceRow>,
}

/// The design's shared price table, as a function returning DATA.
function s34_Prices() -> s34_PriceTable {
    s34_PriceTable {
        rows: {
            "gpt-4.1": s34_PriceRow { input_per_mtok: 2.0, output_per_mtok: 8.0 },
            "gpt-4.1-mini": s34_PriceRow { input_per_mtok: 0.4, output_per_mtok: 1.6 },
            "claude-sonnet-4-5": s34_PriceRow { input_per_mtok: 3.0, output_per_mtok: 15.0 },
        },
    }
}

/// Cost in USD for one call's usage at `row`'s rates, scaled by `multiplier`
/// (the design's PricingMode: 1.0 sync, 0.5 batch, >1.0 priority). Pure BAML —
/// `x * 1.0` widens int→float (there is no implicit int→float in BAML).
function s34_price(usage: baml.ai.Usage, row: s34_PriceRow, multiplier: float) -> float {
    let inp = usage.input_tokens * 1.0;
    let out = usage.output_tokens * 1.0;
    (inp / 1000000.0 * row.input_per_mtok + out / 1000000.0 * row.output_per_mtok) * multiplier
}

/// Look a model's row up in the table, or a zero row if it is unpriced (the
/// design's `UnknownPriceError` degrades to "free" here rather than throwing —
/// the offline fixtures always price a known model).
function s34_row_for(table: s34_PriceTable, model: string) -> s34_PriceRow {
    match (table.rows.get(model)) {
        let r: s34_PriceRow => r,
        null => s34_PriceRow { input_per_mtok: 0.0, output_per_mtok: 0.0 },
    }
}


// =============================================================================
// 2. BUDGET ENFORCEMENT — a wrapper Provider over a shared, mutable cell.
// =============================================================================

/// The running total across a fan-out tree (the design's `BudgetCell`). Mutable
/// host state the `client` sugar can't own, so it is threaded by hand. Holds the
/// projected per-call `Usage` (append-only, like `UsageMeter.entries`) plus the
/// rate row and ceiling, so it reports BOTH summed tokens AND summed cost.
class s34_BudgetCell {
    limit_usd: float,
    row: s34_PriceRow,
    usages: baml.ai.Usage[],

    /// Charge one call's usage onto the running total.
    function charge(self, u: baml.ai.Usage) -> null throws never {
        self.usages.push(u);
        null
    }
    /// Summed usage across every charged call.
    function usage(self) -> baml.ai.Usage throws never {
        let inp = 0;
        let out = 0;
        for (let u in self.usages) {
            inp = inp + u.input_tokens;
            out = out + u.output_tokens;
        }
        baml.ai.Usage { input_tokens: inp, output_tokens: out }
    }
    /// Summed cost in USD across every charged call (sync rate).
    function spent_usd(self) -> float throws never {
        s34_price(self.usage(), self.row, 1.0)
    }
}

/// The typed money-domain failure (the design's `BudgetExceeded`): a ceiling blown
/// mid-tree. Implements the `Failure` axis (a POLICY refusal — not retryable, not a
/// transport fault) AND `CallError`, so it surfaces on `s34_Budget.call`'s channel
/// and the app catches it by its concrete type before any interface arm.
class s34_BudgetExceeded {
    tenant: string,
    spent_usd: float,
    limit_usd: float,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
}

/// A budget over an inner Provider: forwards `call_with` by runtime delegation
/// (identical shape to `Traced`/`Fallback`), projecting each sub-call's usage to
/// CHARGE the shared cell, then REFUSING with `s34_BudgetExceeded` once the total
/// blows the ceiling. Because `call_with` lives on the SAME capability as `call`,
/// the plain `call` path is charged too, and an inner `Fallback`'s model swap still
/// flows through the one accumulator (usage is read off the projected `ResponseMeta`).
class s34_Budget {
    inner: baml.ai.Provider,
    cell: s34_BudgetCell,
    tenant: string,

    implements baml.ai.Provider {
        function is_effectful(self) -> bool throws never {
            self.inner.is_effectful()
        }
    }

    implements baml.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["budget forwards calls; no single request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["budget does not send directly"] }
        }
        function parse<T>(
            self,
            from: unknown,
        ) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["budget does not parse directly"] }
        }
        function parse_meta(
            self,
            from: unknown,
        ) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["budget has no single meta"] }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            match (self.inner) {
                let h: baml.ai.HttpProvider => {
                    // One codec pass: pull the winner's raw meta (identity projection),
                    // charge its usage, THEN run the caller's projection over the same meta.
                    let r0: baml.ai.CallResult<T, baml.ai.ResponseMeta> = h.call_messages_with<T, baml.ai.ResponseMeta, never>(
                        messages,
                        (mm: baml.ai.ResponseMeta) -> baml.ai.ResponseMeta { mm },
                    );
                    self.cell.charge(r0.meta.usage());
                    if (self.cell.spent_usd() > self.cell.limit_usd) {
                        throw s34_BudgetExceeded {
                            tenant: self.tenant,
                            spent_usd: self.cell.spent_usd(),
                            limit_usd: self.cell.limit_usd,
                        };
                    }
                    return baml.ai.CallResult<T, V> { value: r0.value, meta: project(r0.meta) };
                },
                _ => {
                    throw baml.errors.UnknownError { data: null, message: ["budget: inner is not callable"] }
                },
            }
        }
    }
}


// =============================================================================
// 3. THE BATCH LEVER — `baml.ai.Background` submit/poll (scenario 27's lifecycle,
//    referenced not duplicated). An offline backlog scored server-side; the cost
//    lever is the DISCOUNTED rate applied to the summed backlog usage.
// =============================================================================

/// A scripted batch/background provider: `submit` mints a per-`custom_id` job
/// handle NOW; `poll` lands the same scored ticket (`reply`, SAP-parsed into T).
/// Each row bills `inp`/`out` tokens — the per-line usage the design says a batch
/// result carries, supplied here so the backlog can be priced at the batch rate.
/// A foreign-owner handle is refused typed (the scenario-27 D2 owner guard).
class s34_BatchJobs {
    owner: string,
    reply: string,
    inp: int,
    out: int,

    implements baml.ai.Provider {
        function is_effectful(self) -> bool throws never { true }
    }

    implements baml.ai.Background {
        function submit<T>(self, prompt: string, idempotency_key: string) -> baml.ai.Job<T> throws baml.errors.UnknownError {
            baml.ai.Job<T> { id: "batch-" + idempotency_key, owner: self.owner }
        }
        function poll<T>(self, job: baml.ai.Job<T>) -> T? throws baml.errors.UnknownError {
            if (job.owner != self.owner) {
                throw baml.errors.UnknownError {
                    data: null,
                    message: ["batch handle owned by " + job.owner + ", not " + self.owner],
                };
            }
            let v: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["batch parse failed"] },
            };
            v
        }
    }

    /// The known per-row usage a downloaded batch line carries (the design's
    /// `BatchResult.usage`). On the real surface `poll` returns only T?, so the
    /// per-line usage rides this side channel for pricing.
    function row_usage(self) -> baml.ai.Usage throws never {
        baml.ai.Usage { input_tokens: self.inp, output_tokens: self.out }
    }
}


// =============================================================================
// 4. PRESERVED D4 FIXTURES — the metering-provenance fakes the observability
//    tests below ride. Names kept verbatim (MeteredMeta / MeteredEcho).
// =============================================================================

/// Meta-bearing fake: reports scripted token counts.
class MeteredMeta {
    inp: int,
    out: int,
    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.inp, output_tokens: self.out }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
    }
}

class MeteredEcho {
    reply: string,
    inp: int,
    out: int,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["metered echo has no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["metered echo does not send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["parse"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            MeteredMeta { inp: self.inp, out: self.out }
        }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["parse"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(MeteredMeta { inp: self.inp, out: self.out }) }
        }
    }
}
