function main() -> string {
    let s = reflect.Session.new();
    let _ = s.eval(`let n: int = 5`) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].message;
        },
        _ => {
            return "wrong error";
        },
    };
    "unexpected success"
}

test "session_let_annotations_are_still_rejected" {
    assert.equal(
        main(),
        "invalid pattern type ascription: session bindings do not support patterns or `else`: type ascription not allowed here",
    )
}
