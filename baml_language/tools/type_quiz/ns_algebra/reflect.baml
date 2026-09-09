// The bridge between the `Ty` model and the compiler's reflection of types.
//
// Two exhaustive matches make this file the tripwire for language growth: a
// new member of `reflect.TypeKind` breaks `kind_of`, and a new `Ty` variant
// breaks `expected_kind`, so neither can be added without deciding what the
// other side means.

/// The kinds the compiler classifies types into, mirroring `reflect.TypeKind`.
///
/// The compiler files `unknown`, `never`, recursive aliases, and the metatype
/// under `Primitive`; `expected_kind` encodes that so the conformance suite
/// notices if the classification changes.
enum Kind {
    Class,
    Enum,
    Union,
    Literal,
    Array,
    Map,
    Interface,
    Primitive,
    Function,
}

function kind_of(t: reflect.Type) -> Kind throws never {
    match (t.kind()) {
        reflect.class.Type => Kind.Class,
        reflect.enum.Type => Kind.Enum,
        reflect.union.Type => Kind.Union,
        reflect.literal.Type => Kind.Literal,
        reflect.array.Type => Kind.Array,
        reflect.map.Type => Kind.Map,
        reflect.interface.Type => Kind.Interface,
        reflect.primitive.Type => Kind.Primitive,
        reflect.function.Type => Kind.Function,
    }
}

/// The kind the compiler should report for `ty`, or `null` for a type
/// variable, which has no realized reflection.
function expected_kind(ty: Ty) -> Kind? throws never {
    match (ty) {
        Prim => Kind.Primitive,
        Never => Kind.Primitive,
        Top => Kind.Primitive,
        Lit => Kind.Literal,
        EnumLit => Kind.Literal,
        Union => Kind.Union,
        Array => Kind.Array,
        Map => Kind.Map,
        Fn => Kind.Function,
        Class => Kind.Class,
        Enum => Kind.Enum,
        Iface => Kind.Interface,
        // A recursive alias survives lowering as a name and reflects as a
        // primitive; a non-recursive one is transparent and reflects as its body.
        let a: Alias => {
            if (is_recursive(a.decl)) {
                Kind.Primitive
            } else {
                expected_kind(a.decl.body)
            }
        },
        Var => null,
    }
}

/// `ty` built through reflection's constructors instead of through source,
/// for the structural subset the constructors cover: primitives, literals of
/// builtins, unions, arrays, and maps. `null` for anything else.
///
/// This is the independent side of the render round trip: a type built here
/// never passes through `render`, so agreement with the parsed rendering is
/// evidence about `render` alone.
function to_reflect(ty: Ty) -> reflect.Type? throws never {
    match (ty) {
        let p: Prim => match (p.kind) {
            PrimKind.Int => reflect.Type.of<int>(),
            PrimKind.Float => reflect.Type.of<float>(),
            PrimKind.Bool => reflect.Type.of<bool>(),
            PrimKind.String => reflect.Type.of<string>(),
            PrimKind.Bigint => reflect.Type.of<bigint>(),
            PrimKind.Null => reflect.Type.of<null>(),
            PrimKind.Uint8Array => reflect.Type.of<uint8array>(),
        },
        Never => reflect.Type.of<never>(),
        Top => reflect.Type.of<unknown>(),
        let l: Lit => reflect.literal.new(l.value).as_type(),
        EnumLit => null,
        let u: Union => {
            let members = u.members.filter_map((m) -> {
                to_reflect(m)
            });
            if (members.length() != u.members.length()) {
                return null;
            }
            reflect.union.new(members).as_type() catch (e) {
                reflect.errors.CompilationError => null,
            }
        },
        let a: Array => match (to_reflect(a.element)) {
            let e: reflect.Type => e.array().as_type(),
            null => null,
        },
        let m: Map => match (to_reflect(m.key)) {
            let k: reflect.Type => match (to_reflect(m.value)) {
                let v: reflect.Type => reflect.map.new(k, v).as_type(),
                null => null,
            },
            null => null,
        },
        Fn => null,
        Class => null,
        Enum => null,
        Iface => null,
        Alias => null,
        Var => null,
    }
}
