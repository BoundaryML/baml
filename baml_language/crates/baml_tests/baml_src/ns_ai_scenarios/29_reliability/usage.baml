// scenario 29 — reliability: ../../../../../llm-provider/ideas/scenarios/29-reliability/README.md
// Combinators over the existential; the effect-aware refusal is in
// 10_agentic_loop ("retry refuses..."). Rust mock tier covers retry counts.

test "29: Fallback routes past a failing member to the first working one" {
    let p: baml.ai.Provider = baml.ai.Fallback { members: [AlwaysFailProvider {}, EchoProvider { reply: "recovered" }] };
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "recovered")
}

test "29: fluent combinator factories chain off any provider" {
    let chained: baml.ai.Provider = AlwaysFailProvider {}.fallback_to(EchoProvider { reply: "via fluent" });
    let r = match (chained) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "via fluent")
}

test "29: Retry exhausts its budget over a failing provider and throws" {
    let p: baml.ai.Provider = AlwaysFailProvider {}.with_retry(2);
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "exhausted" },
        _ => "not http",
    };
    assert.equal(r, "exhausted")
}

/// Counts wire calls — detects double-billing through combinators.
class CountingEcho {
    reply: string,
    calls: int,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["p"] } }
        }
        function meta_of(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            self.calls = self.calls + 1;
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["p"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}

class S29ProjErr {
    note: string,
}

test "29: a throwing projection does NOT re-drive the billed call through Retry" {
    let counter = CountingEcho { reply: "ok", calls: 0 };
    let p: baml.ai.Provider = baml.ai.Retry { inner: counter, max: 3, base_delay_ms: 1 };
    let outcome = match (p) {
        let h: baml.ai.HttpProvider => {
            let r = h.call_messages_with<string, string, S29ProjErr>(
                [baml.ai.ChatMessage.user("q")],
                (m: baml.ai.ResponseMeta) -> string throws S29ProjErr {
                    throw S29ProjErr { note: "projection blew up" };
                },
            ) catch (e) {
                let pe: S29ProjErr => baml.ai.CallResult<string, string> { value: "projection surfaced", meta: "" },
                _ => baml.ai.CallResult<string, string> { value: "wrong error", meta: "" },
            };
            r.value
        },
        _ => "not http",
    };
    assert.equal(outcome, "projection surfaced")
    assert.equal(counter.calls, 1)
}

test "29: a throwing projection does NOT trigger failover through Fallback" {
    let winner = CountingEcho { reply: "ok", calls: 0 };
    let second = CountingEcho { reply: "should never run", calls: 0 };
    let p: baml.ai.Provider = baml.ai.Fallback { members: [winner, second] };
    let outcome = match (p) {
        let h: baml.ai.HttpProvider => {
            let r = h.call_messages_with<string, string, S29ProjErr>(
                [baml.ai.ChatMessage.user("q")],
                (m: baml.ai.ResponseMeta) -> string throws S29ProjErr {
                    throw S29ProjErr { note: "projection blew up" };
                },
            ) catch (e) {
                let pe: S29ProjErr => baml.ai.CallResult<string, string> { value: "projection surfaced", meta: "" },
                _ => baml.ai.CallResult<string, string> { value: "wrong error", meta: "" },
            };
            r.value
        },
        _ => "not http",
    };
    assert.equal(outcome, "projection surfaced")
    assert.equal(winner.calls, 1)
    assert.equal(second.calls, 0)
}
