// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0010
// docs:endmeta
// docs:start all
function main() -> string {
    let tokens = 12_000;
    tokens > 8_000 ? "split" : "send"
}
// docs:end all
