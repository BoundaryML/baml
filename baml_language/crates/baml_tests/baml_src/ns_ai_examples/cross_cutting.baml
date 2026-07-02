// Cross-cutting family (scenarios 05, 12, 33, 35, 36). Usage patterns over the real model.

// 36 — capability negotiation: query the graded Support lattice up front.
function negotiate_before_send(p: baml.ai.Provider) -> string throws never {
    baml.ai.example_negotiate(p)
}

// 33 — evaluation: the task is already a function->Provider; a judge is just another
// provider; the eval runner scores task output. `Scorer`/`Deterministic` are the net-new
// bits. (Reproducibility can't be a compile-time guarantee over the existential — D3.)
class Score {
    value: float,
    reason: string,
}

interface Scorer {
    function score(self, input: string, output: string) -> Score throws never
}

interface Deterministic requires baml.ai.Provider {
    function fingerprint(self) -> string throws never
}

function run_eval(
    task: baml.ai.Provider,
    judge: Scorer,
    input: string,
) -> Score throws baml.errors.CallError | baml.errors.UnknownError {
    let output = match (task) {
        let h: baml.ai.HttpProvider => h.call<string>(input),
        _ => throw baml.errors.Unsupported { message: "task client cannot produce a value" },
    };
    judge.score(input, output)
}

// 35 — deployment shapes: four homes collapse to four small functions over one class
// (config variance — the "Clean" axis). Auth/transport differ; the provider class doesn't.
function server_client() -> baml.ai.Provider throws baml.errors.Io | baml.errors.ParseError {
    baml.ai.OpenAi {
        model: "gpt-5.4-mini",
        api_key: baml.env.get("OPENAI_API_KEY") ?? "",
        base_url: null,
    }
}

function edge_client(gateway_url: string, token: string) -> baml.ai.Provider throws never {
    baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: token, base_url: gateway_url }
}

function browser_client(proxy_url: string) -> baml.ai.Provider throws never {
    // No API key in the browser: the key lives behind the proxy (base_url).
    baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: "", base_url: proxy_url }
}

// 12 — tool taxonomy: execution-location is a marker on the tool.
enum ToolKind {
    InProcess,
    Hosted,
    Mcp,
    ComputerUse,
}

class KindedTool {
    tool: baml.ai.Tool,
    kind: ToolKind,
}

function describe_tool_kind(kt: KindedTool) -> string throws never {
    match (kt.kind) {
        ToolKind.InProcess => "runs in this process",
        ToolKind.Hosted => "runs provider-side",
        ToolKind.Mcp => "runs on an MCP server",
        ToolKind.ComputerUse => "operates a computer",
    }
}

// 05 — multimodal input: build a request that carries an image part. (A media-capable
// provider encodes it into the wire; the current OpenAi provider is text-only, so this
// shows the shape — image construction via the real baml.media surface.)
function image_prompt(url: string) -> string throws never {
    let img = baml.media.Image.from_url(url, null);
    "Describe this image: " + (img.url() ?? "(inline)")
}
