/// Coherence holds across submissions: the checker sees the whole replayed
/// package, so a later submission may not implement an interface for a type
/// an earlier submission already implemented it for.
function main() -> string {
    let s = reflect.Session.new();
    s.eval(
        `
            interface Greeter {
                function greet(self) -> string throws never { "hello" }
            }
            class Plain { n int }
            implements Greeter for Plain {}
        `,
    );
    let _ = s.eval(
        `
            implements Greeter for Plain {
                function greet(self) -> string throws never { "again" }
            }
        `,
    ) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].code;
        },
        _ => {
            return "wrong error";
        },
    };
    "unexpected success"
}

test "session_coherence_holds_across_submissions" {
    assert.equal(main(), "E0132")
}
