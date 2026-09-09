// The rule table: one entry per principle the spec states, quoted from it.
// Rules are values; facts put them in derivations, and naive models key
// their overrides by `name`. A rule with no section to cite names an entry
// in SPEC_GAPS.md instead.

function section(heading: string) -> root.engine.SpecRef throws never {
    root.engine.Section { heading: heading }
}

function gap(id: string) -> root.engine.SpecRef throws never {
    root.engine.Gap { id: id }
}

function rule(
    name: string,
    spec: root.engine.SpecRef,
    statement: string,
) -> root.engine.Rule throws never {
    root.engine.Rule { name: name, spec: spec, statement: statement }
}

// ─── flow ────────────────────────────────────────────────────────────────────

/// The principle every flow probe rests on. The spec never says which
/// positions are subtyping checks, so this cites the gap ledger.
function flow_requires_subtype() -> root.engine.Rule throws never {
    rule(
        "flow/requires-subtype",
        gap("G-001"),
        "A value of type `S` is accepted where type `T` is expected exactly when `S <: T`.",
    )
}

// ─── taxonomy ────────────────────────────────────────────────────────────────

function concrete_types_atomic() -> root.engine.Rule throws never {
    rule(
        "taxonomy/concrete-types-atomic",
        section("Concrete Types"),
        "As of writing, no concrete types have subtyping relationships with each other.",
    )
}

function literal_subtype_of_concrete() -> root.engine.Rule throws never {
    rule(
        "taxonomy/literal-subtype-of-concrete",
        section("Literal Types"),
        "Literal types represent subsets of concrete types. They correspond to exactly one concrete type (literal type `1` is of concrete type `int`).",
    )
}

function never_omitted_from_unions() -> root.engine.Rule throws never {
    rule(
        "never/omitted-from-unions",
        section("`never`"),
        "As an empty set, `never` can always be omitted from union types: `int | never` is equivalent to `int`.",
    )
}

// ─── subtyping cases ─────────────────────────────────────────────────────────

function never_bottom_rule() -> root.engine.Rule throws never {
    rule(
        "subtyping/never-bottom",
        section("BAML Subtyping Cases"),
        "bottom type: `never <: T` for all `T`",
    )
}

function unknown_top_rule() -> root.engine.Rule throws never {
    rule(
        "subtyping/unknown-top",
        section("BAML Subtyping Cases"),
        "top type: `T <: unknown` for all `T`",
    )
}

function identity_rule() -> root.engine.Rule throws never {
    rule("subtyping/identity", section("BAML Subtyping Cases"), "identity: `T =: T` for all `T`")
}

function union_member_rule() -> root.engine.Rule throws never {
    rule(
        "subtyping/union-member",
        section("BAML Subtyping Cases"),
        "unions: `T <: (T | ...)` for all `T`",
    )
}

function union_subset_rule() -> root.engine.Rule throws never {
    rule(
        "subtyping/union-subset",
        section("BAML Subtyping Cases"),
        "Including union subsets: `(T0 | T1) <: (T0 | T1 | ...)`",
    )
}

function bool_is_true_or_false() -> root.engine.Rule throws never {
    rule("subtyping/bool-is-true-or-false", section("BAML Subtyping Cases"), "`(true | false) == bool`")
}

function enum_is_union_of_variants() -> root.engine.Rule throws never {
    rule(
        "subtyping/enum-is-union-of-variants",
        section("BAML Subtyping Cases"),
        "`(Enum.VariantA | Enum.VariantB | ...) == Enum` (for each enum type)",
    )
}

function literal_absorbed() -> root.engine.Rule throws never {
    rule(
        "subtyping/literal-absorbed",
        section("BAML Subtyping Cases"),
        "In nearly all contexts, we can thus simplify types and maintain equivalence: `1 | int == int | 1 == int`",
    )
}

function unknown_absorbs() -> root.engine.Rule throws never {
    rule(
        "subtyping/unknown-absorbs",
        section("BAML Subtyping Cases"),
        "In nearly all contexts, we can thus simplify types and maintain equivalence: `anything | unknown == unknown`",
    )
}

function union_commutative() -> root.engine.Rule throws never {
    rule("subtyping/union-commutative", section("Interface Coherence"), "unions (`|`) are commutative")
}

function union_associative() -> root.engine.Rule throws never {
    rule("subtyping/union-associative", section("Interface Coherence"), "unions (`|`) are associative")
}

// ─── variance ────────────────────────────────────────────────────────────────

function union_member_covariant() -> root.engine.Rule throws never {
    rule(
        "variance/union-member-covariant",
        section("Variance"),
        "Union members are covariant: all members are subtypes of the union (as are their subtypes).",
    )
}

function generics_invariant() -> root.engine.Rule throws never {
    rule(
        "variance/generics-invariant",
        section("Variance"),
        "type parameters on classes/arrays/maps (generics) are invariant.",
    )
}

function fn_param_contravariant() -> root.engine.Rule throws never {
    rule(
        "variance/function-parameter-contravariant",
        section("Variance"),
        "Arguments must be contravariant (function A is subtype only if its arguments are supertype)",
    )
}

function fn_return_covariant() -> root.engine.Rule throws never {
    rule(
        "variance/function-return-covariant",
        section("Variance"),
        "Return and error types must be covariant (function A is subtype only if its return and error types are subtype)",
    )
}

function fn_error_covariant() -> root.engine.Rule throws never {
    rule(
        "variance/function-error-covariant",
        section("Variance"),
        "Return and error types must be covariant (function A is subtype only if its return and error types are subtype)",
    )
}

function fn_optional_superset() -> root.engine.Rule throws never {
    rule(
        "functions/optional-arguments-superset",
        section("BAML Subtyping Cases"),
        "Each optional argument `boN` has a corresponding `aoN` of the same name where `boN <: aoN` (contravariant, subset function type has superset of optional args)",
    )
}

function fn_positional_count() -> root.engine.Rule throws never {
    rule(
        "functions/same-positional-count",
        section("BAML Subtyping Cases"),
        "Same number of positional arguments where each `bN <: aN` (contravariant)",
    )
}

// ─── aliases ─────────────────────────────────────────────────────────────────

function alias_transparent() -> root.engine.Rule throws never {
    rule(
        "aliases/transparent",
        section("Type Aliases and Recursive Types"),
        "Type aliases are transparent (structural, not nominal): `type X = T` makes `X` the very same type as `T` in every context.",
    )
}

function equirecursive_identity() -> root.engine.Rule throws never {
    rule(
        "aliases/equirecursive-identity",
        section("Equirecursive identity"),
        "two types are the same type if and only if their trees are equal under the type algebra. Alias names, unfolding depth, and spelling never contribute identity",
    )
}

function partial_unfolding() -> root.engine.Rule throws never {
    rule(
        "aliases/partial-unfolding",
        section("Equirecursive identity"),
        "So are `A`, `int | A[]`, and `int | (int | A[])[]` (partial unfoldings)",
    )
}

function unguarded_member_vacuous() -> root.engine.Rule throws never {
    rule(
        "aliases/unguarded-member-vacuous",
        section("Productivity"),
        "an unguarded recursive union member contributes nothing to its type",
    )
}

function unguarded_cycle_rejected() -> root.engine.Rule throws never {
    rule(
        "aliases/unguarded-cycle-rejected",
        section("Productivity"),
        "a fully unguarded cycle is uninhabited: `type B = B` denotes `never` (and is a compile error, E0068)",
    )
}

/// Every rule in the bank.
function rules() -> root.engine.Rule[] throws never {
    [
        flow_requires_subtype(),
        concrete_types_atomic(),
        literal_subtype_of_concrete(),
        never_omitted_from_unions(),
        never_bottom_rule(),
        unknown_top_rule(),
        identity_rule(),
        union_member_rule(),
        union_subset_rule(),
        bool_is_true_or_false(),
        enum_is_union_of_variants(),
        literal_absorbed(),
        unknown_absorbs(),
        union_commutative(),
        union_associative(),
        union_member_covariant(),
        generics_invariant(),
        fn_param_contravariant(),
        fn_return_covariant(),
        fn_error_covariant(),
        fn_optional_superset(),
        fn_positional_count(),
        alias_transparent(),
        equirecursive_identity(),
        partial_unfolding(),
        unguarded_member_vacuous(),
        unguarded_cycle_rejected(),
    ]
}
