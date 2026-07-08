// scenario 43 — workflow graph: ../../../../../llm-provider/ideas/scenarios/43-workflow-graph/README.md
// A step is a typed function; a model call is a provider call; fan-in is
// structured concurrency (spawn/await). Live tier = workflow_graph_live
// (Rust ai_provider suite).

class WfDoc {
    text: string,
}

function wf_fetch_doc(url: string) -> WfDoc throws never {
    WfDoc { text: "contents of " + url }
}

function wf_model_text(
    p: baml.ai.Provider,
    prompt: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    match (p) {
        let h: baml.ai.HttpProvider => h.call<string>(prompt),
        _ => throw baml.errors.Unsupported { message: "client cannot produce a value" },
    }
}

// fetch, then summarize + label in parallel, then combine.
function wf_doc_pipeline(
    p: baml.ai.Provider,
    url: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    let doc = wf_fetch_doc(url);
    let f_summary = spawn { wf_model_text(p, "Summarize:\n" + doc.text) };
    let f_label = spawn { wf_model_text(p, "Label:\n" + doc.text) };
    let summary = await f_summary;
    let label = await f_label;
    "[" + label + "] " + summary
}

test "43: the step graph fans out and combines over a provider" {
    let out = wf_doc_pipeline(EchoProvider { reply: "step-output" }, "https://example.com/x") catch (e) {
        _ => "pipeline failed",
    };
    assert.equal(out, "[step-output] step-output")
}
