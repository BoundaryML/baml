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
        function meta_of(self, from: string) -> baml.ai.ResponseMeta throws never {
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
