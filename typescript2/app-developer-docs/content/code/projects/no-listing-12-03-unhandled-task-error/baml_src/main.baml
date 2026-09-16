// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0096
// docs:endmeta
class LookupFailed {
    key: string,
}

function lookup() -> string {
    throw LookupFailed { key: "missing" }
}

function answer() -> string throws never {
    let future = spawn { lookup() };
    await future
}
