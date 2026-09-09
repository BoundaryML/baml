// The compiler as an oracle for type spellings. Together with `verify.baml`
// this is the whole surface through which the tool learns about BAML.

/// The compiler's reading of `spelling`, given the source text of the
/// declarations it needs, or why it could not be read.
///
/// The spelling is declared as the type of a class field and read back
/// through the class's reflection. A field accepts any spelling, which an
/// alias or a call-site type argument would not: a function type cannot be
/// written inside `reflect.Type.of<...>()`, and an alias of a recursive alias
/// becomes a recursive alias of its own and reflects under the wrong name.
function type_of_spelling(declarations: string, spelling: string) -> Spelled throws never {
    let source = `
        ${declarations}
        class ProbeHolder {
            probed: ${spelling},
        }

        function probe() -> reflect.Type? throws never {
            match (reflect.Type.of<ProbeHolder>().as_class()) {
                let holder: reflect.class.Type => match (holder.fields().at(0)) {
                    let field: reflect.class.Field => field.type,
                    null => null,
                },
                null => null,
            }
        }
    `;
    match (compile({ "probe.baml": source })) {
        let failed: CompileFailed => failed,
        let threw: InitThrew => threw,
        let package: reflect.Package => {
            let probe = package.get_function<ProbeFn>("root.probe") catch (e) {
                reflect.errors.CompilationError => null,
            };
            // A function value never matches a function-typed pattern (see
            // `passes` in verify.baml), so this narrows by null check instead.
            if (probe != null) {
                match (probe()) {
                    let t: reflect.Type => t,
                    null => InitThrew { message: "the probe field's type did not reflect" },
                }
            } else {
                InitThrew { message: "probe function missing from the compiled package" }
            }
        },
    }
}

type Spelled = reflect.Type | CompileFailed | InitThrew;

/// A function type with a `throws` clause cannot be written inside the angle
/// brackets of a call, so the probe's type goes through an alias.
type ProbeFn = () -> reflect.Type? throws never;
