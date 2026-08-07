// A reusable on_event callback: every journal event as one log.info line.
// Works with any client; pass it to Agent.new(on_event = log_event).
// Surface the lines with `baml test --logs INFO` (or run --log-file).

function log_event(e: root.ai.Event) -> null {
    match (e) {
        let r: root.ai.RunStarted => {
            log.info(`run started: ${r.spec_name}`);
        },
        let u: root.ai.UserMessage => {
            log.info(`user: ${u.content}`);
        },
        let a: root.ai.AssistantMessage => {
            log.info(`assistant (${a.client_id}): ${a.content.length()} block(s)`);
        },
        let t: root.ai.ToolRequested => {
            log.info(`tool requested: ${t.name} id=${t.id}`);
        },
        let done: root.ai.ToolCompleted => {
            log.info(`tool completed: id=${done.id} -> ${done.output}`);
        },
        let failed: root.ai.ToolFailed => {
            log.info(`tool FAILED: id=${failed.id} ${failed.message}`);
        },
        let usage: root.ai.Usage => {
            log.info(`usage: in=${usage.input_tokens} out=${usage.output_tokens}`);
        },
        let f: root.ai.FinalProduced => {
            log.info(`final produced: ${f.value_json}`);
        },
    };
    null
}
