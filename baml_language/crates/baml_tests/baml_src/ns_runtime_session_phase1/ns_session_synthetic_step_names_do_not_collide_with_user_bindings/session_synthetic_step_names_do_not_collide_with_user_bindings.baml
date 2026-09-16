function main() -> int {
    let session = reflect.Session.new();
    session.eval(`let result = 5`);
    session.eval(
        `
            let stmt_1 = 7
            log.info("keep the second generated step")
        `,
    );
    session.eval<int>(`result + stmt_1`)
}

test "session_synthetic_step_names_do_not_collide_with_user_bindings" {
    assert.equal(main(), 12)
}
