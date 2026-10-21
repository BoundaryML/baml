function main() -> bool {
    let session = reflect.Session.new();
    let artifact = session._compile<int>(`1`);
    let first = session._finish<int>(artifact);
    let rejected = false;
    let _ = session._finish<int>(artifact) catch (_) {
        _ => {
            rejected = true
        },
    };
    first == 1 && rejected
}

test "session_compile_artifact_is_consumed_once" {
    assert.equal(main(), true)
}
