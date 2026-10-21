function session_assignment_probe(session: reflect.Session, source: string) -> string {
    let _ = session.eval(source) catch (error) {
        reflect.errors.CompilationError => {
            return error.diagnostics[0].code + ": " + error.diagnostics[0].message;
        },
        baml.panics.Panic => {
            return "compiled, panicked at runtime";
        },
        _ => {
            return "wrong error";
        },
    };
    "accepted"
}

function ordinary_error(source: string) -> string throws never {
    let diagnostics = root.compiler.CompileDiagnostics({ "ordinary.baml": source });
    assert.equal(diagnostics.length(), 1);
    diagnostics[0].code + ": " + diagnostics[0].message
}

test "session assignment at another type fails like ordinary code" {
    let expected = ordinary_error(
        `
            function main() -> int {
                let n = 5
                n = "seven"
                n
            }
        `,
    );
    let session = reflect.Session.new();
    session.eval(`let n = 5`);
    let refusal = session_assignment_probe(session, `n = "seven"`);
    let kept = session.eval<int>(`n`);
    assert.equal(refusal + "|" + kept.to_string(), expected + "|5")
}

test "session assignment crash shape is rejected before execution" {
    let expected = ordinary_error(
        `
            function main() -> int {
                let n = 5
                n = "seven"
                n
            }
        `,
    );
    let session = reflect.Session.new();
    session.eval(`let n = 5`);
    let refusal = session_assignment_probe(session, `n = "seven"`);
    let still_an_int = session.eval<int>(`n.abs()`);
    assert.equal(refusal + "|" + still_an_int.to_string(), expected + "|5")
}

test "session compound assignment checks like ordinary code" {
    let expected = ordinary_error(
        `
            function main() -> string {
                let n = "seven"
                n += 1
                n
            }
        `,
    );
    let session = reflect.Session.new();
    session.eval(`let n = 5`);
    session.eval(`n += 1`);
    let after = session.eval<int>(`n`);
    session.eval(`let n = "seven"`);
    let refusal = session_assignment_probe(session, `n += 1`);
    assert.equal(after.to_string() + "|" + refusal, "6|" + expected)
}

test "session wider compound operand is no stricter than ordinary code" {
    let ordinary = `
        function main() -> int {
            let n = 5
            n += 1.5
            n
        }
    `;
    let expected = ordinary_error(ordinary);
    assert.equal(ordinary_error(ordinary.replace("n += 1.5", "n += (1.5)")), expected);

    let session = reflect.Session.new();
    session.eval(`let n = 5`);
    assert.equal(session_assignment_probe(session, `n += 1.5`), expected)
}
