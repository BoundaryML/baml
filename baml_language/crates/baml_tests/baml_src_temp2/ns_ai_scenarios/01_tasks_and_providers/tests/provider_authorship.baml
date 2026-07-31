// D9: the provider-author surface — null fingerprint honored, delegating
// wrappers legal via delegate(), classified shorthand errors, and the
// public helper kit.

// A from-scratch provider using ONLY the documented minimal surface: no
// fingerprint (null default — the guard is skipped, never fails), no
// pending_calls, no ns_internal.
class _MinimalConversation {
    owner: _MinimalProvider,
    history: ai.MessageHistory,

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never { self.owner }
        function messages(self) -> ai.Messages throws never { self.history }
    }
}

class _MinimalProvider {
    implements ai.Provider {
        function name(self) -> string throws never { "minimal" }
        function render_shorthand(self) -> string throws never { "minimal/one" }
    }

    implements ai.AgentProvider {
        function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
            _MinimalConversation { owner: self, history: task.messages().to_history() }
        }

        function step<T>(
            self,
            conversation: ai.Conversation,
            tools: ai.tools.Tool[],
        ) -> ai.ModelStep<T> throws baml.errors.UnknownError {
            let value = baml.sap.parse<T>(`{"category":"billing","priority":"Urgent","summary":"s","reply":"r"}`) catch (e) {
                _ => throw baml.errors.UnknownError.with_message<never>(e, "minimal parse"),
            };
            ai.ModelStep<T> {
                outcome: value,
                metadata: ai.ResponseMetadata {
                    provider: "minimal",
                    model: "one",
                    request_id: null,
                    finish_reason: "stop",
                    usage: null,
                    attributes: {},
                    raw: null,
                },
            }
        }

        function submit(
            self,
            conversation: ai.Conversation,
            results: ai.tools.ToolResult[],
        ) -> ai.Conversation throws never {
            conversation
        }
    }
}

test "D9a: the documented null fingerprint default runs, not rejects" {
    let value = ResolveTicket@task(sample_ticket())
        .with_provider(_MinimalProvider {})
        .complete(runner = ai.run.Agent<Resolution>.new());
    assert.equal(value.category, "billing")
}

// A thin forwarding wrapper — no re-owned conversation class, just
// delegate(). Before D9c this exact shape was rejected with "conversation
// owned by another provider instance" when used directly as a task's
// provider.
class _ForwardingWrapper {
    inner: ai.testing.FakeProvider,

    implements ai.Provider {
        function name(self) -> string throws never { "forwarding" }
        function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
        function delegate(self) -> ai.Provider? throws never { self.inner }
    }

    implements ai.AgentProvider {
        function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws ai.Failure {
            self.inner.begin<T>(task.with_provider(self.inner))
        }

        function step<T>(
            self,
            conversation: ai.Conversation,
            tools: ai.tools.Tool[],
        ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
            self.inner.step<T>(conversation, tools)
        }

        function submit(
            self,
            conversation: ai.Conversation,
            results: ai.tools.ToolResult[],
        ) -> ai.Conversation throws ai.Failure {
            self.inner.submit(conversation, results)
        }
    }
}

test "D9c: a delegating wrapper is legal directly as the task's provider" {
    let wrapper = _ForwardingWrapper { inner: fake_resolution() };
    let value = ResolveTicket@task(sample_ticket())
        .with_provider(wrapper)
        .complete(runner = ai.run.Agent<Resolution>.new());
    assert.equal(value.category, "billing")
    //# Identity walks the chain in both directions
    assert.is_true(ai.same_provider_instance(wrapper, wrapper.inner))
    assert.is_true(ai.same_provider_instance(wrapper.inner, wrapper))
}

class _BadShorthandProvider {
    implements ai.Provider {
        function name(self) -> string throws never { "bad-shorthand" }
        function render_shorthand(self) -> string throws never { "not a shorthand" }
    }

    implements ai.AgentProvider {
        function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
            _MinimalConversation { owner: _MinimalProvider {}, history: task.messages().to_history() }
        }
        function step<T>(
            self,
            conversation: ai.Conversation,
            tools: ai.tools.Tool[],
        ) -> ai.ModelStep<T> throws baml.errors.UnknownError {
            throw baml.errors.UnknownError.with_message<never>(null, "unreachable")
        }
        function submit(
            self,
            conversation: ai.Conversation,
            results: ai.tools.ToolResult[],
        ) -> ai.Conversation throws never {
            conversation
        }
    }
}

test "D9b: a malformed render shorthand is a classified failure, not a panic" {
    let kind = ResolveTicket@task(sample_ticket())
        .with_provider(_BadShorthandProvider {})
        .complete(runner = ai.run.Agent<Resolution>.new())
        .category catch (e) {
        let invalid: ai.InvalidRequest => {
            assert.is_true(invalid.detail.includes("render shorthand"));
            "classified"
        },
        _ => "other",
    };
    assert.equal(kind, "classified")
}

test "D9d: the public helper kit behaves like the stock machinery" {
    //# Usage accumulation
    let total = ai.Usage.zero();
    let _ = total.add(ai.Usage { input_tokens: 5, output_tokens: 2, cached_input_tokens: 1, reasoning_tokens: 0 });
    let _ = total.add(ai.Usage { input_tokens: 3, output_tokens: 4, cached_input_tokens: null, reasoning_tokens: 2 });
    assert.equal(total.input_tokens, 8);
    assert.equal(total.output_tokens, 6);
    assert.equal(total.cached_input_tokens, 1);
    assert.equal(total.reasoning_tokens, 2);

    //# HTTP classification matches the shared table
    match (ai.classify_http("acme", 429, "slow down")) {
        let limited: ai.RateLimited => {},
        _ => assert.is_true(false),
    }
    match (ai.classify_http("acme", 503, "boom")) {
        let network: ai.NetworkFailure => {},
        _ => assert.is_true(false),
    }
    match (ai.classify_http("acme", 422, "bad")) {
        let invalid: ai.InvalidRequest => assert.equal(invalid.status_code, 422),
        _ => assert.is_true(false),
    }

    //# Correlation checks
    let call = ai.tools.ToolCall { id: "c-1", name: "lookup", args: {} };
    let ok = ai.tools.check_results([call], [ai.tools.ToolOk.of(call, { "done": true })], "acme") catch (e) {
        _ => baml.sys.panic("exact correlation must pass"),
    };
    let kind = ai.tools.check_results([call], [], "acme").to_string() catch (e) {
        let invalid: ai.InvalidRequest => "under-answered",
        _ => "other",
    };
    assert.equal(kind, "under-answered")
}
