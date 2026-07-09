// scenario 34 — cost & tokens + batch: ../../../../../llm-provider/ideas/scenarios/34-cost-and-batch/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The design stresses the proposal on metering and money. Its three levers, on the
// REAL surface (fixtures in implementation.baml):
//
//   1. COST ACCOUNTING per call/per chain — usage is a PRODUCT SIDECAR, so it rides
//      the `Foo$with(args, project, client=)` companion (desugar of `call_with`),
//      projecting `m.usage()`; the app prices it with `s34_price(usage, row, mult)`
//      over an app-owned rate table (`s34_Prices()` — DATA, the design's cost table).
//   2. BUDGET ENFORCEMENT — an `s34_Budget` wrapper Provider shares a mutable
//      `s34_BudgetCell` across a fan-out tree and REFUSES the call that blows the
//      ceiling with a typed `s34_BudgetExceeded` (Failure axis + CallError), caught
//      concrete-first. Budgeting is a hand-threaded function, NOT a `client`.
//   3. THE BATCH LEVER — `baml.ai.Background` submit/poll (scenario 27's lifecycle),
//      the backlog priced at the DISCOUNTED (~50%-off) rate: batch as a PricingMode
//      MULTIPLIER on the same rate table, not a parallel one.
//
// PRESERVED (plan D4): the usage-projection summing, AggregateMeta, Traced
// transparency, meter.wrap-over-fallback, and one-meter-across-round-robin tests,
// riding the MeteredEcho/MeteredMeta fixtures (now in implementation.baml).
//
// LIVE (testset "integ-test-34-cost-tokens", real OpenAI): one real $with call
// projecting usage into a cost figure via the rate table, logged.


// --- The single-turn LLM function the cost/budget tier rides. ----------------
// One plain LLM function; every cost policy is just a different Provider handed in
// at the call site. Default is a dead client so offline tests pass an explicit,
// deterministic override (ns_ai_scenarios house rule).
client<llm> s34_DeadClient {
  provider openai
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s34_Classify(text: string) -> s34_Ticket {
  client s34_DeadClient
  prompt #"Classify this support ticket into category, severity (1-5), and a terse summary: {{ text }}"#
}


// ============================================================================
// 1. COST ACCOUNTING — a metered call prices its usage via the rate table.
//    The app wants the answer AND what it cost: `Foo$with` projects `m.usage()`
//    alongside the value, and `s34_price` turns it into dollars.
// ============================================================================

test "34: a metered call prices its usage via the app-owned rate table" {
    let row = s34_row_for(s34_Prices(), "gpt-4.1-mini");
    let echo = MeteredEcho {
        reply: "{\"category\":\"billing\",\"severity\":3,\"summary\":\"late fee dispute\"}",
        inp: 1200,
        out: 400,
    };
    let r = s34_Classify$with(
        "customer disputes a late fee",
        (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
        client = echo,
    ) catch (e) {
        _ => baml.ai.CallResult<s34_Ticket, baml.ai.Usage> {
            value: s34_Ticket { category: "?", severity: 0, summary: "failed" },
            meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 },
        },
    };
    let cost = s34_price(r.meta, row, 1.0);
    log.info("34 cost-per-call: category=" + r.value.category
        + " in=" + r.meta.input_tokens.to_string() + " out=" + r.meta.output_tokens.to_string()
        + " cost=$" + cost.to_string());
    assert.equal(r.value.category, "billing")
    assert.equal(r.meta.input_tokens, 1200)
    assert.equal(r.meta.output_tokens, 400)
    // 1200/1e6*0.4 + 400/1e6*1.6 = 0.00048 + 0.00064 = 0.00112
    assert.is_true(cost > 0.00111)
    assert.is_true(cost < 0.00113)
}


// ============================================================================
// 2. BUDGET ENFORCEMENT — a fan-out tree shares ONE cell; the call that blows the
//    ceiling is REFUSED typed. `s34_Budget` charges each call by projecting usage
//    off the SAME `call_with` seam the plain `.call` rides, so every sub-call is
//    billed and the running total has a hard ceiling.
// ============================================================================

test "34: a budget refuses the call that blows the ceiling (typed BudgetExceeded)" {
    let row = s34_row_for(s34_Prices(), "gpt-4.1");
    // Each call bills 1000 in + 1000 out = $0.01 at gpt-4.1 (2.0 in / 8.0 out per mtok):
    //   1000/1e6*2 + 1000/1e6*8 = 0.002 + 0.008 = 0.01
    let cell = s34_BudgetCell { limit_usd: 0.025, row: row, usages: [] };
    let budget: baml.ai.Provider = s34_Budget {
        inner: MeteredEcho {
            reply: "{\"category\":\"billing\",\"severity\":1,\"summary\":\"x\"}",
            inp: 1000,
            out: 1000,
        },
        cell: cell,
        tenant: "acme",
    };
    // Drive the plain LLM function through the budget override; the shared cell
    // sums across every call. 2 calls fit ($0.02); the 3rd blows $0.025 and every
    // call after it refuses.
    let outcomes: string[] = [];
    let i = 0;
    while (i < 4) {
        i = i + 1;
        let r = s34_Classify("ticket " + i.to_string(), client = budget) catch (e) {
            let be: s34_BudgetExceeded => {
                log.info("34 budget: refused call #" + i.to_string() + " — tenant " + be.tenant
                    + " spent $" + be.spent_usd.to_string() + " > $" + be.limit_usd.to_string());
                s34_Ticket { category: "REFUSED", severity: 0, summary: "" }
            },
            _ => s34_Ticket { category: "ERR", severity: 0, summary: "" },
        };
        outcomes.push(r.category);
    }
    log.info("34 budget: tenant acme final spend $" + cell.spent_usd().to_string()
        + " over " + cell.usage().output_tokens.to_string() + " output tokens");
    assert.equal(outcomes.join(","), "billing,billing,REFUSED,REFUSED")
    // All 4 calls charged the cell before the 3rd+ refused (post-call ceiling check).
    assert.equal(cell.usage().output_tokens, 4000)
}

test "34: a budget over a Fallback charges the winner's usage through one accumulator" {
    let row = s34_row_for(s34_Prices(), "gpt-4.1");
    let cell = s34_BudgetCell { limit_usd: 1.0, row: row, usages: [] };
    // inner: the priority member fails, the fallback member wins — its usage flows
    // through the SAME cell (usage read off the projected ResponseMeta, not config).
    let inner: baml.ai.Provider = baml.ai.Fallback { members: [
        AlwaysFailProvider {},
        MeteredEcho {
            reply: "{\"category\":\"outage\",\"severity\":5,\"summary\":\"api down\"}",
            inp: 300,
            out: 700,
        },
    ] };
    let budget: baml.ai.Provider = s34_Budget { inner: inner, cell: cell, tenant: "globex" };
    let r = s34_Classify("everything is on fire", client = budget) catch (e) {
        _ => s34_Ticket { category: "ERR", severity: 0, summary: "" },
    };
    log.info("34 budget-over-fallback: category=" + r.category
        + " charged $" + cell.spent_usd().to_string() + " over " + cell.usage().input_tokens.to_string() + " in-tokens");
    assert.equal(r.category, "outage")
    // Only the winner's usage is charged (the D4 winner-only residue).
    assert.equal(cell.usage().input_tokens, 300)
    assert.equal(cell.usage().output_tokens, 700)
}


// ============================================================================
// 3. THE BATCH LEVER — an offline backlog scored via `baml.ai.Background`
//    submit/poll (scenario 27's lifecycle), priced at the ~50%-off batch rate.
//    The batch discount is a PricingMode MULTIPLIER (0.5) on the same rate row.
// ============================================================================

test "34: the batch lever prices a backlog at the discounted rate (Background submit/poll)" {
    let row = s34_row_for(s34_Prices(), "gpt-4.1-mini");
    let jobs = s34_BatchJobs {
        owner: "batch-pool",
        reply: "{\"category\":\"feedback\",\"severity\":2,\"summary\":\"scored offline\"}",
        inp: 500,
        out: 150,
    };
    // custom_id = the DB row key, so the unordered results rejoin by our own id.
    let backlog: map<string, string> = {
        "row-1": "great product, shipping was slow",
        "row-2": "love the new dashboard",
        "row-3": "billing charged me twice",
    };
    let p: baml.ai.Provider = jobs;
    let scored: string[] = match (p) {
        let b: baml.ai.Background => {
            let landed: string[] = [];
            for (let key in backlog.keys()) {
                let text: string = match (backlog.get(key)) { let s: string => s, null => "" };
                // submit returns a handle NOW (custom_id keyed); poll lands the score.
                let job: baml.ai.Job<s34_Ticket> = b.submit<s34_Ticket>(text, key) catch (e) {
                    _ => baml.ai.Job<s34_Ticket> { id: "SUBMIT_FAILED", owner: "" },
                };
                match (b.poll<s34_Ticket>(job) catch (e) { _ => null }) {
                    null => {},
                    let t: s34_Ticket => { landed.push(key + ":" + t.category); },
                }
            }
            landed
        },
        _ => [],
    };
    let n = scored.length();
    // Per-line usage summed across the backlog (each row bills jobs.inp/out).
    let batch_usage = baml.ai.Usage { input_tokens: jobs.inp * n, output_tokens: jobs.out * n };
    let sync_cost = s34_price(batch_usage, row, 1.0);
    let batch_cost = s34_price(batch_usage, row, 0.5);
    log.info("34 batch lever: " + n.to_string() + " rows scored (" + scored.join(", ") + ") — "
        + "sync $" + sync_cost.to_string() + " vs batch $" + batch_cost.to_string()
        + " (saved $" + (sync_cost - batch_cost).to_string() + ")");
    assert.equal(n, 3)
    assert.is_true(batch_cost < sync_cost)
    // The batch rate is exactly half the sync rate (the ~50%-off lever).
    let gap = sync_cost - batch_cost * 2.0;
    assert.is_true(gap < 0.0000001)
    assert.is_true(gap > -0.0000001)
}

test "34: a batch handle minted by another provider is refused, not polled (owner guard)" {
    let jobs = s34_BatchJobs { owner: "batch-pool", reply: "{}", inp: 0, out: 0 };
    let foreign = baml.ai.Job<s34_Ticket> { id: "resp_openai_xyz", owner: "openai-responses" };
    let p: baml.ai.Provider = jobs;
    let outcome: string = match (p) {
        let b: baml.ai.Background => match (b.poll<s34_Ticket>(foreign) catch (e) {
            let u: baml.errors.UnknownError => null,
        }) {
            null => "refused",
            let t: s34_Ticket => "unexpectedly polled",
        },
        _ => "not background",
    };
    log.info("34 batch owner guard: " + outcome);
    assert.equal(outcome, "refused")
}


// ============================================================================
// 4. PRESERVED D4 TESTS — usage-projection summing, AggregateMeta, Traced
//    transparency, meter.wrap-over-fallback, one-meter-across-round-robin.
//    (MeteredEcho / MeteredMeta live in implementation.baml.)
// ============================================================================

test "34: token budgets sum across calls via the usage projection" {
    let p = MeteredEcho { reply: "a", inp: 10, out: 5 };
    let total = 0;
    let calls = 0;
    while (calls < 3) {
        calls = calls + 1;
        let r = p.call_with<string, int, never>("q", (m: baml.ai.ResponseMeta) -> int { m.usage().output_tokens }) catch (e) {
            _ => baml.ai.CallResult<string, int> { value: "failed", meta: 0 },
        };
        total = total + r.meta;
    }
    assert.equal(total, 15)
}

test "34: AggregateMeta sums usage across parts; finish_reason is the last part's" {
    let agg = baml.ai.AggregateMeta { parts: [
        MeteredMeta { inp: 2, out: 3 },
        MeteredMeta { inp: 5, out: 7 },
    ] };
    assert.equal(agg.usage().input_tokens, 7)
    assert.equal(agg.usage().output_tokens, 10)
    assert.equal(agg.finish_reason(), "stop")
    let empty = baml.ai.AggregateMeta { parts: [] };
    assert.equal(empty.usage().input_tokens, 0)
    assert.equal(empty.finish_reason(), "unknown")
}

test "34: Traced records metas as a side effect; the projection sees the original meta" {
    let meter = baml.ai.UsageMeter { entries: [] };
    let p: baml.ai.Provider = MeteredEcho { reply: "a", inp: 10, out: 5 }.traced(meter);
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call_with<string, string, never>(
            "q",
            (m: baml.ai.ResponseMeta) -> string { m.finish_reason() },
        ) catch (e) { _ => baml.ai.CallResult<string, string> { value: "failed", meta: "?" } },
        _ => baml.ai.CallResult<string, string> { value: "not http", meta: "?" },
    };
    assert.equal(r.value, "a")
    assert.equal(r.meta, "stop")
    assert.equal(meter.calls(), 1)
    assert.equal(meter.total().input_tokens, 10)
    assert.equal(meter.total().output_tokens, 5)
}

test "34: meter.wrap over fallback members observes exactly what ran (the winner)" {
    let meter = baml.ai.UsageMeter { entries: [] };
    let p: baml.ai.Provider = baml.ai.Fallback { members: [
        meter.wrap(AlwaysFailProvider {}),
        meter.wrap(MeteredEcho { reply: "recovered", inp: 4, out: 6 }),
    ] };
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "recovered")
    // The failed member threw without meta — only the winner is observable (D4 residue).
    assert.equal(meter.calls(), 1)
    assert.equal(meter.total().input_tokens, 4)
    assert.equal(meter.total().output_tokens, 6)
}

test "34: one meter sums across round-robin members over several calls" {
    let meter = baml.ai.UsageMeter { entries: [] };
    let p: baml.ai.Provider = baml.ai.RoundRobin { members: [
        meter.wrap(MeteredEcho { reply: "a", inp: 1, out: 2 }),
        meter.wrap(MeteredEcho { reply: "b", inp: 10, out: 20 }),
    ], counter: 0 };
    match (p) {
        let h: baml.ai.HttpProvider => {
            let r1 = h.call<string>("q") catch (e) { _ => "x" };
            let r2 = h.call<string>("q") catch (e) { _ => "x" };
            assert.equal(r1, "a")
            assert.equal(r2, "b")
        },
        _ => assert.is_true(false),
    };
    assert.equal(meter.calls(), 2)
    assert.equal(meter.total().input_tokens, 11)
    assert.equal(meter.total().output_tokens, 22)
    // And the meter's aggregate is itself a ResponseMeta.
    assert.equal(meter.aggregate().usage().output_tokens, 22)
}


// ── LIVE TIER (excluded by default; run with keys via `-i "integ-test-34-*::"`) ──
// The design's claim against the REAL API: usage rides the value path via `$with`,
// and the app prices it with its own rate table. Tiny prompt so it lands fast.
testset "integ-test-34-cost-tokens" {
    test "34 live: a real $with call projects usage and prices it via the rate table" {
        let real: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        };
        let row = s34_row_for(s34_Prices(), "gpt-4.1-mini");   // a real published rate row
        let r = s34_Classify$with(
            "Refund requested: the package arrived damaged.",
            (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
            client = real,
        ) catch (e) {
            _ => baml.ai.CallResult<s34_Ticket, baml.ai.Usage> {
                value: s34_Ticket { category: "live $with failed", severity: 0, summary: "" },
                meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 },
            },
        };
        let cost = s34_price(r.meta, row, 1.0);
        log.info("34 live cost: category=" + r.value.category
            + " in=" + r.meta.input_tokens.to_string() + " out=" + r.meta.output_tokens.to_string()
            + " cost=$" + cost.to_string());
        assert.is_true(r.meta.input_tokens > 0)
        assert.is_true(r.meta.output_tokens > 0)
        assert.is_true(cost > 0.0)
    }
}
