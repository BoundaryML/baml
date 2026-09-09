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
// it does not also declare.

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
}

/// An enum variant used as a type: `Side.Left`.
class EnumLit {
    decl: EnumDecl,
    variant: string,
}

class Union {
    members: Ty[],
}

class Array {
    element: Ty,
}

class Map {
    key: Ty,
    value: Ty,
}

/// A positional parameter of a function type.
class Positional {
    ty: Ty,
}

/// A named parameter of a function type. Only named parameters can be
/// optional, which is why optionality lives here and not on `Positional`.
class Named {
    name: string,
    ty: Ty,
    optional: bool,
}

type Param = Positional | Named;

class Fn {
    params: Param[],
    ret: Ty,
    /// The `throws` type. Named `error` because `throws` is a keyword.
    error: Ty,
}

/// A class type, possibly an instantiation of a generic declaration.
class Class {
    decl: ClassDecl,
    args: Ty[],
}

class Enum {
    decl: EnumDecl,
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
}

/// A reference to a recursive alias by name. Non-recursive aliases are
/// transparent in BAML, so a generator spells those by their body instead.
class Alias {
    decl: AliasDecl,
}

/// A type variable in scope: a generic parameter or `Self`.
class Var {
    name: string,
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
}

/// A recursive alias. `body` refers back to the alias through `Alias`;
/// build one with `recursive_alias` so the knot is tied in one place.
class AliasDecl {
    name: string,
    body: Ty,
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

// ─── constructors ────────────────────────────────────────────────────────────

function int() -> Ty throws never {
    Prim { kind: PrimKind.Int }
}

function float() -> Ty throws never {
    Prim { kind: PrimKind.Float }
}

function bool() -> Ty throws never {
    Prim { kind: PrimKind.Bool }
}

function string() -> Ty throws never {
    Prim { kind: PrimKind.String }
}

function bigint() -> Ty throws never {
    Prim { kind: PrimKind.Bigint }
}

function null_ty() -> Ty throws never {
    Prim { kind: PrimKind.Null }
}

function uint8array() -> Ty throws never {
    Prim { kind: PrimKind.Uint8Array }
}

function never() -> Ty throws never {
    Never {  }
}

function unknown() -> Ty throws never {
    Top {  }
}

function lit(value: int | string | bool) -> Ty throws never {
    Lit { value: value }
}

/// `Side.Left` as a type.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if `variant` is not declared by `decl`: a
///   generator asked for a variant that does not exist.
function enum_lit(decl: EnumDecl, variant: string) -> Ty throws never {
    if (!decl.variants.includes(variant)) {
        throw baml.panics.Unreachable {
            message: `enum ${decl.name} has no variant ${variant}`,
        }
    }
    EnumLit { decl: decl, variant: variant }
}

function union(members: Ty[]) -> Ty throws never {
    Union { members: members }
}

/// `T?`, in its canonical form `T | null`.
function optional(ty: Ty) -> Ty throws never {
    Union { members: [ty, null_ty()] }
}

function array(element: Ty) -> Ty throws never {
    Array { element: element }
}

function map(key: Ty, value: Ty) -> Ty throws never {
    Map { key: key, value: value }
}

function fn(params: Param[], ret: Ty, error: Ty) -> Ty throws never {
    Fn { params: params, ret: ret, error: error }
}

function positional(ty: Ty) -> Param throws never {
    Positional { ty: ty }
}

function named(name: string, ty: Ty, optional: bool) -> Param throws never {
    Named { name: name, ty: ty, optional: optional }
}

function class_ty(decl: ClassDecl, args: Ty[]) -> Ty throws never {
    Class { decl: decl, args: args }
}

function enum_ty(decl: EnumDecl) -> Ty throws never {
    Enum { decl: decl }
}

function iface(decl: IfaceDecl, args: Ty[], pins: Pin[]) -> Ty throws never {
    Iface { decl: decl, args: args, pins: pins }
}

function var(name: string) -> Ty throws never {
    Var { name: name }
}

/// Declare a recursive alias. `body` receives the alias's own type so it can
/// refer back to it: `recursive_alias("Rec", (self) -> { union([int(), array(self)]) })`.
function recursive_alias(name: string, body: (Ty) -> Ty throws never) -> AliasDecl throws never {
    let decl = AliasDecl { name: name, body: never() };
    decl.body = body(Alias { decl: decl });
    decl
}
