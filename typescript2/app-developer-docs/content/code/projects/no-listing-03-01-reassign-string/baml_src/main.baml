// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
// docs:endmeta
// docs:start all
function main() -> int {
    let remaining = 8_000;
    remaining = "spent";
    remaining
}
// docs:end all
