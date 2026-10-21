// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
// docs:endmeta
// docs:start all
function main() -> float {
    let cents = 285;
    let dollars: float = cents / 100;
    dollars
}
// docs:end all
