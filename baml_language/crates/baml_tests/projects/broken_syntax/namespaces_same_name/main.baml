// Root namespace: demonstrates that same-name types in different
// namespaces don't conflict in type position and in expression position
// (object literals + thrown literals).
function GetLlmText() -> root.llm.Response {
    root.llm.Response { text: "hello", tokens: 5 }
}

function GetHttpStatus() -> root.http.Response {
    root.http.Response { status: 200, body: "ok" }
}

// Locks in throw-fact resolution for `root.<ns>.<name>`-qualified object
// literals. Without cross-namespace fallback in throw_inference, this would
// resolve to `unknown` and lose the concrete thrown type from the throw set.
function ThrowsHttpResponse() throws root.http.Response {
    throw root.http.Response { status: 500, body: "boom" }
}
