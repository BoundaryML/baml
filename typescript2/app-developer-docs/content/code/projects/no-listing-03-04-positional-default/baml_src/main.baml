// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0005
// docs:endmeta
// docs:start all
function estimate_tokens(text: string, chars_per_token: int = 4) -> int {
    text.trim().length() / chars_per_token
}

function main() -> int {
    estimate_tokens("user: Where is my order?", 3)
}
// docs:end all
