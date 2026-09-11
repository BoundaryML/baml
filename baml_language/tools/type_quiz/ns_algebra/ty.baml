// The type model the generators build cases from.
//
// This is tested material, not the compiler: it exists so a generator can
// construct types and declarations as data and hand the compiler their
// rendering. Nothing here decides subtyping or equivalence. `render.baml`
// spells a `Ty` as source, and the conformance suite checks that spelling
// against the compiler's own reading of it.
//
// Every variant that names a declaration carries the declaration itself, so a
// rendered case can never reference a class, enum, interface, or alias that
// it does not also declare. Shapes the language cannot spell are kept
// unrepresentable where the type system allows (a union has at least two
// members, a map's key is `string`, positional parameters precede named
// ones) and rejected by the constructor where it does not (generic arity,
// enum variants).

/// The builtin concrete types.
enum PrimKind {
    Int,
    Float,
    Bool,
    String,
    Bigint,
    Null,
    Uint8Array,
}

class Prim {
    kind: PrimKind,

    function int() -> Prim throws never {
        Prim { kind: PrimKind.Int }
    }

    function float() -> Prim throws never {
        Prim { kind: PrimKind.Float }
    }

    function bool() -> Prim throws never {
        Prim { kind: PrimKind.Bool }
    }

    function string() -> Prim throws never {
        Prim { kind: PrimKind.String }
    }

    function bigint() -> Prim throws never {
        Prim { kind: PrimKind.Bigint }
    }

    function null_ty() -> Prim throws never {
        Prim { kind: PrimKind.Null }
    }

    function uint8array() -> Prim throws never {
        Prim { kind: PrimKind.Uint8Array }
    }
}

/// The bottom type, spelled `never`.
class Never {
}

/// The top type, spelled `unknown`.
class Top {
}

/// A literal type of a builtin: `1`, `"sheep"`, `true`.
class Lit {
    value: int | string | bool,

    function new(value: int | string | bool) -> Lit throws never {
        Lit { value }
    }
}

/// An enum variant used as a type: `Side.Left`.
class EnumLit {
    decl: EnumDecl,
    variant: string,

    /// `Side.Left` as a type.
    ///
    /// # Panics
    ///
    /// - `baml.panics.Unreachable` if `variant` is not declared by `decl`.
    function new(decl: EnumDecl, variant: string) -> EnumLit throws never {
        if (!decl.variants.includes(variant)) {
            throw baml.panics.Unreachable { message: `enum ${decl.name} has no variant ${variant}` }
        }
        EnumLit { decl, variant }
    }
}

/// A union of at least two members. A one-member union is its member and a
/// zero-member union has no spelling, so neither is representable.
class Union {
    first: Ty,
    second: Ty,
    rest: Ty[],

    /// A union of `members`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.Unreachable` if fewer than two members are given.
    function new(members: Ty[]) -> Union throws never {
        match (members.at(0)) {
            let first: Ty => match (members.at(1)) {
                let second: Ty => Union { first, second, rest: members.slice(2, members.length()) },
                null => {
                    throw baml.panics.Unreachable { message: "a union needs at least two members" }
                },
            },
            null => throw baml.panics.Unreachable { message: "a union needs at least two members" },
        }
    }

    /// `T?`, in its canonical form `T | null`.
    function optional(ty: Ty) -> Union throws never {
        Union { first: ty, second: Prim.null_ty(), rest: [] }
    }

    /// The members, in order.
    function members(self) -> Ty[] throws never {
        [self.first, self.second].concat(self.rest)
    }
}

class Array {
    element: Ty,

    function new(element: Ty) -> Array throws never {
        Array { element }
    }
}

/// `map<string, V>`. Map keys are always `string` (E0067), so only the value
/// type is data.
class Map {
    value: Ty,

    function new(value: Ty) -> Map throws never {
        Map { value }
    }
}

/// A named parameter of a function type. Only named parameters can be
/// optional, which is why optionality lives here.
class Named {
    name: string,
    ty: Ty,
    optional: bool,

    function new(name: string, ty: Ty, optional: bool) -> Named throws never {
        Named { name, ty, optional }
    }
}

/// A function type: positional parameters, then named ones, a return type,
/// and the `throws` type (named `error` because `throws` is a keyword).
class Fn {
    positional: Ty[],
    named: Named[],
    ret: Ty,
    error: Ty,

    function new(positional: Ty[], named: Named[], ret: Ty, error: Ty) -> Fn throws never {
        Fn { positional, named, ret, error }
    }
}

/// A class type, possibly an instantiation of a generic declaration.
class Class {
    decl: ClassDecl,
    args: Ty[],

    /// `decl` instantiated with `args`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.Unreachable` if the argument count differs from the
    ///   declaration's parameter count.
    function new(decl: ClassDecl, args: Ty[]) -> Class throws never {
        if (args.length() != decl.params.length()) {
            throw baml.panics.Unreachable {
                message: `class ${decl.name} takes ${decl.params.length()} type arguments, got ${args.length()}`,
            }
        }
        Class { decl, args }
    }
}

class Enum {
    decl: EnumDecl,

    function new(decl: EnumDecl) -> Enum throws never {
        Enum { decl }
    }
}

/// An associated type pinned on an interface-existential: `Iter<Item = int>`.
class Pin {
    name: string,
    ty: Ty,
}

/// An interface used as an existential type.
class Iface {
    decl: IfaceDecl,
    args: Ty[],
    pins: Pin[],

    /// `decl` as an existential type with `args` and `pins`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.Unreachable` if the argument count differs from the
    ///   declaration's parameter count, or a pin names no associated type.
    function new(decl: IfaceDecl, args: Ty[], pins: Pin[]) -> Iface throws never {
        applies(decl, args, pins);
        Iface { decl, args, pins }
    }

    /// An interface that takes nothing, as a type.
    function of(decl: IfaceDecl) -> Iface throws never {
        Iface.new(decl, [], [])
    }
}

/// Whether `args` and `pins` apply to `decl`. Every use of an interface is
/// checked through here, so an existential and an `implements` block cannot
/// disagree about what applying one means.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the argument count differs from the
///   declaration's parameter count, or a pin names no associated type.
function applies(decl: IfaceDecl, args: Ty[], pins: Pin[]) -> void throws never {
    if (args.length() != decl.params.length()) {
        throw baml.panics.Unreachable {
            message: `interface ${decl.name} takes ${decl.params.length()} type arguments, got ${args.length()}`,
        }
    }
    let assoc = decl.assoc.map((a) -> {
        a.name
    });
    for (let pin in pins) {
        if (!assoc.includes(pin.name)) {
            throw baml.panics.Unreachable { message: `interface ${decl.name} has no associated type ${pin.name}` }
        }
    }
}

/// An `implements` block: the interface a class implements, its type
/// arguments, and the associated types the block fixes.
///
/// Implementation is nominal, so this is the only thing that makes its class
/// an implementor. A class that shares an interface's shape and declares no
/// block does not implement it.
class Impl {
    iface: IfaceDecl,
    args: Ty[],
    pins: Pin[],

    /// An `implements` block for `decl` with `args` and `pins`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.Unreachable` if the argument count differs from the
    ///   declaration's parameter count, or a pin names no associated type.
    function new(decl: IfaceDecl, args: Ty[], pins: Pin[]) -> Impl throws never {
        applies(decl, args, pins);
        Impl { iface: decl, args, pins }
    }

    /// An `implements` block for an interface that takes nothing.
    function of(decl: IfaceDecl) -> Impl throws never {
        Impl.new(decl, [], [])
    }
}

/// An alias spelled by name. Recursive aliases can only be spelled this way;
/// a non-recursive alias may be spelled by name (to exercise transparency) or
/// by its body.
class Alias {
    decl: AliasDecl,
}

/// A type variable in scope: a generic parameter or `Self`. The generator
/// that emits one is responsible for the enclosing declaration binding it.
///
/// A bound is spelled exactly as an existential of the same interface is
/// (`Base`, `Lorem<int>`, `Holder<Held = string>`), because the language
/// spells them the same way: what separates a universal from an existential
/// is the position, not the syntax. An unbounded variable admits any type;
/// a bounded one admits only concrete implementors.
class Var {
    name: string,
    bound: Iface?,

    function new(name: string) -> Var throws never {
        Var { name, bound: null }
    }

    /// A variable its binder writes as `<name extends bound>`.
    function bounded(name: string, bound: Iface) -> Var throws never {
        Var { name, bound }
    }
}

type Ty = Prim
    | Never
    | Top
    | Lit
    | EnumLit
    | Union
    | Array
    | Map
    | Fn
    | Class
    | Enum
    | Iface
    | Alias
    | Var;

/// The immediate child types of `ty`, in spelling order. The one traversal
/// every structural measure is written against.
function children(ty: Ty) -> Ty[] throws never {
    match (ty) {
        Prim | Never | Top | Lit | EnumLit | Enum | Alias | Var => [],
        let u: Union => u.members(),
        let a: Array => [a.element],
        let m: Map => [m.value],
        let f: Fn => {
            f.positional
                .concat(
                    f.named.map((n) -> {
                        n.ty
                    }),
                )
                .concat([f.ret, f.error])
        },
        let c: Class => c.args,
        let i: Iface => {
            i.args.concat(
                i.pins.map((p) -> {
                    p.ty
                }),
            )
        },
    }
}

/// Every type variable `tys` mention, in first-seen order, deduplicated by
/// name. A variable's bound is searched too, so a binder list built from
/// this one names everything the spelling needs.
///
/// A bound is not a child of the variable that carries it: `children` is
/// spelling order at the use site, where a variable is spelled by its name
/// alone, and the structural measures are written against that.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if two variables share a name and differ,
///   which would ask one binder to declare two variables.
function vars_of(tys: Ty[]) -> Var[] throws never {
    let out: Var[] = [];
    let seen: map<string, Var> = {};
    for (let ty in tys) {
        gather_vars(ty, out, seen);
    }
    out
}

function gather_vars(ty: Ty, out: Var[], seen: map<string, Var>) -> void throws never {
    match (ty) {
        let v: Var => {
            match (seen.get(v.name)) {
                let prior: Var => {
                    if (prior != v) {
                        throw baml.panics.Unreachable {
                            message: `two different type variables named ${v.name} in one case`,
                        }
                    }
                },
                null => {
                    seen.set(v.name, v);
                    out.push(v);
                    match (v.bound) {
                        let i: Iface => gather_vars(i, out, seen),
                        null => {},
                    }
                },
            }
        },
        Prim | Never | Top | Lit | EnumLit | Union | Array | Map | Fn | Class | Enum | Iface | Alias => {},
    }
    for (let child in children(ty)) {
        gather_vars(child, out, seen);
    }
}

/// Whether an `implements` block may name `ty` as the type it implements for.
///
/// The spec's rule is that only concrete types may implement an interface,
/// which the Taxonomy defines as the primitives, classes, enums, function
/// types and a few builtins. Two of those are held out because the compiler
/// and the spec disagree about them today, both recorded rather than guessed:
/// a function type is concrete but rejected (CD-002), and `never` is accepted
/// though it belongs to none of the three groups (G-004). A type variable is
/// held out for a different reason: `implement<T> I for T {}` is a blanket
/// implementation, a construct of its own rather than a target. An
/// interface-existential is abstract, so it is no more a target than a union
/// is.
///
/// An alias stands for its body, so it is a target exactly when its body is
/// one. A recursive alias never is: its body is a union or a container of
/// itself, and asking would not terminate.
function is_target(ty: Ty) -> bool throws never {
    match (ty) {
        Prim | Class | Enum | Array | Map => true,
        Lit | EnumLit | Union | Top | Never | Fn | Var | Iface => false,
        let a: Alias => {
            if (is_recursive(a.decl)) {
                false
            } else {
                is_target(a.decl.body)
            }
        },
    }
}

/// Whether `ty` mentions a type variable, and so needs a binder in scope.
function mentions_var(ty: Ty) -> bool throws never {
    vars_of([ty]).length() > 0
}

// ─── declarations ────────────────────────────────────────────────────────────

class FieldDecl {
    name: string,
    ty: Ty,
}

class ClassDecl {
    name: string,
    /// Generic parameter names, in order.
    params: string[],
    fields: FieldDecl[],
    /// The interfaces this class implements, as in-body blocks.
    impls: Impl[],
}

class EnumDecl {
    name: string,
    variants: string[],
}

/// An associated type declared on an interface, with its default if any.
class AssocDecl {
    name: string,
    default: Ty?,
}

class IfaceDecl {
    name: string,
    params: string[],
    assoc: AssocDecl[],
    /// The interfaces an implementor must also implement: the declaration's
    /// `requires` clause, which is spelled `bounds` here because `requires`
    /// is a keyword. Each is an applied interface, carrying its own
    /// arguments and pins.
    bounds: Iface[],
}

/// An alias. A recursive one refers back to itself through `Alias`; build
/// those with `recursive_alias` so the knot is tied in one place.
class AliasDecl {
    name: string,
    body: Ty,

    /// Declare a recursive alias. `body` receives the alias's own type so it
    /// can refer back to it:
    /// `AliasDecl.recursive("Rec", (self) -> { Union.new([Prim.int(), Array.new(self)]) })`.
    function recursive(name: string, body: (Ty) -> Ty throws never) -> AliasDecl throws never {
        let decl = AliasDecl { name, body: Never {  } };
        decl.body = body(Alias { decl });
        decl
    }
}

type Decl = ClassDecl | EnumDecl | IfaceDecl | AliasDecl;

function decl_name(decl: Decl) -> string throws never {
    match (decl) {
        let d: ClassDecl => d.name,
        let d: EnumDecl => d.name,
        let d: IfaceDecl => d.name,
        let d: AliasDecl => d.name,
    }
}
