// A submission is a sequence of statements: `let _ = e` runs `e` and binds
// nothing, and a submission that closes the block it runs in is an error, not
// an internal one.

test "let underscore runs its initializer and binds nothing" {
    let s = reflect.Session.new();
    assert.equal(s.eval<int>("let _ = 1;\n2"), 2);
    assert.equal(s.eval<int>("let base = 40;\nlet _ = base + 1;\nbase + 2"), 42)
}

test "a submission cannot close the enclosing block" {
    let s = reflect.Session.new();
    let outcome = s.eval<unknown>("1 }\nlet y = 2;\nlet z = {") catch (error) {
        reflect.errors.CompilationError => "compile:" + error.diagnostics[0].code,
    };
    assert.equal(outcome, "compile:E0010")
}
