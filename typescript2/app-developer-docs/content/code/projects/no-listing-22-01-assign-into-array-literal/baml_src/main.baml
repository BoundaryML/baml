// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0010
// docs:endmeta
// docs:start all
function main() -> string {
    let tool = "";
    let argument = "";
    [tool, argument] = ["search", "baml"];
    `${tool}(${argument})`
}
// docs:end all
