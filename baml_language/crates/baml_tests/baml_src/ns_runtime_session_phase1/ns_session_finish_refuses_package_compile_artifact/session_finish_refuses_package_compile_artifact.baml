function main() -> bool {
    let session = reflect.Session.new();
    let artifact = reflect.Package._compile({ "main.baml": "function ready() -> bool { true }" }, {});
    let rejected = false;
    let _ = session._finish<unknown>(artifact) catch (_) {
        _ => {
            rejected = true
        },
    };
    rejected
}

test "session_finish_refuses_package_compile_artifact" {
    assert.equal(main(), true)
}
