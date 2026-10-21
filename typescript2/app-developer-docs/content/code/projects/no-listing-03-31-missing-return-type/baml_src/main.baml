// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0010
// docs:endmeta
// docs:start all
function report_budget(tokens: int) {
    log.info(tokens);
}

function main() -> int {
    report_budget(4_388);
    4_388
}
// docs:end all
