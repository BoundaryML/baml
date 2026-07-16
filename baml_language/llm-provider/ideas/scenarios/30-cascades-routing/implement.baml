// ─────────────────────────────────────────────────────────────────────────────
// implement.baml — Model cascades & semantic routing  (LIBRARY / STD-LIB code)
//
// Builds on the canonical primitives from _conventions.md (provider-as-marker spine):
//   interface Provider { with_retry; fallback_to; cached }              // bare MARKER, no call
//   interface HttpProvider requires Provider { Body; build_request; send; parse; call; call_with }
//
// What this scenario needs that the spine does NOT give for free:
//   1. A way to read a CONFIDENCE signal off a producing provider, where the
//      signal differs per backend (logprobs on OpenAI, none on Anthropic/Gemini).
//   2. A `Cascade` combinator: cheap-first, escalate on a low signal.
//   3. A `Judge`-gated escalation combinator (the gate is itself an HttpProvider call).
//   4. Semantic / classifier routing: a function returning a Provider per request
//      (this needs NO new primitive — it is just `client` sugar, §3 of the proposal).
//
// The divergence this file makes concrete: confidence is NOT a property `HttpProvider.call<T>`
// can return (call returns a clean T). So we introduce a capability interface `Scored`
// requires Provider, and show two providers diverging inside it. The combinators here
// (Cascade/JudgeGated/SemanticRouter) are request/response, so they `implements HttpProvider`
// and forward call_with to the member that answers (sidecar is not dropped).
// ─────────────────────────────────────────────────────────────────────────────


// ─── net-new capability: a producing call that also reports a confidence signal ───
//
// `HttpProvider.call<T>` deliberately returns a bare T. A cascade needs (value, signal).
// Rather than widen `call` for every HttpProvider, confidence is a CAPABILITY: providers
// that can surface a signal implement `Scored`; those that can't simply don't, and the
// cascade `match`es for it (exactly the §5 negotiation pattern). Like every interaction
// it `requires Provider` (the marker) and adds its own method.
class Scored<T> {
  value: T
  // 0.0..1.0, provider-normalized. Source differs per backend (see impls below).
  confidence: float
  // where the number came from, for observability / silent-regression debugging.
  source: string                  // "logprobs" | "self_report" | "validator" | "none"
}

interface ConfidenceProvider requires Provider {
  // Same prompt as HttpProvider.call, but returns the value WITH a normalized confidence.
  // call_scored is a producing call on the call path -> the CallError channel.
  function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T>
      throws baml.ExtendUnknownError<baml.errors.CallError>
}


// ─── concrete provider errors (each implements the capability interfaces it can arise on) ───
//
// An HTTP non-2xx from either backend, carrying the status. It implements CallError
// and classifies a 429 as a rate-limit so the app can back off without a bespoke class.
class OpenAiHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

class AnthropicHttpError {
  status: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return self.status == 429; }
    function is_parse_error(self)   -> bool { return false; }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Concrete providers — the two backends the background flags as divergent.
// OpenAI: real token logprobs.  Anthropic: no logprobs, falls back to self-report.
// ─────────────────────────────────────────────────────────────────────────────

class OpenAI {
  model: string
  api_key: string
  // logprobs is an OpenAI request field — a plain provider field (§4: options dissolve).
  logprobs: bool

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // SAP-in-prompt encoding for the schema; logprobs toggled by the field.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp = baml.http.send(request);
      if (!resp.ok()) { throw OpenAiHttpError { status: resp.status_code }; }
      resp
    } catch (e) {
      // foreign Io/Timeout from baml.http.send -> normalize; the thrown OpenAiHttpError passes through.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }

  implements ConfidenceProvider {
    // OpenAI: derive a real confidence from token logprobs in the SAME response.
    function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T>
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // We must build the request with logprobs ON regardless of the field, else
      // there is no signal to read. Construct a logprobs-on variant of self.
      let me = OpenAI { model: self.model, api_key: self.api_key, logprobs: true };
      let resp = me.send(me.build_request<T>(prompt));
      let value: T = me.parse<T>(resp);
      // mean token probability over the answer span -> a 0..1 confidence.
      let conf = baml.llm.logprob_mean_confidence(resp.text())
                   catch (e) { _ => { 1.0 } };   // if absent, don't block the value
      Scored<T> { value: value, confidence: conf, source: "logprobs" }
    }
  }
}

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Anthropic forced-tool encoding of the schema; max_tokens is a real field.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let resp = baml.http.send(request);
      if (!resp.ok()) { throw AnthropicHttpError { status: resp.status_code }; }
      resp
    } catch (e) {
      // foreign Io/Timeout from baml.http.send -> normalize; the thrown AnthropicHttpError passes through.
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }

  implements ConfidenceProvider {
    // Anthropic exposes NO token-level probabilities. The ONLY honest signal is a
    // self-reported certainty we must ask for as part of the structured answer.
    // This is the leak the background warns about: the cascade's "confidence" means
    // something *different* here than on OpenAI, and it costs an extra schema field.
    function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T>
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // We can still parse T cleanly; but we cannot derive a calibrated number.
      // Two choices, both unsatisfying — we pick the conservative one: declare the
      // signal absent (1.0 => "never blocks", caller must use a Judge instead).
      let value: T = self.call<T>(prompt);
      Scored<T> { value: value, confidence: 1.0, source: "none" }
    }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Combinator (1): Cheap-first cascade, escalate on low confidence.
//
// Plain non-generic class wrapping two providers, exactly like Fallback (§6).
// The difference from Fallback: Fallback escalates on a thrown Error; Cascade
// escalates on a QUALITY signal from a successful cheap call.
// ─────────────────────────────────────────────────────────────────────────────
class Cascade {
  cheap: Provider
  strong: Provider
  threshold: float                 // escalate when cheap confidence < threshold

  // call / call_with belong to HttpProvider now (provider-as-marker.md). Cascade is a
  // request/response combinator: it OWNS call (the confidence gate) and forwards
  // call_with to whichever member ultimately answers, so the sidecar is not dropped.
  implements HttpProvider {
    type Body = unknown

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // The cheap tier MUST be Scored for a confidence gate to mean anything.
      // If it isn't, we have no signal and (honestly) cannot cascade on confidence
      // — we degrade to "always trust cheap", which the app author should see.
      match (self.cheap) {
        let c: ConfidenceProvider => {
          let s: Scored<T> = c.call_scored<T>(prompt);
          if (s.confidence >= self.threshold) {
            // observability hook: most traffic settles here, at tier-1 prices.
            baml.telemetry.emit("cascade.settled", { tier: "cheap", conf: s.confidence, src: s.source });
            return s.value;
          }
          baml.telemetry.emit("cascade.escalate", { conf: s.confidence, src: s.source });
          // only the residual pays the premium.
          match (self.strong) {
            let h: HttpProvider => { return h.call<T>(prompt); },
            _ => { throw baml.errors.Unsupported { message: "cascade strong tier cannot call" }; },
          }
        },
        // cheap tier reports no signal -> we can't gate on confidence at all.
        _ => {
          baml.telemetry.emit("cascade.no_signal", { tier: "cheap" });
          match (self.cheap) {
            let h: HttpProvider => { return h.call<T>(prompt); },
            _ => { throw baml.errors.Unsupported { message: "cascade cheap tier cannot call" }; },
          }
        },
      }
    }

    // Forward the sidecar projection to the tier that answers. The gate decides WHICH
    // member responds; the sidecar (usage/cost) rides whichever one paid for the call.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.cheap) {
        let c: ConfidenceProvider => {
          let s: Scored<T> = c.call_scored<T>(prompt);
          if (s.confidence >= self.threshold) {
            // settled on cheap: project the cheap tier's meta.
            match (self.cheap) {
              let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
              _ => { throw baml.errors.Unsupported { message: "cascade cheap tier has no request/response sidecar" }; },
            }
          }
          match (self.strong) {
            let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
            _ => { throw baml.errors.Unsupported { message: "cascade strong tier has no request/response sidecar" }; },
          }
        },
        _ => {
          match (self.cheap) {
            let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
            _ => { throw baml.errors.Unsupported { message: "cascade cheap tier has no request/response sidecar" }; },
          }
        },
      }
    }
  }

  // Cascade forwards ConfidenceProvider too: the escalated answer can itself be
  // scored if the strong tier supports it, so a Cascade can nest in a Cascade.
  implements ConfidenceProvider {
    function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T>
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.cheap) {
        let c: ConfidenceProvider => {
          let s: Scored<T> = c.call_scored<T>(prompt);
          if (s.confidence >= self.threshold) { return s; }
          // escalate; re-score on the strong tier if it can, else mark "none".
          match (self.strong) {
            let cs: ConfidenceProvider => { return cs.call_scored<T>(prompt); },
            let h: HttpProvider => { return Scored<T> { value: h.call<T>(prompt), confidence: 1.0, source: "none" }; },
            _ => { throw baml.errors.Unsupported { message: "cascade strong tier cannot call" }; },
          }
        },
        _ => {
          match (self.cheap) {
            let h: HttpProvider => { return Scored<T> { value: h.call<T>(prompt), confidence: 1.0, source: "none" }; },
            _ => { throw baml.errors.Unsupported { message: "cascade cheap tier cannot call" }; },
          }
        },
      }
    }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Combinator (2): Judge-gated escalation.
//
// A SEPARATE provider (the judge) scores the cheap output; below `min_score`
// the request is retried on the strong tier. The judge is itself a billed
// HttpProvider.call — the file makes that explicit (background: "the router is itself
// a call"). The judge produces a Verdict via a structured call.
// ─────────────────────────────────────────────────────────────────────────────
class Verdict {
  score: float          // 0..1
  reason: string
}

class JudgeGated {
  cheap: Provider
  strong: Provider
  judge: Provider       // billed on EVERY request — the eats-the-savings risk
  min_score: float

  // call / call_with belong to HttpProvider now (provider-as-marker.md). JudgeGated is a
  // request/response combinator: it OWNS call (cheap -> judge -> maybe strong) and
  // forwards call_with to the tier that ultimately answers so the sidecar survives.
  implements HttpProvider {
    type Body = unknown

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // 1. cheap produces a candidate.
      let candidate: T = match (self.cheap) {
        let h: HttpProvider => h.call<T>(prompt),
        _ => { throw baml.errors.Unsupported { message: "judge-gated cheap tier cannot call" }; },
      };

      // 2. the judge scores it. NOTE: we render a NEW prompt for the judge from
      //    the original prompt + the candidate. This is a second billed call.
      let judge_prompt = baml.llm.render_judge(prompt, candidate);   // host: build the rubric prompt
      let v: Verdict = match (self.judge) {
        let h: HttpProvider => h.call<Verdict>(judge_prompt),
        _ => { throw baml.errors.Unsupported { message: "judge-gated judge cannot call" }; },
      };
      baml.telemetry.emit("judge.scored", { score: v.score, reason: v.reason });

      if (v.score >= self.min_score) { return candidate; }

      // 3. below threshold -> pay for the strong tier.
      baml.telemetry.emit("judge.escalate", { score: v.score });
      match (self.strong) {
        let h: HttpProvider => { return h.call<T>(prompt); },
        _ => { throw baml.errors.Unsupported { message: "judge-gated strong tier cannot call" }; },
      }
    }

    // Forward the sidecar to the tier that answers. The judge gate runs first to decide
    // cheap-vs-strong; we project the meta of whichever member produces the returned T.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (candidate, cheap_meta): (T, U) = match (self.cheap) {
        let h: HttpProvider => h.call_with<T, U>(prompt, project),
        _ => { throw baml.errors.Unsupported { message: "judge-gated cheap tier has no request/response sidecar" }; },
      };
      let judge_prompt = baml.llm.render_judge(prompt, candidate);
      let v: Verdict = match (self.judge) {
        let h: HttpProvider => h.call<Verdict>(judge_prompt),
        _ => { throw baml.errors.Unsupported { message: "judge-gated judge cannot call" }; },
      };
      baml.telemetry.emit("judge.scored", { score: v.score, reason: v.reason });
      if (v.score >= self.min_score) { return (candidate, cheap_meta); }
      baml.telemetry.emit("judge.escalate", { score: v.score });
      match (self.strong) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _ => { throw baml.errors.Unsupported { message: "judge-gated strong tier has no request/response sidecar" }; },
      }
    }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Combinator (3): SemanticRouter — embedding-similarity routing to a tier.
//
// A classifier that picks the tier BEFORE generation, off labeled exemplars.
// The embedding lookup is itself a (cheap) provider/host call. Unlike Cascade,
// this never issues a wasted cheap generation on a known-hard request.
//
// Note: a SINGLE-shot classifier route (string in -> Provider out) needs NO
// combinator class at all — it is just `client` sugar (see usage.baml). We model
// the *embedding* router as a class because it carries state (the exemplar table).
// ─────────────────────────────────────────────────────────────────────────────
class Exemplar {
  text: string
  tier: string          // "easy" | "hard"
}

class SemanticRouter {
  exemplars: Exemplar[]
  easy: Provider
  hard: Provider

  // call / call_with belong to HttpProvider now (provider-as-marker.md). SemanticRouter is a
  // request/response combinator: it picks a tier up front, then OWNS call / forwards call_with
  // to the chosen tier so the sidecar (usage/cost) rides the member that answered.
  implements HttpProvider {
    type Body = unknown

    function route(self, prompt: baml.llm.PromptAst) -> Provider {
      // embed the incoming prompt + nearest-exemplar vote. This is a billed
      // embedding call (host) — the router-is-a-call cost, paid up front.
      let tier = baml.llm.nearest_exemplar_tier(prompt, self.exemplars)
                   catch (e) { _ => { "hard" } };   // fail safe to the strong tier
      baml.telemetry.emit("router.dispatch", { tier: tier });
      match (tier) {
        "easy" => self.easy,
        _      => self.hard,
      }
    }

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.route(prompt)) {
        let h: HttpProvider => { return h.call<T>(prompt); },
        _ => { throw baml.errors.Unsupported { message: "routed tier cannot call" }; },
      }
    }

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.route(prompt)) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _ => { throw baml.errors.Unsupported { message: "routed tier has no request/response sidecar" }; },
      }
    }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Fluent sugar: add cascade/judge as default methods so they chain like §6's
// with_retry / fallback_to. These are COMBINATOR FACTORIES — exactly the kind of member
// the `Provider` MARKER hosts (it carries no interaction, only these factories). They EXTEND
// interface Provider with new defaults — flagged in evaluation.md as net-new surface on the marker.
// ─────────────────────────────────────────────────────────────────────────────
interface Provider {
  function escalate_to(self, strong: Provider, threshold: float) -> Cascade {
    Cascade { cheap: self, strong: strong, threshold: threshold }
  }
  function judged_by(self, judge: Provider, strong: Provider, min_score: float) -> JudgeGated {
    JudgeGated { cheap: self, strong: strong, judge: judge, min_score: min_score }
  }
}


// ─── shared read-interface reused from the spine (§4) ───
interface ChatModelOptions { model: string, api_key: string? }


// ─── host boundaries this file relies on (net-new flags in evaluation.md) ───
//   baml.llm.logprob_mean_confidence(text: string) -> float throws        // OpenAI-only signal
//   baml.llm.render_judge(prompt, candidate) -> baml.llm.PromptAst        // judge rubric prompt
//   baml.llm.nearest_exemplar_tier(prompt, Exemplar[]) -> string throws   // embedding KNN
//   baml.telemetry.emit(event: string, attrs) -> null                     // silent-regression watch
