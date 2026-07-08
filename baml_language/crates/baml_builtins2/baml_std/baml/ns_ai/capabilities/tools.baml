// The Tools capability (plan Phase 3, scenario 09): a provider that runs a multi-turn
// function-calling loop. The provider-owned `Transcript` hides the wire threading (call
// ids, assistant-turn preservation, tool-result ordering); the loop speaks a tiny
// vocabulary (`ToolCall`/`ToolResult`/`ToolCalls`).

class Tool {
    name: string,
    description: string,
    parameters: string, // JSON Schema for the tool's arguments (the wire form)

    /// Build a tool whose parameter schema is lowered from a BAML `type` — the
    /// design-true typed entry (plan P7/D6): declare an args class, hand its `type`
    /// here, and SAP-parse `ToolCall.args` back into it in your dispatcher.
    ///
    ///   class WeatherArgs { city: string }
    ///   Tool.from_type("get_weather", "…", reflect.type_of<WeatherArgs>())
    function from_type(name: string, description: string, params: type) -> Tool
        throws root.errors.Unsupported {
        Tool {
            name: name,
            description: description,
            parameters: root.schema.json_schema(params, false),
        }
    }
}

/// One tool invocation the model requested. `id` is opaque to the loop; `args` is the
/// raw JSON arguments string the provider emitted.
class ToolCall {
    id: string,
    name: string,
    args: string,
}

/// The result the app produced for a `ToolCall` (fed back to the model). `output` is a
/// string (JSON or plain text).
class ToolResult {
    id: string,
    output: string,
}

/// Sentinel: the model asked to call tools rather than produce the final value
/// (the `Done`-shaped arm of the `step` loop).
class ToolCalls {
    calls: ToolCall[],
}

//baml:llm_capability
/// Multi-turn tool loop. `Transcript` is provider-owned + opaque.
interface Tools requires Provider {
    type Transcript

    function begin<T>(self, prompt: string, tools: Tool[]) -> Transcript
        throws root.errors.ToolError | root.errors.UnknownError
    // Mirror of `Iterator.next -> Item | Done`: either the final value or a request to call tools.
    function step<T>(self, t: Transcript) -> T | ToolCalls
        throws root.errors.ToolError | root.errors.UnknownError
    function submit(self, t: Transcript, results: ToolResult[]) -> Transcript
        throws root.errors.ToolError | root.errors.UnknownError

    // The default agentic loop: begin, then step/dispatch/submit until a value falls out.
    // `dispatch` is the app's tool executor (calls -> results); it handles its own errors.
    function run_tools<T>(
        self,
        prompt: string,
        tools: Tool[],
        dispatch: (ToolCall[]) -> ToolResult[] throws never,
    ) -> T throws root.errors.ToolError | root.errors.UnknownError {
        let t = self.begin<T>(prompt, tools);
        while (true) {
            match (self.step<T>(t)) {
                let calls: ToolCalls => {
                    t = self.submit(t, dispatch(calls.calls));
                },
                let value: T => {
                    return value;
                },
            }
        }
        root.sys.panic("unreachable")
    }
}

//baml:llm_companion(run_tools)
/// Drive the default agentic loop (the generated `Foo$run_tools` companion
/// delegates here). Tool-calling has no honest single-call degrade — a
/// non-Tools provider is a typed `Unsupported`. The prompt is flattened to
/// text because `Tools.begin` speaks the string form.
function drive_run_tools<T>(
    client: Provider,
    prompt: root.llm.PromptAst,
    tools: Tool[],
    dispatch: (ToolCall[]) -> ToolResult[] throws never,
) -> T throws root.errors.ToolError | root.errors.UnknownError {
    let text = root.llm.prompt_to_text(prompt);
    match (client) {
        let tp: Tools => tp.run_tools<T>(text, tools, dispatch),
        _ => throw root.errors.Unsupported {
            message: "client's provider does not support Tools",
        },
    }
}

// ── ToolLoop: the agentic loop as a provider (DCP §1.2; scenario 10's `Bounded`
// with tools/dispatch on board) ───────────────────────────────────────────────
// Implements `HttpProvider` by driving `begin`/`step`/`submit` over a
// Tools-capable `inner`, so a plain LLM-function call with a ToolLoop client
// runs the loop and returns `T` — the loop is implied by the provider, not a
// call-site mode. `per_turn_tools` from scenario 10 is deliberately absent for
// now: filtering tools mid-loop needs a `set_tools` seam on `Tools` (the D7
// gap); tools are fixed at `begin`.

/// What a stop policy sees before each tool dispatch: completed model turns and
/// the calls requested on the latest turn.
class LoopInfo {
    steps: int,
    calls: ToolCall[],
}

/// The budget outcome of `run_to_budget` (plan D5): the policy stopped the loop
/// before a final value. `partial` is reserved for a future partial-value
/// carry — the Tools seam does not surface one today.
class Budget<T> {
    partial: T?,
    steps_taken: int,
}

/// Stop after `n` completed model turns.
function step_count_is(n: int) -> (LoopInfo) -> bool throws never {
    (info: LoopInfo) -> bool {
        info.steps >= n
    }
}

/// Stop once the model requests the named tool (e.g. a `final_answer` signal).
function has_tool_call(name: string) -> (LoopInfo) -> bool throws never {
    (info: LoopInfo) -> bool {
        let found = false;
        for (let c in info.calls) {
            if (c.name == name) {
                found = true;
            }
        }
        found
    }
}

/// OR-combine stop policies (Vercel's `stopWhen: [stepCountIs(8), hasToolCall(..)]`).
function any_of(preds: ((LoopInfo) -> bool throws never)[]) -> (LoopInfo) -> bool throws never {
    (info: LoopInfo) -> bool {
        let hit = false;
        for (let p in preds) {
            if (p(info)) {
                hit = true;
            }
        }
        hit
    }
}

/// Loop-produced response metadata: no wire meta is surfaced by the Tools seam,
/// so dimensions answer best-effort defaults; `usage` is zeroed.
class LoopMeta {
    steps: int,

    implements root.ai.ResponseMeta {
        function finish_reason(self) -> string throws never {
            "tool_loop_complete"
        }
        function usage(self) -> root.ai.Usage throws never {
            root.ai.Usage { input_tokens: 0, output_tokens: 0 }
        }
        function reasoning(self) -> string | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "tool loop surfaces no reasoning" }
        }
        function logprobs(self) -> root.ai.Logprob[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "tool loop surfaces no logprobs" }
        }
        function citations(self) -> root.ai.Citation[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "tool loop surfaces no citations" }
        }
    }
}

/// The agentic loop as a provider: wraps a Tools-capable `inner` with tools,
/// dispatch, and an optional stop policy. Effectful by declaration (a re-driven
/// loop replays every tool side effect), so `Retry`/`Fallback` refuse to wrap it.
class ToolLoop {
    inner: root.ai.Provider,
    tools: Tool[],
    dispatch: (ToolCall[]) -> ToolResult[] throws never,
    stop_when: ((LoopInfo) -> bool throws never)?,

    implements root.ai.Provider {
        function is_effectful(self) -> bool throws never {
            true
        }
    }

    implements root.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.http.Request throws root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["tool loop has no single request"] }
        }
        function send(
            self,
            request: root.http.Request,
        ) -> unknown throws root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["tool loop does not send directly"] }
        }
        function parse<T>(
            self,
            from: unknown,
        ) -> T throws root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["tool loop does not parse directly"] }
        }
        function meta_of(
            self,
            from: unknown,
        ) -> root.ai.ResponseMeta throws root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["tool loop has no single meta"] }
        }

        // The loop: begin/step/dispatch/submit until a value falls out. Loop-path
        // errors ride the ToolError channel; they are normalized into this
        // method's CallError|UnknownError channel (CallError-implementing errors
        // pass through, anything else boxes).
        function call_messages_with<T, V, E2>(
            self,
            messages: root.ai.ChatMessage[],
            project: (root.ai.ResponseMeta) -> V throws E2,
        ) -> root.ai.CallResult<T, V> throws root.errors.CallError | root.errors.UnknownError | E2 {
            let text = _messages_text(messages);
            match (self.inner) {
                let tp: Tools => {
                    let d = self.dispatch;
                    let steps = 0;
                    let t = tp.begin<T>(text, self.tools) catch (e) {
                        _ => throw root.errors.UnknownError { data: e, message: ["tool loop: begin failed"] },
                    };
                    while (true) {
                        let outcome = tp.step<T>(t) catch (e) {
                            _ => throw root.errors.UnknownError { data: e, message: ["tool loop: step failed"] },
                        };
                        match (outcome) {
                            let calls: ToolCalls => {
                                steps = steps + 1;
                                match (self.stop_when) {
                                    null => {},
                                    let pred: (LoopInfo) -> bool => {
                                        if (pred(LoopInfo { steps: steps, calls: calls.calls })) {
                                            throw root.errors.LoopBudgetExceeded { steps_taken: steps };
                                        }
                                    },
                                }
                                t = tp.submit(t, d(calls.calls)) catch (e) {
                                    _ => throw root.errors.UnknownError { data: e, message: ["tool loop: submit failed"] },
                                };
                            },
                            let value: T => {
                                return root.ai.CallResult<T, V> {
                                    value: value,
                                    meta: project(LoopMeta { steps: steps }),
                                };
                            },
                        }
                    }
                    root.sys.panic("unreachable")
                },
                _ => throw root.errors.Unsupported {
                    message: "ToolLoop.inner does not support Tools",
                },
            }
        }
    }

    /// The budget-aware explicit surface (plan D5): the stop policy yields a
    /// `Budget<T>` sum arm instead of an error.
    function run_to_budget<T>(self, prompt: string) -> T | Budget<T>
        throws root.errors.ToolError | root.errors.UnknownError {
        match (self.inner) {
            let tp: Tools => {
                let d = self.dispatch;
                let steps = 0;
                let t = tp.begin<T>(prompt, self.tools);
                while (true) {
                    match (tp.step<T>(t)) {
                        let calls: ToolCalls => {
                            steps = steps + 1;
                            match (self.stop_when) {
                                null => {},
                                let pred: (LoopInfo) -> bool => {
                                    if (pred(LoopInfo { steps: steps, calls: calls.calls })) {
                                        return Budget<T> { partial: null, steps_taken: steps };
                                    }
                                },
                            }
                            t = tp.submit(t, d(calls.calls));
                        },
                        let value: T => {
                            return value;
                        },
                    }
                }
                root.sys.panic("unreachable")
            },
            _ => throw root.errors.Unsupported {
                message: "ToolLoop.inner does not support Tools",
            },
        }
    }
}

/// Flatten native messages to the string form the `Tools` seam speaks.
function _messages_text(messages: root.ai.ChatMessage[]) -> string throws never {
    let out = "";
    for (let m in messages) {
        for (let p in m.parts) {
            match (p.text) {
                null => {},
                let t: string => {
                    if (out == "") {
                        out = t;
                    } else {
                        out = out + "\n" + t;
                    }
                },
            }
        }
    }
    out
}
