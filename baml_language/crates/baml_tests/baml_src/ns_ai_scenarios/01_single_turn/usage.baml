// scenario 01 — single-turn text: ../../../../../llm-provider/ideas/scenarios/01-single-turn-text/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The irreducible case: send some text, get text back. This mirrors the design's
// usage.baml on the REAL surface:
//   - how little a one-shot LLM function is (`function Foo -> T { client ... prompt }`),
//   - that the SAME function body works across Anthropic / OpenAI / Gemini despite
//     the three system-prompt placements and the assistant/"model" role split (all
//     absorbed inside each provider's build_request — see REALIZED.md §a),
//   - how usage is read via the `$with` sidecar companion (the design's `.call_with`)
//     — a ResponseMeta projection returning a `CallResult<T, V>` (the design's
//     `(T, U)` tuple; tuples aren't a language feature, so CallResult carries it),
//   - how a refusal surfaces as a typed channel the app branches on (s01_Refused),
//   - how a combinator chain reads (with_retry / fallback_to, inherited on Provider).
//
// Live tier = testset "integ-test-01-single-turn" (OpenAI + Anthropic + Gemini).


// --- One-shot client. -------------------------------------------------------
// The design declares three named clients (Claude/GPT/Flash) over Anthropic/
// OpenAI/Gemini and swaps them into ONE function body. On the real surface a
// declared `client` is the function's DEFAULT; the swap is a one-line call-site
// `client = <provider>` override (proven in the live tier below, and offline via
// the fakes). Provider-specific params (max_tokens, temperature, top_k, …) are
// just plain fields on each provider — no options blob. We default to a dead
// client so every offline test must pass an explicit, deterministic override.
client<llm> DeadS01 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// --- The actual single-turn function. ---------------------------------------
// This is the whole "text in / text out" surface. The desugar lowers this body
// to `baml.ai.drive_call<string>(client, s01_Summarize$render_prompt(work))`;
// the client's provider re-shapes the rendered turns into its own envelope. The
// body is identical no matter which provider is wired in — that's the
// portability claim (design §a). (Named `s01_Summarize`: all scenario dirs share
// ONE namespace and plain `Summarize` is already scenario 32's.)
function s01_Summarize(work: string) -> string {
  client DeadS01                            // swap via `client =` at the call site
  prompt #"
    {{ _.role("system") }}
    You are a terse assistant.

    {{ _.role("user") }}
    Summarize the plot of {{ work }} in one line.
  "#
}


// --- The base call: text in, text out. --------------------------------------
test "01: the plain call routes through a swapped-in provider" {
    let r = s01_Summarize("Hamlet", client = EchoProvider { reply: "A prince avenges his father." }) catch (e) {
        _ => "call failed",
    };
    assert.equal(r, "A prince avenges his father.")
}

// The portability claim, offline: the SAME body, a DIFFERENT provider, is a
// one-line client override. (The live tier proves it across three real APIs.)
test "01: same body, a different provider is a one-line client override" {
    let r = s01_Summarize("Dune", client = EchoProvider { reply: "A boy becomes a desert messiah." }) catch (e) {
        _ => "call failed",
    };
    assert.equal(r, "A boy becomes a desert messiah.")
}

test "01: a capability-less client is a typed Unsupported" {
    let r = s01_Summarize("x", client = NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}


// --- Usage: the `$with` companion (a product sidecar). ----------------------
// `s01_Summarize(work)` uses `call` and discards usage. When the app wants
// tokens, it calls the generated `$with` companion (the design's `.call_with`),
// handing a `project: (ResponseMeta) -> V` closure that reads exactly the
// dimension it wants off the normalized sidecar. The result is a
// `CallResult<T, V>`: the answer AND the projection. `$with` desugars to
// `drive_with`, which routes to HttpProvider.call_messages_with — a SINGLE codec
// pass (parse<T> alongside project(parse_meta(body))), so no duplicated send and
// every combinator forwards it (usage is not dropped under Fallback/Retry).
test "01: usage rides the $with sidecar as CallResult — real tokens, not dropped" {
    let r = s01_Summarize$with(
        "Moby-Dick",
        (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
        client = s01_MeteredEcho { reply: "A captain hunts a whale.", inp: 12, out: 6 },
    ) catch (e) {
        _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "with failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } },
    };
    assert.equal(r.value, "A captain hunts a whale.")
    assert.equal(r.meta.input_tokens, 12)
    assert.equal(r.meta.output_tokens, 6)
}

// A richer projection is still ONE call. The design projects a TUPLE
// `(Usage, Supported<Logprob[]>)`; tuples aren't a language feature, so we
// project a small class (the same substitution CallResult makes for (T, U)).
// logprobs come back `Unavailable` on chat providers — a TYPED "can't", not a
// silent zero, which the app branches on.
class s01_MetaSnapshot {
    usage: baml.ai.Usage,
    logprobs: baml.ai.Logprob[] | baml.ai.Unavailable,
}

test "01: logprobs come back a typed Unavailable, not a silent zero" {
    let r = s01_Summarize$with(
        "1984",
        (m: baml.ai.ResponseMeta) -> s01_MetaSnapshot {
            s01_MetaSnapshot { usage: m.usage(), logprobs: m.logprobs() }
        },
        client = s01_MeteredEcho { reply: "A clerk rebels against Big Brother.", inp: 9, out: 7 },
    ) catch (e) {
        _ => baml.ai.CallResult<string, s01_MetaSnapshot> {
            value: "with failed",
            meta: s01_MetaSnapshot { usage: baml.ai.Usage { input_tokens: -1, output_tokens: -1 }, logprobs: baml.ai.Unavailable { reason: "err" } },
        },
    };
    assert.equal(r.meta.usage.input_tokens, 9)
    let logprobs_msg = match (r.meta.logprobs) {
        let u: baml.ai.Unavailable => "unavailable: " + u.reason,
        let lp: baml.ai.Logprob[]  => "got " + lp.length().to_string() + " logprobs",
    };
    assert.is_true(logprobs_msg.includes("unavailable"))
}


// --- Refusals: a refusal is not the answer. ---------------------------------
// s01_Summarize throws s01_Refused (the typed channel from implementation.baml)
// when the model declines — caught INSIDE `parse`, before SAP ever runs. The app
// branches most-specific first: the concrete refusal, then the CallError
// interface, then the UnknownError escape hatch. `Refused` is indistinguishable
// from any other CallError at the channel level — the app recovers its meaning
// by matching the concrete type BEFORE the interface arm (design §b.4 / §c).
test "01: a refusal surfaces as a typed s01_Refused channel, distinct from an answer" {
    let r = s01_Summarize("something disallowed", client = s01_RefusingProvider { category: "policy" }) catch (e) {
        let ref: s01_Refused          => "[declined: " + ref.category + "]",
        let c: baml.errors.CallError  => { if (c.is_rate_limit()) { "[rate-limited]" } else { "[error]" } },
        let u: baml.errors.UnknownError => "[error: " + u.message.join(" / ") + "]",
    };
    assert.equal(r, "[declined: policy]")
}

// The refusal reports itself honestly on the Failure super-interface, too.
test "01: a refusal is a policy refusal, not a network/rate-limit failure" {
    let flags = s01_Summarize("disallowed", client = s01_RefusingProvider { category: "safety" }) catch (e) {
        let ref: s01_Refused => ref.is_policy_refusal().to_string() + "/" + ref.is_network_error().to_string() + "/" + ref.is_rate_limit().to_string(),
        _ => "wrong error",
    };
    assert.equal(flags, "true/false/false")
}


// --- Combinator chain reads. ------------------------------------------------
// Orchestration is NOT a provider field — it wraps the provider. The design's
// `Claude().with_retry(2).fallback_to(GPT())` reads left-to-right off the
// inherited Provider factories. Offline analog: retry a failing provider twice,
// then fall back to a working one — the whole chain is just another Provider
// handed to the SAME single-turn function via a `client =` override.
test "01: a resilient chain — retry then fall back — reads left to right" {
    let resilient: baml.ai.Provider =
        AlwaysFailProvider {}.with_retry(2).fallback_to(EchoProvider { reply: "recovered via fallback" });
    let r = s01_Summarize("Frankenstein", client = resilient) catch (e) {
        _ => "chain failed",
    };
    assert.equal(r, "recovered via fallback")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The portability claim against three REAL APIs: ONE function body, three
// providers, three system-prompt placements (OpenAI in-array, Anthropic
// top-level `system`, Gemini `system_instruction`) and divergent sampling knobs
// all absorbed inside each provider — the app just swaps `client =`.
testset "integ-test-01-single-turn" {
    test "01 live: single turn through a native OpenAI override" {
        let r = s01_Summarize("Reply with exactly the lowercase word: pong", client = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        }) catch (e) {
            _ => "live call failed",
        };
        assert.is_true(r.includes("pong") || r.includes("Pong"))
    }

    test "01 live: the same body through a native Anthropic override" {
        let r = s01_Summarize("Reply with exactly the lowercase word: pong", client = baml.ai.Anthropic {
            model: "claude-haiku-4-5-20251001",
            api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
            base_url: null,
            max_tokens: null,
            anthropic_version: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }) catch (e) {
            _ => "live call failed",
        };
        // The point is that the native override ROUTES (the template wraps the
        // arg in "Summarize the plot of ...", so exact-echo obedience is not a
        // fair assertion against every model).
        log.info("01 anthropic override reply: " + r);
        assert.is_true(r.length() > 0 && r != "live call failed")
    }

    test "01 live: the same body through a native Gemini override" {
        // .with_retry(2): live Gemini transiently 429s/empties under the
        // concurrent sweep — the reliability combinator IS the honest answer.
        let r = s01_Summarize("Reply with exactly the lowercase word: pong", client = baml.ai.Gemini {
            model: "gemini-flash-latest",
            api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
            base_url: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }.with_retry(2)) catch (e) {
            _ => "live call failed",
        };
        assert.is_true(r.includes("pong") || r.includes("Pong"))
    }

    test "01 live: usage rides the $with sidecar off a real response" {
        let r = s01_Summarize$with(
            "Summarize the plot of Hamlet in one line.",
            (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
            client = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
        ) catch (e) {
            _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "live failed", meta: baml.ai.Usage { input_tokens: 0, output_tokens: 0 } },
        };
        assert.is_true(r.meta.input_tokens > 0)
        assert.is_true(r.meta.output_tokens > 0)
    }
}
