// A reusable on_event callback: every journal event as one log.info line.
// Works with any client; pass it to Agent.new(on_event = log_event).
// Surface the lines with `baml test --logs INFO` (or run --log-file).

function log_event(e: ai.events.Event) -> null {
    match (e) {
        let r: ai.events.RunStarted => {
            log.info(`run started: ${r.spec_name}`);
        },
        let u: ai.events.UserMessage => {
            log.info(`user: ${u.content}`);
        },
        let a: ai.events.AssistantMessage => {
            log.info(`assistant (${a.client_id}): ${a.content.length()} block(s)`);
        },
        let t: ai.events.ToolRequested => {
            log.info(`tool requested: ${t.name} id=${t.id}`);
        },
        let done: ai.events.ToolCompleted => {
            log.info(`tool completed: id=${done.id} -> ${done.output}`);
        },
        let failed: ai.events.ToolFailed => {
            log.info(`tool FAILED: id=${failed.id} ${failed.message}`);
        },
        let usage: ai.events.Usage => {
            log.info(`usage: in=${usage.input_tokens} out=${usage.output_tokens}`);
        },
        let f: ai.events.FinalProduced => {
            log.info(`final produced: ${f.value_json}`);
        },
    };
    null
}
