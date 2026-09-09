// The rule table: one entry per principle the spec states. Rules are values;
// items put them in traces, and naive models key their overrides by `name`.

function union_is_superset_of_member() -> root.engine.Rule throws never {
    root.engine.Rule {
        name: "subtyping/union-is-superset-of-member",
        spec: root.engine.Section { heading: "BAML Subtyping Cases" },
        statement: "unions: `T <: (T | ...)` for all `T`",
    }
}

/// Every rule in the bank.
function rules() -> root.engine.Rule[] throws never {
    [union_is_superset_of_member()]
}
