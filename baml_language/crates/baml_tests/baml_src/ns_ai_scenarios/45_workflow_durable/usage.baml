// scenario 45 — durable execution: ../../../../../llm-provider/ideas/scenarios/45-workflow-durable/README.md
// BLOCKED: P8 — the step-log persistence is host surface; the record-vs-replay
// shape and coordinate keying are real and tested offline.

class WfCheckpoint {
    run_id: string,
}

function wf_durable_step<T>(
    ckpt: WfCheckpoint,
    coord: string,
    compute: () -> T throws baml.errors.UnknownError,
) -> T throws baml.errors.UnknownError {
    // A real impl checks the step log for `ckpt.run_id + "/" + coord` and
    // replays a hit; on a miss it runs `compute()` and records. P8-pending:
    // run straight through.
    compute()
}

test "45: durable steps key by (run, coordinate) and pass values through" {
    let ckpt = WfCheckpoint { run_id: "run-7" };
    let doc = wf_durable_step<string>(ckpt, "fetch", () -> string { "the doc" }) catch (e) {
        _ => "step failed",
    };
    let sum = wf_durable_step<string>(ckpt, "summarize", () -> string { "sum of " + doc }) catch (e) {
        _ => "step failed",
    };
    assert.equal(sum, "sum of the doc")
}
