/// A later submission's `implements` block may bypass to a default body of
/// an interface an EARLIER submission declared: the body is an import of the
/// session's own package, reached through its package-private body table.
function main() -> string {
    let s = reflect.Session.new();
    s.eval(
        `
            interface Greeter {
                function greet(self) -> string throws never { "hello" }
            }
            class Plain {
                n int
                implements Greeter {}
            }
        `,
    );
    s.eval(
        `
            class Loud {
                n int
                implements Greeter {
                    function greet(self) -> string throws never { default.greet() + "!" }
                }
            }
        `,
    );
    s.eval<string>(`Loud { n: 1 }.greet() + " " + Plain { n: 2 }.greet()`)
}

test "session_impl_bypasses_to_an_earlier_submissions_default_body" {
    assert.equal(main(), "hello! hello")
}
