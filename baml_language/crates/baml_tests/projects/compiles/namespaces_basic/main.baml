// Root namespace: defines Config, references root.llm.Response
class Config {
    key string
    model string
}

// Cross-namespace reference: use type from ns_llm namespace
function GetResponse(cfg: Config) -> root.llm.Response {
    root.llm.Response { text: "hello", model: cfg.model }
}
