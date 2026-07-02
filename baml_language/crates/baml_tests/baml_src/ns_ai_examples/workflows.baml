// Workflow / durable-execution family (scenarios 43, 44, 45, 47). A step is a typed
// function; a model call is `provider.call<T>`; fan-in is structured concurrency
// (spawn + await). Durable resume is an app-level Checkpoint/replay layer OUTSIDE the
// provider hierarchy (design: the net-new layer is durability, keyed by step coordinate).

class Doc {
    text: string,
}

function _fetch_doc(url: string) -> Doc throws never {
    Doc { text: "contents of " + url }
}

function _model_text(
    p: baml.ai.Provider,
    prompt: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    match (p) {
        let h: baml.ai.HttpProvider => h.call<string>(prompt),
        _ => throw baml.errors.Unsupported { message: "client cannot produce a value" },
    }
}

// 43 — a durable step graph: fetch, then summarize + classify in parallel (spawn/await
// fan-in), then combine. The topology lives in let/spawn/await (no reifiable graph IR — §3).
function doc_pipeline(
    p: baml.ai.Provider,
    url: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    let doc = _fetch_doc(url);
    let f_summary = spawn { _model_text(p, "Summarize:\n" + doc.text) };
    let f_label = spawn { _model_text(p, "One-word topic label for:\n" + doc.text) };
    let summary = await f_summary;
    let label = await f_label;
    "[" + label + "] " + summary
}

// 45 — durable execution: a Checkpoint records-then-replays a step keyed by a coordinate so
// the model is never re-sampled on resume. The persistence is stubbed (record/replay host
// surface pending); the record-vs-replay shape and coordinate keying are real.
class Checkpoint {
    run_id: string,
}

function durable_step<T>(
    ckpt: Checkpoint,
    coord: string,
    compute: () -> T throws baml.errors.UnknownError,
) -> T throws baml.errors.UnknownError {
    // A real impl checks the step log for `ckpt.run_id + "/" + coord` and replays a hit;
    // on a miss it runs `compute()` and records the result. Here we just run it.
    compute()
}

function durable_pipeline(
    p: baml.ai.Provider,
    url: string,
    ckpt: Checkpoint,
) -> string throws baml.errors.UnknownError {
    let doc = durable_step<Doc>(
            ckpt,
            "fetch",
            () -> Doc {
                _fetch_doc(url)
            },
        );
    durable_step<string>(
            ckpt,
            "summarize",
            () -> string throws baml.errors.UnknownError {
                _model_text(p, "Summarize:\n" + doc.text) catch (e) {
                    let u: baml.errors.UnknownError => throw u,
                    let c: baml.errors.CallError => {
                        throw baml.errors.UnknownError { data: null, message: ["summarize failed"] }
                    },
                }
            },
        )
}

// 47 — agents-in-workflows / workflow-as-tool: a workflow is a function, a function is
// usable as a tool's dispatch, so a whole workflow can back a single ToolResult.
function workflow_as_tool_dispatch(
    p: baml.ai.Provider,
    calls: baml.ai.ToolCall[],
) -> baml.ai.ToolResult[] throws never {
    let results: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let out = doc_pipeline(p, c.args) catch (e) {
            _ => "workflow failed"
        };
        results.push(baml.ai.ToolResult { id: c.id, output: out });
    }
    results
}

// 44 — suspend/resume HITL: negotiate the Suspendable capability (defined in stateful.baml).
function workflow_with_approval(
    p: baml.ai.Provider,
    task: string,
) -> string throws baml.errors.UnknownError {
    match (p) {
        let s: baml.ai.Suspendable => match (s.start<string>(task)) {
            let value: string => value,
            let sus: baml.ai.Suspend => "awaiting approval at " + sus.point,
        },
        _ => throw baml.errors.UnknownError { data: null, message: ["client is not suspendable"] },
    }
}
