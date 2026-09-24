// Shared isolation helpers for native ports of Rust integration fixtures.
// Each fixture is compiled as its own root package so canonical names and
// reflect.Package.current() retain the semantics of the original test.

function compile(source: string) -> reflect.Package {
    reflect.Package.compile({ "case.baml": source })
}

function call(source: string, entry: string) -> unknown {
    let package = compile(source);
    let function_ = package.get_function<() -> unknown>(entry) ?? throw `missing ${entry}`;
    function_()
}

function string_result(value: unknown) -> string {
    let result: string = value else {
        throw "expected string result"
    };
    result
}

function int_result(value: unknown) -> int {
    let result: int = value else {
        throw "expected int result"
    };
    result
}

function bool_result(value: unknown) -> bool {
    let result: bool = value else {
        throw "expected bool result"
    };
    result
}
