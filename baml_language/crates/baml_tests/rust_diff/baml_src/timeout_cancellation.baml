// A timeout cancels a compiled body suspended in a sys-op. Its frame records
// must survive until the fiber has unwound, including thread-finish telemetry.
function TimeoutSlow() -> string throws never {
    baml.sys.sleep(baml.time.Duration.from_seconds(60));
    "finished"
}

function TimeoutTimed(body: () -> string throws unknown) -> string {
    baml.future.with_timeout(baml.time.Duration.from_milliseconds(25), body)
    catch_all (e) { _ => e.to_string() }
}

function TimeoutCancellation() -> void {
    let result = TimeoutTimed(TimeoutSlow);
    assert.is_true(result.starts_with("Timeout"));
    baml.io.println("Timeout");
}
