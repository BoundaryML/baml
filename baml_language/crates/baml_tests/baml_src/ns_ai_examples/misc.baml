// Remaining scenarios (13, 16, 25, 46) as compiling usage examples, plus a note for 06.

// 13 — deferred & searchable tools: a catalog + a resolver that pages in matching tools.
class Catalog {
    tools: baml.ai.Tool[],
}

function search_tools(cat: Catalog, query: string) -> baml.ai.Tool[] throws never {
    cat.tools
        .filter(
            (t: baml.ai.Tool) -> bool {
                t.name.includes(query) || t.description.includes(query)
            },
        )
}

// 16 — agent security: an allowlist/taint gate slots into the dispatcher; a denied call is a
// distinct outcome (here a marked ToolResult). (Compile-time info-flow is out of reach — §3.)
class Tainted {
    value: string,
    labels: string[],
}

function security_gated_dispatch(
    calls: baml.ai.ToolCall[],
    allowed: string[],
) -> baml.ai.ToolResult[] throws never {
    let results: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let out = if (allowed.includes(c.name)) {
            "ran " + c.name
        } else {
            "DENIED (policy): " + c.name
        };
        results.push(baml.ai.ToolResult { id: c.id, output: out });
    }
    results
}

// 25 — cascaded voice: STT + LLM + TTS as three single-capability providers wired together.
interface Stt {
    function transcribe(self, audio_url: string) -> string throws never
}

interface Tts {
    function synthesize(self, text: string) -> string throws never
}

function cascaded_voice(
    stt: Stt,
    llm: baml.ai.Provider,
    tts: Tts,
    audio_url: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    let text = stt.transcribe(audio_url);
    let reply = match (llm) {
        let h: baml.ai.HttpProvider => h.call<string>(text),
        _ => throw baml.errors.Unsupported { message: "llm stage cannot produce a value" },
    };
    tts.synthesize(reply)
}

// 46 — workflow observability: a stream of typed step events over a run.
class StepEvent {
    step: string,
    status: string,
}

function observe_pipeline(steps: string[]) -> StepEvent[] throws never {
    let events: StepEvent[] = [];
    for (let s in steps) {
        events.push(StepEvent { step: s, status: "done" });
    }
    events
}

// 06 — non-text output: an image model is an output-specialized HttpProvider whose `Body`
// decodes bytes and `parse<image>` yields media (would live in stdlib alongside OpenAi; the
// current provider is text-only). Shape of the request side:
function image_gen_prompt(description: string) -> string throws never {
    "Generate an image of: " + description
}
