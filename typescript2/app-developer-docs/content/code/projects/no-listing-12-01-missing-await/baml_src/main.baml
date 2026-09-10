// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
//       messageContains: "expected `int`, found `baml.future.Future<int, never>`"
// docs:endmeta
function answer() -> int {
    spawn { 42 }
}
