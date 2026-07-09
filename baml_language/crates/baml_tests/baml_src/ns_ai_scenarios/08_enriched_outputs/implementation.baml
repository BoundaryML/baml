// scenario 08 — enriched outputs: ../../../../../llm-provider/ideas/scenarios/08-enriched-outputs/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design's teaching point: "enriched output" is NOT one capability. It is
// several, each provider-bound, and several have NO honest fallback when the
// provider lacks them:
//
//   * Logprobs     — OpenAI yes, Gemini yes, Anthropic NEVER.
//   * Citations    — Anthropic per-document spans into YOUR docs.
//   * Grounding    — Gemini groundingMetadata (chunk + answer-segment map).
//   * Annotations  — OpenAI URL/file citations riding on tool output.
//
// There is NO shared citation type, and the std lib does NOT promise
// cross-provider normalization — an app that wants one shape writes it itself
// (see usage.baml, s08_SourcedAnswer).
//
// CARRIER: every one of these is a PRODUCT sidecar (the answer AND the
// enrichment off the same response), so they ride the canonical value+sidecar
// path — the design's `call_with -> (T, U)` tuple, realized as the `$with`
// companion returning `CallResult<T, V>` (tuples aren't a language feature).
// On the real surface most of the design's implement.baml is ALREADY stdlib:
//   - `baml.ai.ResponseMeta` carries the common dimensions (usage,
//     finish_reason) plus the "can't"-typed ones (logprobs / citations /
//     reasoning) as `T | baml.ai.Unavailable` — the design's `Supported<T>`
//     spelled inline (generic type aliases aren't a feature).
//   - `baml.ai.Fallback` implements HttpProvider and forwards
//     `call_messages_with`, so the projection is NOT dropped under combinators
//     — the design's P1 win, reused unchanged (proven in usage.baml).
//   - The design's `AllProvidersFailed` concrete error is the one stdlib
//     divergence on that path: the real Fallback throws UnknownError
//     "fallback: all providers failed" instead of a typed class.
//
// What this file adds is exactly the design's genuinely-novel surface:
//   1. the three UN-unified wire shapes (doc-citations, grounding, annotations);
//   2. the three divergent dimensions as EXTERNAL-impl interfaces over the
//     provider's concrete ResponseMeta, reached by a runtime `match` inside
//     `project` — the same fork the error model took. (Real-surface note: the
//     design wrote bare `interface Cited`; matching a ResponseMeta value
//     against an unrelated interface doesn't narrow, so here each dimension
//     `requires baml.ai.ResponseMeta` — the Provider→HttpProvider shape.)
//   3. deterministic offline provider fakes whose parse_meta normalizes each
//     provider's enrichment — hand-rolled HttpProviders are justified here
//     because the enrichment lives in the codec's parse_meta leg.
//
// Logprob-shape divergence vs the design: the design's Logprobs carried
// per-token top-N alternatives and Gemini's length-normalized `avg`
// (avgLogprobs); the stdlib `baml.ai.Logprob` is the flat { token, logprob }.
// The base dimension is therefore lossy on `top`/`avg` — noted, not worked
// around: the sharp point (Anthropic CAN'T vs OpenAI/Gemini CAN) survives.


// ----------------------------------------------------------------------------
// Net-new value types: the wire shapes, lifted into BAML. Deliberately NOT
// unified — each mirrors one provider's response exactly.
// ----------------------------------------------------------------------------

/// Anthropic: a citation is a span into ONE of the documents YOU supplied.
/// Unit = your document + a char range + the quoted text.
class s08_DocCitation {
    document_index: int,
    start: int,
    end: int,
    cited_text: string,
}

/// A text block of the answer, carrying the doc-citations that back it.
/// Coverage is partial: `citations` may be empty for a block.
class s08_CitedBlock {
    text: string,
    citations: s08_DocCitation[],
}

/// Gemini: grounding is a SEPARATE object beside the content. Chunks are the
/// retrieved sources; supports map an answer SEGMENT to the chunk indices that
/// back it. Unit = retrieved chunk, NOT your document — a different object.
class s08_GroundingChunk {
    uri: string,
    title: string,
}

class s08_GroundingSupport {
    segment_text: string,
    chunk_indices: int[],
}

class s08_GroundingMetadata {
    chunks: s08_GroundingChunk[],
    supports: s08_GroundingSupport[],
}

/// OpenAI: sources arrive as ANNOTATIONS on tool output (web-search /
/// file-search), not a per-document toggle. Unit = a URL or a file span on a
/// slice of the answer.
class s08_Annotation {
    kind: string,         // "url_citation" | "file_citation"
    url: string?,         // for url_citation
    file_id: string?,     // for file_citation
    start: int,
    end: int,
}


// ----------------------------------------------------------------------------
// The divergent dimensions, as EXTERNAL-impl interfaces over the provider's
// concrete response meta. Logprobs needs no interface here — it is common
// enough to sit on the base `baml.ai.ResponseMeta` (there is no `Scored`
// companion at all; that is the design's headline change vs its old draft).
//
// Channel note: the design put these accessors on the CallError throws
// channel; the real stdlib convention for meta accessors is best-effort
// `throws never` (a missing wire shape yields a default), so these follow the
// house convention and stay infallible.
// ----------------------------------------------------------------------------

/// Anthropic-style: answer text broken into blocks, each carrying the document
/// spans that back it. (Base `citations()` on ResponseMeta answers the FLAT,
/// lossy `baml.ai.Citation[]`; `s08_Cited` adds the rich block structure.)
interface s08_Cited requires baml.ai.ResponseMeta {
  function cited_blocks(self) -> s08_CitedBlock[] throws never
}

/// Gemini-style: the grounding-metadata sidecar (answer-segment -> chunk).
interface s08_Grounded requires baml.ai.ResponseMeta {
  function grounding(self) -> s08_GroundingMetadata throws never
}

/// OpenAI-style: annotation spans from tool output.
interface s08_Annotated requires baml.ai.ResponseMeta {
  function annotations(self) -> s08_Annotation[] throws never
}


// ----------------------------------------------------------------------------
// Concrete per-provider ResponseMeta classes — the lazily-normalized response
// sidecars. Each implements the base interface plus EXACTLY the external
// dimensions its provider really has. Usage is zeroed here on purpose: token
// accounting is scenario 32/34's dimension; this scenario is about the
// enriched ones.
// ----------------------------------------------------------------------------

/// OpenAI-shaped meta: CAN score (logprobs list; the design's `avg` is null on
/// OpenAI — no slot on the flat stdlib Logprob, see header note) and carries
/// tool annotations. NOT Cited, NOT Grounded.
class s08_OpenAiMeta {
    tokens: baml.ai.Logprob[],
    anns: s08_Annotation[],

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: 0, output_tokens: 0 }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai chat: no reasoning text" }
        }
        // OpenAI CAN score — the list is "what this call carried" (possibly
        // empty when the logprobs knob was off), never the typed "can't".
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            self.tokens
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai: no doc citations" }
        }
    }

    implements s08_Annotated {
        // message.annotations[] (url_citation / file_citation); empty if no
        // search tool fired this call.
        function annotations(self) -> s08_Annotation[] throws never { self.anns }
    }
}

/// Gemini-shaped meta: CAN score (candidates[0].logprobsResult — and fills the
/// design's `avg`, which the flat stdlib Logprob cannot carry) and exposes
/// grounding. Same ResponseMeta dimension as OpenAI, divergent normalizer —
/// THAT is the point: one interface, provider-private wire code co-located
/// with the response. NOT Cited, NOT Annotated.
class s08_GeminiMeta {
    tokens: baml.ai.Logprob[],
    gm: s08_GroundingMetadata,

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "STOP" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: 0, output_tokens: 0 }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "gemini: no reasoning text here" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            self.tokens
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "gemini: grounding, not doc citations" }
        }
    }

    implements s08_Grounded {
        // candidates[0].groundingMetadata -> { groundingChunks, groundingSupports }
        function grounding(self) -> s08_GroundingMetadata throws never { self.gm }
    }
}

/// Anthropic-shaped meta — THE sharp divergence, made typed: Anthropic exposes
/// NO logprobs and NO logit_bias, ever. `logprobs()` returns the typed
/// `Unavailable` — "this provider CAN'T", not "empty this call" — so a caller
/// projecting `m.logprobs()` reads a VALUE back rather than catching a throw.
/// Base `citations()` answers the flat lossy list; the rich per-block spans
/// ride the external `s08_Cited` dimension. NOT Annotated, NOT Grounded.
class s08_AnthropicMeta {
    blocks: s08_CitedBlock[],

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "end_turn" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: 0, output_tokens: 0 }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic: none on this path" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic: no logprobs, no logit_bias" }
        }
        // The flat base dimension, flattened from the blocks. Lossy mapping
        // into the stdlib's { url, title } shape (a doc span has no URL):
        // url = "doc:<index>", title = the quoted text. The block structure
        // for callers that want it is s08_Cited below.
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            let flat: baml.ai.Citation[] = [];
            for (let b in self.blocks) {
                for (let c in b.citations) {
                    flat.push(baml.ai.Citation {
                        url: "doc:" + c.document_index.to_string(),
                        title: c.cited_text,
                    });
                }
            }
            flat
        }
    }

    implements s08_Cited {
        // content[].text + content[].citations[] as per-block structure.
        function cited_blocks(self) -> s08_CitedBlock[] throws never { self.blocks }
    }
}


// ----------------------------------------------------------------------------
// Offline fakes: one per provider shape, each running the REAL codec path
// (build_request -> send -> parse / parse_meta), with `send` answering a
// canned reply and `parse_meta` normalizing the canned enrichment. The
// design's request-side knobs (want_logprobs / want_websearch /
// want_citations, private to build_request) have their live analog in
// `extra_body` on the real providers (see the live tier in usage.baml); the
// fakes carry the enrichment payload directly instead.
// ----------------------------------------------------------------------------

/// OpenAI-shaped fake: scores every call, may carry annotations.
class s08_ScoredEcho {
    reply: string,
    tokens: baml.ai.Logprob[],
    anns: s08_Annotation[],

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            // A real provider adds logprobs:true / top_logprobs / the search
            // tool here when the knobs are set; the offline request is inert.
            baml.http.Request { method: "POST", url: "https://example.invalid/scored", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            self.reply
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s08 scored parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s08_OpenAiMeta { tokens: self.tokens, anns: self.anns }
        }
    }
}

/// Gemini-shaped fake: scores AND grounds.
class s08_GroundedEcho {
    reply: string,
    tokens: baml.ai.Logprob[],
    gm: s08_GroundingMetadata,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            baml.http.Request { method: "POST", url: "https://example.invalid/grounded", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            self.reply
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s08 grounded parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s08_GeminiMeta { tokens: self.tokens, gm: self.gm }
        }
    }
}

/// Anthropic-shaped fake: cites, but can NEVER score.
class s08_CitingEcho {
    reply: string,
    blocks: s08_CitedBlock[],

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            baml.http.Request { method: "POST", url: "https://example.invalid/citing", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            self.reply
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s08 citing parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s08_AnthropicMeta { blocks: self.blocks }
        }
    }
}
