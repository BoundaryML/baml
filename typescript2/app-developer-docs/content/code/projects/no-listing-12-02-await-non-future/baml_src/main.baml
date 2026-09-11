// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
//       messageContains: "expected `baml.future.Future<unknown, unknown>`, found `42`"
// docs:endmeta
function answer() -> int {
    await 42
}
