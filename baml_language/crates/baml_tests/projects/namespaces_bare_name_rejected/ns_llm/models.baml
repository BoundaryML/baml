class Response {
    text string
}

// This bare reference to Config should fail — it's in root namespace,
// not in the llm namespace. Must use root.Config instead.
function UseConfig(cfg: Config) -> Response {
    Response { text: cfg.key }
}
