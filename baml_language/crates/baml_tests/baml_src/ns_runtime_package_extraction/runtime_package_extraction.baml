// Native coverage for BEP-066 function extraction and ordinary function subtyping.

type RpeNeverThrowContract = (string) -> (string | int) throws never;

test "p7_omitted_throws_contract_infers_a_wildcard" {
    let pkg = reflect.Package.compile(
        {
            "main.baml": `
                function Risky(value: string) -> string throws string {
                    value
                }
            `,
        },
    );
    let risky = pkg.get_function<(string) -> string>("root.Risky") ?? throw "missing root.Risky";
    assert.equal(risky("accepted"), "accepted")
}

test "extraction_uses_function_subtyping_and_throw_wildcard" {
    let pkg = reflect.Package.compile(
        {
            "main.baml": `
                function Flexible(value: unknown) -> string throws string {
                    "accepted"
                }
            `,
        },
    );

    // The target accepts a wider input and returns a narrower output. Its
    // declared throw remains admissible because this contract omits `throws`.
    let inferred = pkg.get_function<(string) -> (string | int)>("root.Flexible")
        ?? throw "omitted throws rejected ordinary function subtyping";

    let explicit_never = pkg.get_function<RpeNeverThrowContract>("root.Flexible") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    if (explicit_never is string) {
        assert.equal(inferred("input").to_string() + "|" + explicit_never, "accepted|E0001")
    } else {
        throw "explicit throws never accepted a throwing function"
    }
}
