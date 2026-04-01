// Hard assertions — throw and stop execution on failure.
// These remain $rust_function because cross-package calls (e.g. baml.sys.panic)
// are not yet supported in the parser, and operator precedence for !expr.method()
// binds incorrectly.

function is_true(condition: bool) -> null { $rust_function }
function not_null(value: unknown?) -> null { $rust_function }
function equal(actual: unknown, expected: unknown) -> null { $rust_function }
function contains(haystack: string, needle: string) -> null { $rust_function }
