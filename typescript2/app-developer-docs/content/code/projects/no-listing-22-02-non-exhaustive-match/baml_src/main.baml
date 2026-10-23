// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0062
// docs:endmeta
// docs:start all
enum Outcome {
    Done
    Cancelled
}

function describe(event: Outcome | int) -> string {
    match (event) {
        Outcome.Done => "done",
        let tokens: int if tokens > 1_000 => "long reply",
        int => "short reply",
    }
}
// docs:end all
