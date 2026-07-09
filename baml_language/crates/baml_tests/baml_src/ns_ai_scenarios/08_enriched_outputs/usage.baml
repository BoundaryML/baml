// scenario 08 — enriched outputs: ../../../../../llm-provider/ideas/scenarios/08-enriched-outputs/usage.baml
//   usage.baml — APPLICATION-author code. End-user surface.
//
// What the app author writes to (a) score a classification with logprobs and
// (b) get a sourced answer. The interesting part is what they CAN'T write
// portably: there is no `Foo$cited_or_grounded` companion, because there is no
// shared citation type. The app picks a dimension and lives with the provider
// divergence — or normalizes by hand (s08_SourcedAnswer below).
//
// CARRIER: the answer AND its enrichment come back together from ONE call via
// the `$with` companion (the design's `.call_with`), returning
// `CallResult<T, V>` (the design's `(T, U)` tuple). `project` reads the
// normalized `baml.ai.ResponseMeta`: `m.logprobs()` for scores (the typed
// `Unavailable` when the provider can't) and a runtime `match (m)` for the
// divergent `s08_Cited` / `s08_Grounded` / `s08_Annotated` dimensions. No
// `Scored$Result<T>` wrapper, no `.logprobs` companion method.
//
// Live tier = testset "integ-test-08-enriched-outputs".

class s08_Sentiment {
    label: string,        // "positive" | "negative" | "neutral"
}

class s08_Answer {
    text: string,
}


// ---------------------------------------------------------------------------
// Clients. Enriched output is NOT a separate companion — it is a `project`
// over the response sidecar. The design's request-side knobs (want_logprobs,
// want_search, want_citations) are provider-private; on the real providers
// they ride `extra_body` (see the live tier). Offline tests override
// `client =` with the deterministic fakes, so the declared client is dead.
// ---------------------------------------------------------------------------
client<llm> DeadS08 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// ---------------------------------------------------------------------------
// (1) LOGPROBS — choice-scoring. The background's exact use case: compare the
// confidence of the answer instead of trusting the sampled word.
//
// `s08_Classify$with(review, (m) -> ... { m.logprobs() })` returns
// `CallResult<s08_Sentiment, baml.ai.Logprob[] | baml.ai.Unavailable>` in one
// call. When the provider can't score (Anthropic), the projected sidecar is
// the typed `Unavailable` — the app branches on it instead of catching an
// Unsupported THROW.
// ---------------------------------------------------------------------------
function s08_Classify(review: string) -> s08_Sentiment {
  client DeadS08
  prompt #"Is this review positive, negative, or neutral? {{ review }}"#
}

/// A deliberate domain failure the app raises when the scored confidence is
/// too low. A typed error on the CallError channel so callers can branch on it
/// (it is not a network, rate-limit, or parse failure — every classifier is
/// false).
class s08_LowConfidence {
    confidence: float,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }

    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
}

/// Per-token confidence for the sampled label. Tokenizer-bound (the design's
/// "What's hard"): a one-word label is usually one scored token, but a
/// multi-token label must be summed from its subword entries — so we sum every
/// token that is a piece of the label. No matching token = a floor score.
function s08_score_of_label(label: string, tokens: baml.ai.Logprob[]) -> float throws never {
  let sum = 0.0;
  let matched = false;
  for (let t in tokens) {
    if (t.token != "" && label.includes(t.token)) {
      sum = sum + t.logprob;
      matched = true;
    }
  }
  if (matched) { sum } else { -99.0 }
}

/// App code: score the classification, threshold on confidence. The design
/// destructures `(value, scores)`; here the CallResult carries both, and
/// `scores` is `Logprob[] | Unavailable`, so "this provider cannot score" is a
/// typed branch, not a thrown error.
function s08_ConfidentClassify(review: string, client: baml.ai.Provider) -> s08_Sentiment
    throws baml.errors.CallError | baml.errors.UnknownError {
  let r = s08_Classify$with(
    review,
    (m: baml.ai.ResponseMeta) -> baml.ai.Logprob[] | baml.ai.Unavailable { m.logprobs() },
    client = client,
  ) catch (e) {
    // Rule-3 normalization: keep the typed channel, box the render-path
    // foreigners (InvalidArgument / RenderPrompt / LlmClient).
    let c: baml.errors.CallError => throw c,
    let u: baml.errors.UnknownError => throw u,
    _ => throw baml.errors.UnknownError { data: e, message: ["s08 classify failed"] },
  };
  match (r.meta) {
    let u: baml.ai.Unavailable => {
      // The client cannot score (e.g. swapped to an Anthropic-shaped one).
      // Honest typed branch: accept the unscored label rather than fail. (An
      // app that REQUIRES scoring would `throw` here instead — its policy,
      // on the value channel.)
      r.value
    },
    let lp: baml.ai.Logprob[] => {
      let confidence = s08_score_of_label(r.value.label, lp);
      if (confidence < -2.3) {     // ~10% probability floor
        throw s08_LowConfidence { confidence: confidence };
      }
      r.value
    },
  }
}


// ---------------------------------------------------------------------------
// (2) SOURCED ANSWERS — and here the abstraction REFUSES to unify. Anthropic's
// citations are s08_CitedBlock[] (spans into your docs); Gemini's grounding is
// s08_GroundingMetadata (answer-segment -> retrieved chunk). Different
// dimensions returning different types, reached by matching the projected
// ResponseMeta against the external interface. The app chooses which dimension
// to project. (The design named these `Research$cited` / `$grounded`; `$` is
// the generated-companion namespace on the real surface, so the app wrappers
// are plain functions.)
// ---------------------------------------------------------------------------
function s08_Research(question: string) -> s08_Answer {
  client DeadS08
  prompt #"Answer using the attached documents: {{ question }}"#
}

/// Anthropic path: per-document citation blocks, projected off the sidecar.
/// The `match (m)` reaches the external `s08_Cited` dimension; the `_` arm
/// makes "this provider doesn't cite" a typed empty rather than a throw.
function s08_ResearchCited(question: string, client: baml.ai.Provider) -> baml.ai.CallResult<s08_Answer, s08_CitedBlock[]> throws never {
  s08_Research$with(
    question,
    (m: baml.ai.ResponseMeta) -> s08_CitedBlock[] {
      match (m) {
        let c: s08_Cited => c.cited_blocks(),
        _ => [],
      }
    },
    client = client,
  ) catch (e) {
    _ => baml.ai.CallResult<s08_Answer, s08_CitedBlock[]> { value: s08_Answer { text: "failed" }, meta: [] },
  }
}

/// Gemini path: grounding metadata. A DIFFERENT dimension, a DIFFERENT type —
/// reached by matching the SAME projected ResponseMeta against `s08_Grounded`.
function s08_ResearchGrounding(question: string, client: baml.ai.Provider) -> baml.ai.CallResult<s08_Answer, s08_GroundingMetadata> throws never {
  s08_Research$with(
    question,
    (m: baml.ai.ResponseMeta) -> s08_GroundingMetadata {
      match (m) {
        let g: s08_Grounded => g.grounding(),
        _ => s08_GroundingMetadata { chunks: [], supports: [] },
      }
    },
    client = client,
  ) catch (e) {
    _ => baml.ai.CallResult<s08_Answer, s08_GroundingMetadata> {
      value: s08_Answer { text: "failed" },
      meta: s08_GroundingMetadata { chunks: [], supports: [] },
    },
  }
}


// ---------------------------------------------------------------------------
// If an app wants "a sourced answer regardless of provider," IT writes the
// normalization — the std lib does not. This is the honest cost: the app owns
// the lowest-common-denominator "claim -> source" shape, because the three
// providers do not share one. ONE `project` match tries EACH dimension in turn
// over ONE response sidecar.
// ---------------------------------------------------------------------------
class s08_SourcedClaim {
    text: string,
    source_uris: string[],   // app's chosen common shape; lossy on purpose
}

function s08_normalize_citations(blocks: s08_CitedBlock[]) -> s08_SourcedClaim[] throws never {
  let claims: s08_SourcedClaim[] = [];
  for (let b in blocks) {
    let uris: string[] = [];
    for (let c in b.citations) {
      uris.push("doc:" + c.document_index.to_string());
    }
    claims.push(s08_SourcedClaim { text: b.text, source_uris: uris });
  }
  claims
}

function s08_normalize_grounding(gm: s08_GroundingMetadata) -> s08_SourcedClaim[] throws never {
  let claims: s08_SourcedClaim[] = [];
  for (let s in gm.supports) {
    let uris: string[] = [];
    for (let i in s.chunk_indices) {
      match (gm.chunks.at(i)) {
        null => {},
        let ch: s08_GroundingChunk => { uris.push(ch.uri); },
      }
    }
    claims.push(s08_SourcedClaim { text: s.segment_text, source_uris: uris });
  }
  claims
}

function s08_normalize_annotations(anns: s08_Annotation[]) -> s08_SourcedClaim[] throws never {
  let claims: s08_SourcedClaim[] = [];
  for (let a in anns) {
    claims.push(s08_SourcedClaim {
      text: "[" + a.start.to_string() + ".." + a.end.to_string() + "]",
      source_uris: [a.url ?? ("file:" + (a.file_id ?? "unknown"))],
    });
  }
  claims
}

/// The app-owned unifier: one projection, three dimension probes, one lossy
/// common shape out. A provider with NO source dimension yields a typed empty,
/// not a throw; a provider with no HttpProvider at all surfaces as the
/// stdlib's Unsupported on the CallError channel (rethrown typed below).
function s08_SourcedAnswer(question: string, p: baml.ai.Provider) -> s08_SourcedClaim[]
    throws baml.errors.CallError | baml.errors.UnknownError {
  let r = s08_Research$with(
    question,
    (m: baml.ai.ResponseMeta) -> s08_SourcedClaim[] {
      match (m) {
        let c: s08_Cited => s08_normalize_citations(c.cited_blocks()),
        let g: s08_Grounded => s08_normalize_grounding(g.grounding()),
        let a: s08_Annotated => s08_normalize_annotations(a.annotations()),
        _ => [],   // no source dimension on this provider — typed empty, not a throw
      }
    },
    client = p,
  ) catch (e) {
    let c: baml.errors.CallError => throw c,
    let u: baml.errors.UnknownError => throw u,
    _ => throw baml.errors.UnknownError { data: e, message: ["s08 sourced answer failed"] },
  };
  r.meta
}

/// Test helper: flatten every claim's source URIs for one-line assertions.
function s08_uris_of(claims: s08_SourcedClaim[]) -> string throws never {
  let all: string[] = [];
  for (let c in claims) {
    for (let u in c.source_uris) {
      all.push(u);
    }
  }
  all.join(",")
}


// ---------------------------------------------------------------------------
// Offline tests — deterministic, no network.
// ---------------------------------------------------------------------------

test "08: logprobs ride the $with sidecar — answer AND scores in ONE call" {
    let r = s08_Classify$with(
        "Great product, would buy again!",
        (m: baml.ai.ResponseMeta) -> baml.ai.Logprob[] | baml.ai.Unavailable { m.logprobs() },
        client = s08_ScoredEcho {
            reply: "{\"label\": \"positive\"}",
            tokens: [baml.ai.Logprob { token: "positive", logprob: -0.05 }],
            anns: [],
        },
    ) catch (e) {
        _ => baml.ai.CallResult<s08_Sentiment, baml.ai.Logprob[] | baml.ai.Unavailable> {
            value: s08_Sentiment { label: "failed" },
            meta: baml.ai.Unavailable { reason: "call failed" },
        },
    };
    assert.equal(r.value.label, "positive")
    let scored = match (r.meta) {
        let u: baml.ai.Unavailable => "unavailable: " + u.reason,
        let lp: baml.ai.Logprob[] => "scored " + lp.length().to_string(),
    };
    assert.equal(scored, "scored 1")
}

test "08: a confident classification passes the threshold" {
    let v = s08_ConfidentClassify("Loved it!", s08_ScoredEcho {
        reply: "{\"label\": \"positive\"}",
        tokens: [baml.ai.Logprob { token: "positive", logprob: -0.05 }],
        anns: [],
    }) catch (e) {
        _ => s08_Sentiment { label: "error" },
    };
    assert.equal(v.label, "positive")
}

test "08: low confidence is a typed app error on the CallError channel" {
    // The label token scored ~0.7% probability — below the ~10% floor, so the
    // app throws its own concrete CallError and the caller branches on it
    // most-specific first.
    let v = s08_ConfidentClassify("Meh.", s08_ScoredEcho {
        reply: "{\"label\": \"neutral\"}",
        tokens: [baml.ai.Logprob { token: "neutral", logprob: -5.0 }],
        anns: [],
    }) catch (e) {
        let lc: s08_LowConfidence => s08_Sentiment { label: "low:" + lc.confidence.to_string() },
        _ => s08_Sentiment { label: "wrong error" },
    };
    assert.is_true(v.label.starts_with("low:"))
}

// The portability proof — and the leak. Swapping the scored client for the
// Anthropic-shaped one does NOT fail to compile: the client is the existential
// Provider, and `m.logprobs()` still type-checks. The difference is a VALUE —
// `Unavailable` — at runtime, not a throw: per-field absence is a runtime
// promise, carried typed.
test "08: a provider that can't score yields Unavailable — a value, not a throw" {
    let v = s08_ConfidentClassify("Fine I guess.", s08_CitingEcho {
        reply: "{\"label\": \"neutral\"}",
        blocks: [],
    }) catch (e) {
        _ => s08_Sentiment { label: "error" },
    };
    // accepted UNSCORED — the Unavailable branch, not LowConfidence, not a throw
    assert.equal(v.label, "neutral")
}

test "08: Anthropic-style doc citations ride the external Cited dimension" {
    let r = s08_ResearchCited("Is the sky blue?", s08_CitingEcho {
        reply: "{\"text\": \"Per doc 0, yes.\"}",
        blocks: [s08_CitedBlock {
            text: "Per doc 0, yes.",
            citations: [s08_DocCitation { document_index: 0, start: 12, end: 15, cited_text: "the sky is blue" }],
        }],
    });
    assert.equal(r.value.text, "Per doc 0, yes.")
    assert.equal(r.meta.length(), 1)
    let first_cite = match (r.meta.at(0)) {
        null => "none",
        let b: s08_CitedBlock => match (b.citations.at(0)) {
            null => "none",
            let c: s08_DocCitation => c.cited_text,
        },
    };
    assert.equal(first_cite, "the sky is blue")
}

test "08: Gemini-style grounding is a different dimension with a different type" {
    let r = s08_ResearchGrounding("What's new?", s08_GroundedEcho {
        reply: "{\"text\": \"Recent things.\"}",
        tokens: [],
        gm: s08_GroundingMetadata {
            chunks: [s08_GroundingChunk { uri: "https://news.example/a", title: "A" }],
            supports: [s08_GroundingSupport { segment_text: "Recent things.", chunk_indices: [0] }],
        },
    });
    assert.equal(r.meta.chunks.length(), 1)
    assert.equal(r.meta.supports.length(), 1)
}

test "08: the flat citations dimension is the lossy base; blocks are the rich one" {
    // Base `m.citations()` answers the stdlib's flat { url, title } list —
    // enough for "was anything cited"; the block spans need s08_Cited.
    let r = s08_Research$with(
        "Is it?",
        (m: baml.ai.ResponseMeta) -> string {
            match (m.citations()) {
                let u: baml.ai.Unavailable => "unavailable",
                let cs: baml.ai.Citation[] => match (cs.at(0)) {
                    null => "empty",
                    let c: baml.ai.Citation => c.url,
                },
            }
        },
        client = s08_CitingEcho {
            reply: "{\"text\": \"yes\"}",
            blocks: [s08_CitedBlock {
                text: "yes",
                citations: [s08_DocCitation { document_index: 2, start: 0, end: 3, cited_text: "yes" }],
            }],
        },
    ) catch (e) {
        _ => baml.ai.CallResult<s08_Answer, string> { value: s08_Answer { text: "failed" }, meta: "" },
    };
    assert.equal(r.meta, "doc:2")
}

test "08: one app-owned normalization spans all three source dimensions" {
    let citing = s08_CitingEcho {
        reply: "{\"text\": \"cited answer\"}",
        blocks: [s08_CitedBlock {
            text: "cited answer",
            citations: [s08_DocCitation { document_index: 0, start: 0, end: 5, cited_text: "quoted" }],
        }],
    };
    let grounded = s08_GroundedEcho {
        reply: "{\"text\": \"grounded answer\"}",
        tokens: [],
        gm: s08_GroundingMetadata {
            chunks: [s08_GroundingChunk { uri: "https://source.example/x", title: "X" }],
            supports: [s08_GroundingSupport { segment_text: "grounded answer", chunk_indices: [0] }],
        },
    };
    let annotated = s08_ScoredEcho {
        reply: "{\"text\": \"annotated answer\"}",
        tokens: [],
        anns: [s08_Annotation { kind: "url_citation", url: "https://web.example/hit", file_id: null, start: 0, end: 9 }],
    };
    let from_citing = s08_SourcedAnswer("q", citing) catch (e) { _ => [] };
    let from_grounded = s08_SourcedAnswer("q", grounded) catch (e) { _ => [] };
    let from_annotated = s08_SourcedAnswer("q", annotated) catch (e) { _ => [] };
    assert.equal(s08_uris_of(from_citing), "doc:0")
    assert.equal(s08_uris_of(from_grounded), "https://source.example/x")
    assert.equal(s08_uris_of(from_annotated), "https://web.example/hit")
}

// Because the stdlib Fallback implements HttpProvider and forwards
// call_messages_with, the enrichment projection SURVIVES the route — the
// design's P1 win, on the real combinator (no Scored-specific arm anywhere).
test "08: the sidecar survives a Fallback route — the P1 win" {
    let resilient: baml.ai.Provider = AlwaysFailProvider {}.fallback_to(s08_ScoredEcho {
        reply: "{\"label\": \"positive\"}",
        tokens: [baml.ai.Logprob { token: "positive", logprob: -0.1 }],
        anns: [],
    });
    let r = s08_Classify$with(
        "Great!",
        (m: baml.ai.ResponseMeta) -> string {
            match (m.logprobs()) {
                let u: baml.ai.Unavailable => "unavailable: " + u.reason,
                let lp: baml.ai.Logprob[] => "scored " + lp.length().to_string(),
            }
        },
        client = resilient,
    ) catch (e) {
        _ => baml.ai.CallResult<s08_Sentiment, string> { value: s08_Sentiment { label: "failed" }, meta: "" },
    };
    assert.equal(r.value.label, "positive")
    assert.equal(r.meta, "scored 1")
}

// HONEST RESIDUAL (from the design): `project` runs over ONE ResponseMeta —
// the WINNER's. If the Anthropic-shaped member wins, `m.logprobs()` is
// Unavailable even though a later member COULD have scored; Fallback does not
// re-route to satisfy a projection.
test "08: Fallback projects the WINNER's meta — the aggregate-provenance residual" {
    let chain: baml.ai.Provider = s08_CitingEcho { reply: "{\"label\": \"neutral\"}", blocks: [] }
        .fallback_to(s08_ScoredEcho {
            reply: "{\"label\": \"neutral\"}",
            tokens: [baml.ai.Logprob { token: "neutral", logprob: -0.1 }],
            anns: [],
        });
    let r = s08_Classify$with(
        "Meh.",
        (m: baml.ai.ResponseMeta) -> string {
            match (m.logprobs()) {
                let u: baml.ai.Unavailable => "unavailable: " + u.reason,
                let lp: baml.ai.Logprob[] => "scored " + lp.length().to_string(),
            }
        },
        client = chain,
    ) catch (e) {
        _ => baml.ai.CallResult<s08_Sentiment, string> { value: s08_Sentiment { label: "failed" }, meta: "" },
    };
    // the citing member WON, so the projection sees ITS meta: no logprobs.
    assert.is_true(r.meta.includes("unavailable"))
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The design's want_logprobs knob, for real: `logprobs: true` rides
// `extra_body` on the native OpenAi provider, and the stdlib's OpenAiMeta
// normalizes choices[0].logprobs.content[] into baml.ai.Logprob[]. Anthropic
// proves the sharp edge live: no logprobs, EVER — the typed Unavailable.
testset "integ-test-08-enriched-outputs" {
    test "08 live: OpenAI logprobs ride the sidecar when requested via extra_body" {
        let r = s08_Classify$with(
            "This is wonderful!",
            (m: baml.ai.ResponseMeta) -> string {
                match (m.logprobs()) {
                    let u: baml.ai.Unavailable => "unavailable: " + u.reason,
                    let lp: baml.ai.Logprob[] => "scored " + lp.length().to_string(),
                }
            },
            // gpt-4.1-mini: the reasoning-family models reject the chat
            // completions `logprobs` param, so the scored live probe uses a
            // classic chat model.
            client = baml.ai.OpenAi {
                model: "gpt-4.1-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
                extra_body: { "logprobs": true, "top_logprobs": 3 },
            },
        ) catch (e) {
            _ => baml.ai.CallResult<s08_Sentiment, string> { value: s08_Sentiment { label: "live failed" }, meta: "" },
        };
        assert.is_true(r.meta.starts_with("scored"))
    }

    test "08 live: Anthropic logprobs are the typed Unavailable — can't, not empty" {
        let r = s08_Classify$with(
            "This is wonderful!",
            (m: baml.ai.ResponseMeta) -> string {
                match (m.logprobs()) {
                    let u: baml.ai.Unavailable => "unavailable: " + u.reason,
                    let lp: baml.ai.Logprob[] => "scored " + lp.length().to_string(),
                }
            },
            client = baml.ai.Anthropic {
                model: "claude-haiku-4-5-20251001",
                api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                base_url: null,
                max_tokens: null,
                anthropic_version: null,
                extra_headers: null,
                extra_body: null,
                append_output_schema: null,
            },
        ) catch (e) {
            _ => baml.ai.CallResult<s08_Sentiment, string> { value: s08_Sentiment { label: "live failed" }, meta: "" },
        };
        assert.is_true(r.meta.includes("unavailable"))
    }
}
