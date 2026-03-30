// Root namespace: demonstrates that same-name types in different
// namespaces don't conflict. Uses root.* in type position only
// (expression-position root.* is deferred).
function GetLlmText() -> root.llm.Response {
    root.llm.Response { text: "hello", tokens: 5 }
}

function GetHttpStatus() -> root.http.Response {
    root.http.Response { status: 200, body: "ok" }
}
