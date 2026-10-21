// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0096
// docs:endmeta
// docs:start excerpt-18
// docs:start excerpt-03
class ToolUnavailable {
    tool: string,
    retry_after_ms: int,
}
// docs:end excerpt-03


function invoke_tool() -> string throws ToolUnavailable {
    throw ToolUnavailable { tool: "search", retry_after_ms: 500 }
}

function main() -> string throws never {
    invoke_tool()
}
// docs:end excerpt-18
