// Regression: a misspelled enum variant name in a TYPE position (match arm,
// type annotation) used to lower to `Ty::EnumVariant(Status, "Pendng", ...)`
// without checking that "Pendng" actually exists on `Status`. Validation in
// `lower_type_expr` should surface an unresolved-type / unknown-variant
// diagnostic at this site instead of silently producing a bogus variant
// type that confuses downstream type checking and exhaustiveness analysis.
enum Status {
    Active
    Inactive
    Pending
}

function classify(s: Status) -> string {
    match (s) {
        Status.Active => "active",
        Status.Inactive => "inactive",
        Status.Pendng => "pending"
    }
}
