// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0004
// docs:endmeta
// docs:start all
function main() -> string {
    let cents = 285;
    "cost in cents: " + cents
}
// docs:end all
