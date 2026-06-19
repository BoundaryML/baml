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
// are three different objects. We model each as its own capability interface
// that `requires Provider`, with no cross-provider normalization promised by
// the std lib (an app can normalize itself — see usage.baml).
//
// Everything here builds on the canonical primitives in _conventions.md.
// Net-new surface is flagged in evaluation.md.
// ============================================================================


// ----------------------------------------------------------------------------
// Net-new value types. These are the wire shapes, lifted into BAML.
// Deliberately NOT unified — each mirrors one provider's response exactly.
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
// Capability interfaces. Each `requires Provider`. None of these has a default
// `call`-based fallback, because there is no honest one: you cannot synthesize a
// token distribution, a span into a document, or a retrieved-chunk map out of a
// plain completion. A provider EITHER implements the capability or it does not.
// ----------------------------------------------------------------------------

// Logprobs: produce the answer AND its token scores in one call.
// `logprobs<T>` returns the value plus the scores; `with_logprobs` is a request
// knob that lives at the codec (see how providers thread `top_logprobs`).
interface Scored requires Provider {
  function logprobs<T>(self, prompt: baml.llm.PromptAst, top: int) -> Scored$Result<T> throws Error
}

// The paired result. Generic over the answer type T so `logprobs<Sentiment>` is
// honest about what it returns.
class Scored$Result<T> {
  value: T
  scores: Logprobs
}

// Anthropic-style citations: answer text broken into blocks, each carrying the
// document spans that back it. The caller flags which docs are citable when it
// builds the prompt; that detail is provider-private (inside build_request).
interface Citable requires Provider {
  function cited<T>(self, prompt: baml.llm.PromptAst) -> Citable$Result<T> throws Error
}

class Citable$Result<T> {
  value: T
  blocks: CitedBlock[]
}

// Gemini-style grounding: the answer plus a grounding-metadata sidecar.
interface Grounded requires Provider {
  function grounded<T>(self, prompt: baml.llm.PromptAst) -> Grounded$Result<T> throws Error
}

class Grounded$Result<T> {
  value: T
  grounding: GroundingMetadata
}

// OpenAI-style annotations: the answer plus annotation spans from tool output.
interface Annotated requires Provider {
  function annotated<T>(self, prompt: baml.llm.PromptAst) -> Annotated$Result<T> throws Error
}

class Annotated$Result<T> {
  value: T
  annotations: Annotation[]
}


// ----------------------------------------------------------------------------
// OpenAI — HttpProvider + Scored + Annotated.  NOT Citable, NOT Grounded.
// ----------------------------------------------------------------------------
class OpenAI {
  model: string
  api_key: string?
  max_tokens: int?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // plain completion: schema goes in via response_format (json_schema, strict)
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Scored {
    function logprobs<T>(self, prompt: baml.llm.PromptAst, top: int) -> Scored$Result<T> throws baml.errors.LlmClient {
      // Add logprobs:true + top_logprobs:top to the SAME body the codec builds.
      let req: baml.http.Request = self.build_request<T>(prompt);
      let scored_req: baml.http.Request = openai_add_logprobs(req, top);
      let resp: baml.http.Response = self.send(scored_req);
      // The value is parsed off message.content; the scores off choices[0].logprobs.content[].
      let value: T = baml.sap.parse<T>(resp.json_path("choices.0.message.content"));
      let scores: Logprobs = openai_read_logprobs(resp);   // choices[0].logprobs.content[] -> TokenLogprob[], avg=null
      Scored$Result<T> { value: value, scores: scores }
    }
  }

  implements Annotated {
    function annotated<T>(self, prompt: baml.llm.PromptAst) -> Annotated$Result<T> throws baml.errors.LlmClient {
      // Annotations only appear if a web/file-search tool actually fired. We attach
      // the search tool here; if nothing fired, `annotations` comes back empty.
      let req: baml.http.Request = openai_with_websearch(self.build_request<T>(prompt));
      let resp: baml.http.Response = self.send(req);
      let value: T = baml.sap.parse<T>(resp.json_path("choices.0.message.content"));
      let anns: Annotation[] = openai_read_annotations(resp);  // walk message.annotations[]
      Annotated$Result<T> { value: value, annotations: anns }
    }
  }
}

// Host boundaries for OpenAI's enriched encodings.
function openai_add_logprobs(req: baml.http.Request, top: int) -> baml.http.Request { $rust_io_function }
function openai_read_logprobs(resp: baml.http.Response) -> Logprobs { $rust_io_function }
function openai_with_websearch(req: baml.http.Request) -> baml.http.Request { $rust_io_function }
function openai_read_annotations(resp: baml.http.Response) -> Annotation[] { $rust_io_function }


// ----------------------------------------------------------------------------
// Gemini — HttpProvider + Scored + Grounded.  NOT Citable, NOT Annotated.
// Note: Gemini's `Scored` is genuinely different from OpenAI's — it fills `avg`
// (avgLogprobs) and reads off candidates[0].logprobsResult. Same interface,
// divergent body. THAT is the point: one interface, provider-private wire code.
// ----------------------------------------------------------------------------
class Gemini {
  model: string
  api_key: string?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // schema goes in via generationConfig.responseSchema (a THIRD encoding)
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("candidates.0.content.parts.0.text"))
    }
  }

  implements Scored {
    function logprobs<T>(self, prompt: baml.llm.PromptAst, top: int) -> Scored$Result<T> throws baml.errors.LlmClient {
      // Gemini: response_logprobs:true + logprobs:top in generationConfig.
      let req: baml.http.Request = gemini_add_logprobs(self.build_request<T>(prompt), top);
      let resp: baml.http.Response = self.send(req);
      let value: T = self.parse<T>(resp);
      // candidates[0].logprobsResult -> tokens; candidates[0].avgLogprobs -> avg.
      let scores: Logprobs = gemini_read_logprobs(resp);   // avg is POPULATED here, unlike OpenAI
      Scored$Result<T> { value: value, scores: scores }
    }
  }

  implements Grounded {
    function grounded<T>(self, prompt: baml.llm.PromptAst) -> Grounded$Result<T> throws baml.errors.LlmClient {
      // Attach the google_search tool; grounding only materializes if it fires.
      let req: baml.http.Request = gemini_with_search(self.build_request<T>(prompt));
      let resp: baml.http.Response = self.send(req);
      let value: T = self.parse<T>(resp);
      // candidates[0].groundingMetadata -> {groundingChunks, groundingSupports}
      let gm: GroundingMetadata = gemini_read_grounding(resp);
      Grounded$Result<T> { value: value, grounding: gm }
    }
  }
}

function gemini_add_logprobs(req: baml.http.Request, top: int) -> baml.http.Request { $rust_io_function }
function gemini_read_logprobs(resp: baml.http.Response) -> Logprobs { $rust_io_function }
function gemini_with_search(req: baml.http.Request) -> baml.http.Request { $rust_io_function }
function gemini_read_grounding(resp: baml.http.Response) -> GroundingMetadata { $rust_io_function }


// ----------------------------------------------------------------------------
// Anthropic — HttpProvider + Citable.  NOT Scored, NOT Grounded, NOT Annotated.
// THE sharp divergence: Anthropic exposes NO logprobs and NO logit_bias. There
// is no `implements Scored` block here, and there CANNOT honestly be one.
// ----------------------------------------------------------------------------
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int            // bespoke required field — Messages API needs it

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // schema goes in via a FORCED synthetic tool whose input_schema IS T (4th encoding)
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // the structured answer is the forced tool's input
      baml.sap.parse<T>(from.json_path("content.0.input"))
    }
  }

  implements Citable {
    function cited<T>(self, prompt: baml.llm.PromptAst) -> Citable$Result<T> throws baml.errors.LlmClient {
      // Set citations.enabled per document content block. The answer's text blocks
      // then carry a citations[] array (document_index + span + cited_text).
      let req: baml.http.Request = anthropic_enable_citations(self.build_request<T>(prompt));
      let resp: baml.http.Response = self.send(req);
      // value: from the forced tool input, as usual.
      let value: T = self.parse<T>(resp);
      // blocks: walk content[].text + content[].citations[]
      let blocks: CitedBlock[] = anthropic_read_citations(resp);
      Citable$Result<T> { value: value, blocks: blocks }
    }
  }

  // DELIBERATELY ABSENT: implements Scored { ... }
  // Anthropic has no logprobs path. Omitting the block is the honest model:
  // `match (p) { let s: Scored => ... }` will fall through for Anthropic.
}

function anthropic_enable_citations(req: baml.http.Request) -> baml.http.Request { $rust_io_function }
function anthropic_read_citations(resp: baml.http.Response) -> CitedBlock[] { $rust_io_function }


// ----------------------------------------------------------------------------
// Combinator interaction: Fallback must forward `Scored` like any capability.
// But forwarding `Scored` is a LIE if no member is Scored, and unlike Streaming
// (which degrades to .call) there is no honest degrade. So Fallback's Scored
// arm errors when it finds no Scored member — same shape as its Realtime arm in
// the proposal. We re-state it here to show the std-lib author must write it.
// ----------------------------------------------------------------------------
class Fallback {
  strategy: Provider[]

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        let r: T = p.call<T>(prompt) catch (e) { _ => { continue; } };
        return r;
      }
      throw baml.errors.LlmClient { message: "all providers failed" }
    }
  }

  implements Scored {
    function logprobs<T>(self, prompt: baml.llm.PromptAst, top: int) -> Scored$Result<T> throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        match (p) {
          let s: Scored => { return s.logprobs<T>(prompt, top); },
          _            => { continue; },     // skip members with no logprobs (e.g. Anthropic)
        }
      }
      // No honest fallback: a plain completion has no token distribution.
      throw baml.errors.Unsupported { message: "no member provider exposes logprobs" }
    }
  }
}
