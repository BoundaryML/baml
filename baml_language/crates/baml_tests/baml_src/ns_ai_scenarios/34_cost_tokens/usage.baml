// scenario 34 — cost & tokens: ../../../../../llm-provider/ideas/scenarios/34-cost-and-batch/README.md
// Metering rides call_with usage projection. Live tier = usage_metering_live.

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
