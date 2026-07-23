// scenario 46 — workflow observability: ../../../../../llm-provider/ideas/scenarios/46-workflow-observability/README.md
// A stream of typed step events over a run.

class StepEvent {
    step: string,
    status: string,
}

function observe_pipeline(steps: string[]) -> StepEvent[] throws never {
    let events: StepEvent[] = [];
    for (let s in steps) {
        events.push(StepEvent { step: s, status: "done" });
    }
    events
}

test "46: a run emits one typed event per step" {
    let events = observe_pipeline(["fetch", "summarize", "label"]);
    assert.equal(events.length(), 3)
    assert.equal(events[1].step, "summarize")
    assert.equal(events[2].status, "done")
}
