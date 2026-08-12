// Assignment is statement-only in BAML, so an assignment operator in
// expression position (e.g. `(x = 5)`) has no value and is rejected at
// lowering. This previously lowered to a `Missing` that only failed at
// runtime (the former `paren_assign_must_error` baml_src test).

function assignment_in_expression_position() -> int {
    let x = 10;
    let y = (x = 5);
    y
}
