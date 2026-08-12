class Response {
    text string
}

// Cross-namespace references require explicit root. prefix.
function UseConfig(cfg: root.Config) -> Response {
    Response { text: cfg.key }
}
