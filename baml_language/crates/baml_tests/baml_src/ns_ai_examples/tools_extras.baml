// Usage examples for the tools/agent family (scenarios 10, 11, 14, 15) built on the real
// `baml.ai.Tools` capability. These are app-side (usage.baml) code, compiled by the test
// suite. They demonstrate the shapes; network-calling is not exercised here.

// 10 — agentic loop with a stop condition: drive begin/step/submit manually with a step cap,
// returning a partial-at-budget instead of looping forever.
function bounded_agent(
    p: baml.ai.Provider,
    prompt: string,
    tools: baml.ai.Tool[],
    max_steps: int,
) -> string throws baml.errors.ToolError | baml.errors.UnknownError {
    match (p) {
        let tp: baml.ai.Tools => {
            let t = tp.begin<string>(prompt, tools);
            let step_no = 0;
            while (step_no < max_steps) {
                step_no = step_no + 1;
                match (tp.step<string>(t)) {
                    let calls: baml.ai.ToolCalls => {
                        // no-op dispatch: echo empty results (a real app runs the tools)
                        let results: baml.ai.ToolResult[] = [];
                        for (let c in calls.calls) {
                            results.push(baml.ai.ToolResult { id: c.id, output: "{}" });
                        }
                        t = tp.submit(t, results);
                    },
                    let value: string => {
                        return value;
                    },
                }
            }
            "[budget hit after " + max_steps.to_string() + " steps]"
        },
        _ => throw baml.errors.Unsupported { message: "client cannot run tools" },
    }
}

// 11 — parallel tool calls: dispatch runs each tool concurrently via structured concurrency
// (spawn + baml.future.all), the shipped BEP-034 stack — no missing host surface.
function parallel_dispatch(calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    let futures: baml.future.Future<baml.ai.ToolResult, null>[] = [];
    for (let c in calls) {
        futures
            .push(
                spawn {
            baml.ai.ToolResult { id: c.id, output: "result for " + c.name }
        },
            );
    }
    await baml.future.all(futures)
}

// 15 — guardrails: inspect the input before the call and abort on a tripwire, otherwise
// run through the provider. (Output guards mirror this after the call.)
function guarded_call(
    p: baml.ai.Provider,
    prompt: string,
    blocked: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    if (prompt.includes(blocked)) {
        throw baml.errors.Unsupported { message: "input tripwire: blocked content" };
    }
    match (p) {
        let h: baml.ai.HttpProvider => h.call<string>(prompt),
        _ => throw baml.errors.Unsupported { message: "client cannot produce a value" },
    }
}

// 14 — multi-agent handoff: a sub-agent is itself a Provider, so a "handoff" is just calling
// another provider inside the dispatcher (deterministic orchestration = plain composition).
function handoff_dispatch(
    specialist: baml.ai.Provider,
    calls: baml.ai.ToolCall[],
) -> baml.ai.ToolResult[] throws never {
    let results: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let out = match (specialist) {
            let h: baml.ai.HttpProvider => {
                h.call<string>(c.args) catch (e) {
                    _ => "sub-agent failed"
                }
            },
            _ => "specialist is not callable",
        };
        results.push(baml.ai.ToolResult { id: c.id, output: out });
    }
    results
}
