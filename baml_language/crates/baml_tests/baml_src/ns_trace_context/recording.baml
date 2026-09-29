function context_leaf(fail: bool) -> int throws string {
    if (trace.current_context().metadata["phase"] != "entry") {
        throw "missing entry context";
    }
    trace.update_context(metadata = {
        "phase": "done", "remove": null, "count": 2, "ratio": 1.5, "flag": true,
    });
    if (fail) {
        throw "context failure";
    }
    7
}

function recording_options(capture: bool) -> trace.Options {
    trace.context(metadata = {
        "phase": "entry", "remove": "yes", "text": "quote=\" backslash=\\ newline=\n tab=\t",
    }, distinct_id = "user-42").span(inputs = capture, output = capture, error = capture)
}

function context_record(capture: bool) -> int throws string {
    trace.update_context(metadata = { "root": "kept" });
    let options = recording_options(capture);
    let child = spawn { context_leaf(false, $trace = options) };
    let value = context_leaf(false, $trace = options);
    if (trace.current_context().metadata.length() != 1) {
        throw "child context leaked";
    }
    value + (await child)
}

function context_failure() -> int throws string {
    trace.update_context(metadata = { "root": "kept" });
    context_leaf(true, $trace = recording_options(false).reserve())
}

function context_many() -> int throws string {
    trace.update_context(metadata = { "root": "kept" });
    let options = recording_options(false);
    let sum = 0;
    let index = 0;
    while (index < 4096) {
        sum += context_leaf(false, $trace = options);
        index += 1;
    }
    sum
}
