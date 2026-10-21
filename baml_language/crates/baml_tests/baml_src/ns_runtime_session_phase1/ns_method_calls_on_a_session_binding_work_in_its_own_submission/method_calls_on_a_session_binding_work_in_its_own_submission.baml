function main() -> string {
    let s = reflect.Session.new();
    s.eval<string>(
        `
            let v = ["a", "b"]
            v.length().to_string()
        `,
    )
}

test "method_calls_on_a_session_binding_work_in_its_own_submission" {
    assert.equal(main(), "2")
}
