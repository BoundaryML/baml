class LogValue {
    value: int,
}

enum LogChoice {
    First,
}

function hidden_log() -> void {
    log.info("hidden", event_name = "hidden");
}

function span_log() -> void {
    log.info("span", event_name = "span");
}

function name_first(order: int[]) -> string {
    order.push(1);
    "ordered"
}

function data_second(order: int[]) -> int {
    order.push(2);
    99
}

function log_sequence(long_name: string) -> int {
    trace.update_context(metadata = { "phase": "before" }, distinct_id = "log-user");
    let mutable = [1, 2];
    log.info(mutable, event_name = "mutable");
    mutable.push(3);
    log.info({ "level": "user", "data": 42, "event_name": "payload" }, event_name = "structured");
    trace.update_context(metadata = { "phase": "after" });
    log.debug(null);
    log.warn([null, true, 1.5], event_name = null);
    log.error("empty name", event_name = "");
    log.info(LogValue { value: 7 }, event_name = "class");
    log.info(LogChoice.First, event_name = "enum");
    let cycle: map<string, unknown> = {};
    cycle["self"] = cycle;
    log.info(cycle, event_name = "cycle");
    hidden_log($trace = trace.hidden().context(metadata = { "phase": "hidden" }));
    let task = spawn { log.info("spawn", event_name = "spawned"); };
    await task;
    let order: int[] = [];
    log.info(event_name = name_first(order), data = data_second(order));
    assert.equal(order, [1, 2]);
    log.info("long", event_name = long_name);
    span_log($trace = trace.span(inputs = false, output = false, error = false));
    log.debug(7, event_name = "last");
    14
}

test "named and unnamed logs preserve source evaluation and context boundaries" {
    assert.equal(log_sequence("long-name"), 14);
}
