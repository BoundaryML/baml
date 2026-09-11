// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0139
// docs:endmeta
// docs:start excerpt-42
implements baml.ToString for int {
    function to_string(self) -> string {
        `integer ${self}`
    }
}
// docs:end excerpt-42
