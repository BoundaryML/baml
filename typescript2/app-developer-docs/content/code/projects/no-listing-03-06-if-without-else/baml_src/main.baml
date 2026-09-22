// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
// docs:endmeta
// docs:start all
function main() -> string {
    let tokens = 12_000;
    let label = if (tokens > 8_000) {
        "split"
    };
    label
}
// docs:end all
