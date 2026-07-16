// ============================================================================
// Scenario 08 — Enriched outputs: logprobs, citations, grounding
//
// STD-LIB / library-author code.
//
// The teaching point of this scenario is that "enriched output" is NOT one
// capability. It is several, each provider-bound, and several of them have NO
// honest fallback when the provider lacks them:
//
//   * Logprobs        — OpenAI yes, Gemini yes, Anthropic NEVER.
//   * Citations       — Anthropic per-document spans into YOUR docs.
//   * Grounding       — Gemini groundingMetadata (chunk + answer-segment map).
//   * Annotations     — OpenAI URL/file citations riding on tool output.
//
// There is NO shared citation type. Anthropic's "span into my doc",
// Gemini's "answer-segment -> retrieved-chunk", and OpenAI's "tool annotation"
// are three different objects. We DON'T promise cross-provider normalization
// (an app can normalize itself — see usage.baml).
//
// CARRIER (the change vs. the old draft): every one of these is a PRODUCT
// sidecar — you get the answer AND the enrichment off the same response. So
// they ride the canonical `call_with` + `ResponseMeta` path (one tuple `(T, U)`
// return), NOT a bespoke `Scored$Result<T>` companion per feature. `call_with`
// is an HttpProvider capability (../provider-as-marker.md): `Provider` is a bare
// marker, and `call`/`call_with` belong to the request/response capability that
// owns their shape. `ResponseMeta` is an interface; each provider's concrete
// response type normalizes its wire response LAZILY, and "this provider CAN'T" is
// the typed `Unsupported` (via `Supported<T>`), not a null. Because `Fallback`
// implements HttpProvider and forwards `call_with` to an HttpProvider member, the
// sidecar is NOT dropped under combinators (the P1 win). The genuinely-novel dimensions (citations into your
// docs, grounding, annotations) arrive as EXTERNAL-impl interfaces reached by a
// runtime `match` inside `project` — the same fork the error model took.
//
// Everything here builds on the canonical primitives in _conventions.md.
// Net-new surface is flagged in evaluation.md.
// ============================================================================


// ----------------------------------------------------------------------------
// Net-new value types. These are the wire shapes, lifted into BAML.
// Deliberately NOT unified — each mirrors one provider's response exactly.
// (Unchanged: these are the contents of the sidecars, whatever carries them.)
// ----------------------------------------------------------------------------

// One scored position in the decoded sequence. Tokenizer-bound: a "word" may be
// several of these, so a consumer that wants per-WORD confidence must combine
// subword entries itself (see background "What's hard").
class TokenLogprob {
  token: string
  logprob: float
  top: TopAlternative[]      // the top-N alternatives the model weighed; may be empty
}

class TopAlternative {
  token: string
  logprob: float
}

// The whole scored result. `avg` is present only where the provider gives a
// length-normalized aggregate (Gemini's avgLogprobs); OpenAI leaves it null.
class Logprobs {
  tokens: TokenLogprob[]
  avg: float?
}

// Anthropic: a citation is a span into ONE of the documents YOU supplied.
// Unit = your document + a char/page/block range + the quoted text.
class DocCitation {
  document_index: int
  start: int
  end: int
  cited_text: string
}

// A text block of the answer, carrying the doc-citations that back it.
// Coverage is partial: `citations` may be empty for a block.
class CitedBlock {
  text: string
  citations: DocCitation[]
}

// Gemini: grounding is a SEPARATE object beside the content. Chunks are the
// retrieved sources; supports map an answer SEGMENT to the chunk indices that
// back it. Unit = retrieved chunk, NOT your document — a different object.
class GroundingChunk {
  uri: string
  title: string
}

class GroundingSupport {
  segment_text: string
  chunk_indices: int[]
}

class GroundingMetadata {
  chunks: GroundingChunk[]
  supports: GroundingSupport[]
}

// OpenAI: sources arrive as ANNOTATIONS on tool output (web-search / file-search),
// not a per-document toggle. Unit = a URL or a file span on a slice of the answer.
class Annotation {
  kind: string          // "url_citation" | "file_citation"
  url: string?          // for url_citation
  file_id: string?      // for file_citation
  start: int
  end: int
}


// ----------------------------------------------------------------------------
// The sidecar surface. `ResponseMeta` (from _conventions.md) already carries the
// dimensions every response can answer (usage, finish_reason) plus the common
// "can't"-typed ones (`logprobs`, `citations`, `reasoning`) as `Supported<T>`.
// Logprobs is common enough to sit on the base `ResponseMeta` interface, so we
// reuse it verbatim — there is no `Scored` companion here at all.
//
// The genuinely-divergent dimensions (Anthropic doc-citations, Gemini grounding,
// OpenAI tool annotations) are three DIFFERENT objects with no shared type, so
// they are EXTERNAL-impl interfaces over the provider's concrete response —
// added beside the response type, reached by a runtime `match` inside `project`.
// This is the §7 "novel dimension, external impl" pattern, not a new companion.
// ----------------------------------------------------------------------------

// Anthropic-style: answer text broken into blocks, each carrying the document
// spans that back it. (`citations()` on the base ResponseMeta returns the flat
// DocCitation[]; `Cited` adds the block structure for callers that want it.)
interface Cited {
  function cited_blocks(self) -> CitedBlock[]
      throws baml.ExtendUnknownError<baml.errors.CallError>
}

// Gemini-style: the grounding-metadata sidecar (answer-segment -> chunk).
interface Grounded {
  function grounding(self) -> GroundingMetadata
      throws baml.ExtendUnknownError<baml.errors.CallError>
}

// OpenAI-style: annotation spans from tool output.
interface Annotated {
  function annotations(self) -> Annotation[]
      throws baml.ExtendUnknownError<baml.errors.CallError>
}


// ----------------------------------------------------------------------------
// OpenAI — HttpProvider; its OpenAIResponse implements ResponseMeta (logprobs +
// usage) and the external `Annotated` dimension. NOT Cited, NOT Grounded.
//
// The enrichment knobs (logprobs:true / top_logprobs, the web-search tool) are
// request-side and stay private to build_request — they are NOT client fields
// and NOT a separate companion method. `call_with` does the one codec pass; the
// projection reads whichever sidecar the caller asked for off `meta_of`.
// ----------------------------------------------------------------------------
class OpenAI {
  model: string
  api_key: string?
  max_tokens: int?
  // request-side enrichment knobs, threaded into build_request (NOT client fields
  // the app sets per call — see usage.baml; provider-private, like top_logprobs).
  want_logprobs: int?       // top_logprobs N; null = off
  want_websearch: bool?     // attach the web/file-search tool so annotations can fire

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = OpenAIResponse

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // plain completion: schema goes in via response_format (json_schema, strict);
      // logprobs:true + top_logprobs and the search tool are added here when set.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> OpenAIResponse
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAIResponse { raw: baml.http.send(request) }
          catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed"); } }
    }
    function parse<T>(self, from: OpenAIResponse) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Decode the wire envelope into a typed shape (honoring `from_json`), pull
      // the content string, then hand it to SAP. `baml.json.from_json<T>` replaces
      // the invented `Response.json_path` (Response has no such method).
      let wire: OpenAIWire = baml.json.from_json<OpenAIWire>(baml.json.parse(from.raw.text()))
          catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai decode failed"); } };
      baml.sap.parse<T>(openai_content_of(wire))
    }
    function meta_of(self, from: OpenAIResponse) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> { from }
  }
}

// OpenAIResponse normalizes the wire response LAZILY — `project = m => m.usage()`
// never touches logprobs. `Annotated` is an EXTERNAL impl: a downstream dimension
// bolted onto the concrete response, reached by `match` inside `project`.
class OpenAIResponse {
  raw: baml.http.Response

  implements ResponseMeta {
    function usage(self) -> Usage {
      let wire: OpenAIWire = baml.json.from_json<OpenAIWire>(baml.json.parse(self.raw.text()));
      Usage { input:  wire.usage.prompt_tokens,
              output: wire.usage.completion_tokens }
    }
    function finish_reason(self) -> string {
      let wire: OpenAIWire = baml.json.from_json<OpenAIWire>(baml.json.parse(self.raw.text()));
      match (wire.choices.at(0)) { null => "stop", let c: OpenAIChoice => c.finish_reason }
    }
    // choices[0].logprobs.content[] -> TokenLogprob[]; avg=null on OpenAI. Typed
    // "empty this call" when the knob was off; never `Unsupported` (OpenAI CAN).
    function logprobs(self)  -> Supported<Logprobs> { openai_read_logprobs(self.raw) }
    function citations(self) -> Supported<DocCitation[]> { Unsupported { reason: "openai: no doc citations" } }
    function reasoning(self) -> Supported<string>        { Unsupported { reason: "gpt-4: no reasoning text" } }
  }

  implements Annotated {
    // walk message.annotations[] (url_citation / file_citation); empty if no tool fired.
    function annotations(self) -> Annotation[]
        throws baml.ExtendUnknownError<baml.errors.CallError> { openai_read_annotations(self.raw) }
  }
}

// Typed wire envelope for the OpenAI chat response — decoded with
// `baml.json.from_json<OpenAIWire>` (replaces the invented `Response.json_path`).
// Only the fields the codec reads; SAP handles the structured content separately.
class OpenAIWire {
  choices: OpenAIChoice[]
  usage: OpenAIUsage
}
class OpenAIChoice {
  message: OpenAIMessage
  finish_reason: string
}
class OpenAIMessage { content: string }
class OpenAIUsage { prompt_tokens: int, completion_tokens: int }

// The content string SAP parses, pulled off the decoded envelope.
function openai_content_of(wire: OpenAIWire) -> string {
  match (wire.choices.at(0)) { null => "", let c: OpenAIChoice => c.message.content }
}

// Host boundaries for OpenAI's enriched encodings.
function openai_read_logprobs(resp: baml.http.Response) -> Supported<Logprobs> { $rust_io_function }
function openai_read_annotations(resp: baml.http.Response) -> Annotation[] { $rust_io_function }


// ----------------------------------------------------------------------------
// Gemini — HttpProvider; its GeminiResponse implements ResponseMeta (logprobs,
// WITH avg) and the external `Grounded` dimension. NOT Cited, NOT Annotated.
// Note: Gemini's logprobs are genuinely different from OpenAI's — it fills `avg`
// (avgLogprobs) and reads off candidates[0].logprobsResult. Same ResponseMeta
// dimension, divergent normalizer. THAT is the point: one interface, lazy
// provider-private wire code co-located with the response.
// ----------------------------------------------------------------------------
class Gemini {
  model: string
  api_key: string?
  want_logprobs: int?       // response_logprobs:true + logprobs:N in generationConfig
  want_search: bool?        // attach google_search so groundingMetadata can fire

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = GeminiResponse

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // schema goes in via generationConfig.responseSchema (a THIRD encoding);
      // response_logprobs and the google_search tool are added here when set.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> GeminiResponse
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      GeminiResponse { raw: baml.http.send(request) }
          catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini send failed"); } }
    }
    function parse<T>(self, from: GeminiResponse) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let wire: GeminiWire = baml.json.from_json<GeminiWire>(baml.json.parse(from.raw.text()))
          catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "gemini decode failed"); } };
      baml.sap.parse<T>(gemini_text_of(wire))
    }
    function meta_of(self, from: GeminiResponse) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> { from }
  }
}

class GeminiResponse {
  raw: baml.http.Response

  implements ResponseMeta {
    function usage(self) -> Usage {
      let wire: GeminiWire = baml.json.from_json<GeminiWire>(baml.json.parse(self.raw.text()));
      Usage { input:  wire.usageMetadata.promptTokenCount,
              output: wire.usageMetadata.candidatesTokenCount }
    }
    function finish_reason(self) -> string {
      let wire: GeminiWire = baml.json.from_json<GeminiWire>(baml.json.parse(self.raw.text()));
      match (wire.candidates.at(0)) { null => "STOP", let c: GeminiCandidate => c.finishReason }
    }
    // candidates[0].logprobsResult -> tokens; candidates[0].avgLogprobs -> avg
    // (POPULATED here, unlike OpenAI). Gemini CAN score, so never Unsupported.
    function logprobs(self)  -> Supported<Logprobs> { gemini_read_logprobs(self.raw) }
    function citations(self) -> Supported<DocCitation[]> { Unsupported { reason: "gemini: grounding, not doc citations" } }
    function reasoning(self) -> Supported<string>        { Unsupported { reason: "gemini: no reasoning text here" } }
  }

  implements Grounded {
    // candidates[0].groundingMetadata -> {groundingChunks, groundingSupports}
    function grounding(self) -> GroundingMetadata
        throws baml.ExtendUnknownError<baml.errors.CallError> { gemini_read_grounding(self.raw) }
  }
}

// Typed wire envelope for the Gemini response — decoded with
// `baml.json.from_json<GeminiWire>` (replaces the invented `Response.json_path`).
class GeminiWire {
  candidates: GeminiCandidate[]
  usageMetadata: GeminiUsage
}
class GeminiCandidate {
  content: GeminiContent
  finishReason: string
}
class GeminiContent { parts: GeminiPart[] }
class GeminiPart { text: string }
class GeminiUsage { promptTokenCount: int, candidatesTokenCount: int }

// The candidate text SAP parses, pulled off the decoded envelope.
function gemini_text_of(wire: GeminiWire) -> string {
  match (wire.candidates.at(0)) {
    null => "",
    let c: GeminiCandidate => match (c.content.parts.at(0)) { null => "", let p: GeminiPart => p.text },
  }
}

function gemini_read_logprobs(resp: baml.http.Response) -> Supported<Logprobs> { $rust_io_function }
function gemini_read_grounding(resp: baml.http.Response) -> GroundingMetadata { $rust_io_function }


// ----------------------------------------------------------------------------
// Anthropic — HttpProvider; its AnthropicResponse implements ResponseMeta and
// the external `Cited` dimension. NOT Annotated, NOT Grounded.
// THE sharp divergence: Anthropic exposes NO logprobs and NO logit_bias. Its
// `logprobs()` normalizer returns the TYPED `Unsupported` — "this provider
// CAN'T", not "empty this call". A caller projecting `m.logprobs()` gets an
// `Unsupported` value back (or a runtime miss in a Cited/Grounded `match`); there
// is no `implements Scored` block to omit because there is no Scored companion.
// ----------------------------------------------------------------------------
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int            // bespoke required field — Messages API needs it
  want_citations: bool?      // set citations.enabled per document content block

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = AnthropicResponse

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // schema goes in via a FORCED synthetic tool whose input_schema IS T (4th
      // encoding); citations.enabled is set per document block when requested.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> AnthropicResponse
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      AnthropicResponse { raw: baml.http.send(request) }
          catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed"); } }
    }
    function parse<T>(self, from: AnthropicResponse) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // the structured answer is the forced tool's input — a JSON object on
      // content[0].input. Navigate the parsed `json` to it, re-serialize, and
      // hand the string to SAP. (Replaces the invented `Response.json_path`.)
      let root = baml.json.parse(from.raw.text())
          catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic decode failed"); } };
      baml.sap.parse<T>(baml.json.stringify(anthropic_tool_input(root)))
    }
    function meta_of(self, from: AnthropicResponse) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> { from }
  }
}

class AnthropicResponse {
  raw: baml.http.Response

  implements ResponseMeta {
    function usage(self) -> Usage {
      let wire: AnthropicWire = baml.json.from_json<AnthropicWire>(baml.json.parse(self.raw.text()));
      Usage { input:  wire.usage.input_tokens,
              output: wire.usage.output_tokens }
    }
    function finish_reason(self) -> string {
      let wire: AnthropicWire = baml.json.from_json<AnthropicWire>(baml.json.parse(self.raw.text()));
      wire.stop_reason
    }
    // THE divergence, made typed: Anthropic has no logprobs path, ever. This is
    // "can't", not "empty" — so the normalizer returns Unsupported, and a caller
    // projecting m.logprobs() reads it back as a value rather than a thrown error.
    function logprobs(self)  -> Supported<Logprobs> { Unsupported { reason: "anthropic: no logprobs, no logit_bias" } }
    // base citations() returns the FLAT doc-citation list (content[].citations[]);
    // the block structure is the richer external `Cited` dimension below.
    function citations(self) -> Supported<DocCitation[]> { anthropic_read_doc_citations(self.raw) }
    function reasoning(self) -> Supported<string>        { extract_thinking(self.raw) }
  }

  implements Cited {
    // walk content[].text + content[].citations[] into per-block structure.
    function cited_blocks(self) -> CitedBlock[]
        throws baml.ExtendUnknownError<baml.errors.CallError> { anthropic_read_citations(self.raw) }
  }
}

// Typed wire envelope for the Anthropic Messages response — only the sidecar
// fields the codec reads (usage, stop_reason). The structured answer lives in
// content[0].input and is navigated as raw `json` (see anthropic_tool_input),
// because its shape is the caller's `T`, not a fixed wire class.
class AnthropicWire {
  usage: AnthropicUsage
  stop_reason: string
}
class AnthropicUsage { input_tokens: int, output_tokens: int }

// Navigate the parsed `json` to content[0].input (the forced tool's input
// object), returning it as a `json` value for re-serialization into SAP.
// Replaces the invented `Response.json_path` with `baml.json.field` + union match.
function anthropic_tool_input(root: baml.json.json) -> baml.json.json {
  match (baml.json.field(root, "content")) {
    let content: baml.json.json[] => match (content.at(0)) {
      null => {},
      let block: baml.json.json => baml.json.field(block, "input"),
    },
    _ => {},
  }
}

function anthropic_read_doc_citations(resp: baml.http.Response) -> Supported<DocCitation[]> { $rust_io_function }
function anthropic_read_citations(resp: baml.http.Response) -> CitedBlock[] { $rust_io_function }
function extract_thinking(resp: baml.http.Response) -> Supported<string> { $rust_io_function }


// ----------------------------------------------------------------------------
// Combinator interaction: `call`/`call_with` are an HttpProvider capability now
// (../provider-as-marker.md), so Fallback forwards them by implementing
// HttpProvider and routing to a member that is itself an HttpProvider — usage,
// logprobs, citations, grounding all survive the route. This is the P1 win the old
// draft could NOT get: the previous `implements Scored` arm had to re-run a
// for-in/match/continue loop and threw when no member scored; now there is ONE
// forward, and the per-field "can't" is already typed inside the projected
// ResponseMeta (`m.logprobs()` is `Unsupported` for the Anthropic winner), so the
// combinator never needs a Scored-specific arm at all. The combinator factories
// (with_retry/fallback_to/cached) are inherited via `requires Provider` (the
// marker) — Fallback does NOT redefine them.
//
// HONEST RESIDUAL: `project` runs over ONE ResponseMeta — the WINNER's. If
// Anthropic wins, `m.logprobs()` is Unsupported even though a later OpenAI member
// COULD have scored; Fallback does not re-route to satisfy a projection, and the
// tokens burned on earlier failed members have nowhere to go (aggregate-provenance
// gap, same as the error side — see evaluation.md).
// ----------------------------------------------------------------------------
// A classified domain failure the combinator raises on purpose: every member's
// call path failed. Concrete CallError so the app can branch on it; not a network
// or rate-limit or parse failure in itself, so all classifiers return false.
class AllProvidersFailed {
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class Fallback {
  strategy: Provider[]

  // call/call_with are an HttpProvider capability now — Fallback forwards them by
  // implementing HttpProvider and routing to the first HttpProvider-capable member.
  implements HttpProvider {
    type Body = unknown

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      for (let p in self.strategy) {
        match (p) {
          let h: HttpProvider => {
            let r: T = h.call<T>(prompt) catch (e) { _ => { continue; } };
            return r;
          },
          _ => {},
        }
      }
      throw AllProvidersFailed {}
    }

    // call_with is forwarded structurally — the sidecar projection is NOT dropped.
    // The winner's ResponseMeta is what `project` sees (residual above). The
    // projection is handed to the member's own call_with, so usage/logprobs/
    // citations are not dropped under the combinator (the P1 fix).
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      for (let p in self.strategy) {
        match (p) {
          let h: HttpProvider => {
            let r: (T, U) = h.call_with<T, U>(prompt, project) catch (e) { _ => { continue; } };
            return r;   // usage/logprobs/citations forwarded, NOT dropped — the P1 win
          },
          _ => {},
        }
      }
      throw AllProvidersFailed {}
    }
  }
}
