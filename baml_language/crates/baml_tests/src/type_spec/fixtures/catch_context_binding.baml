// R2 (S15 residue): `catch (e, ctx)`: the AST field says "stack trace", but the binding
// is the full error CONTEXT (TIR resolves `baml.errors.Context`);
// S12 typed it `string`.
class Boom {
    m string
}

function boom() -> string throws user.Boom {
    throw Boom { m: "x" }
}

function f() -> string throws never {
    boom() catch (e, ctx) {
        Boom => ctx.to_string()
             // ^^^^^^^^^^^^^^^ string
    }
}
