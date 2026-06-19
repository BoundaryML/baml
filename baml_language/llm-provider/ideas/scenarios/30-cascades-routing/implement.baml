// ─────────────────────────────────────────────────────────────────────────────
// implement.baml — Model cascades & semantic routing  (LIBRARY / STD-LIB code)
//
// Builds on the canonical primitives from _conventions.md:
//   interface Provider { call<T>; with_retry; fallback_to; cached }
//   interface HttpProvider requires Provider { Body; build_request; send; parse }
//
// What this scenario needs that the spine does NOT give for free:
//   1. A way to read a CONFIDENCE signal off a producing provider, where the
//      signal differs per backend (logprobs on OpenAI, none on Anthropic/Gemini).
//   2. A `Cascade` combinator: cheap-first, escalate on a low signal.
//   3. A `Judge`-gated escalation combinator (the gate is itself a Provider call).
//   4. Semantic / classifier routing: a function returning a Provider per request
//      (this needs NO new primitive — it is just `client` sugar, §3 of the proposal).
//
// The divergence this file makes concrete: confidence is NOT a property the base
// `call<T>` can return (call returns a clean T). So we introduce a capability
// interface `Scored` requires Provider, and show two providers diverging inside it.
// ─────────────────────────────────────────────────────────────────────────────


// ─── net-new capability: a producing call that also reports a confidence signal ───
//
// `call<T>` deliberately returns a bare T. A cascade needs (value, signal). Rather
// than widen `call` for everyone, confidence is a CAPABILITY: providers that can
// surface a signal implement `Scored`; those that can't simply don't, and the
// cascade `match`es for it (exactly the §5 negotiation pattern).
class Scored<T> {
  value: T
  // 0.0..1.0, provider-normalized. Source differs per backend (see impls below).
  confidence: float
  // where the number came from, for observability / silent-regression debugging.
  source: string                  // "logprobs" | "self_report" | "validator" | "none"
}

interface ConfidenceProvider requires Provider {
  // Same prompt as `call`, but returns the value WITH a normalized confidence.
  function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T> throws Error
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // SAP-in-prompt encoding for the schema; logprobs toggled by the field.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "openai http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements ConfidenceProvider {
    // OpenAI: derive a real confidence from token logprobs in the SAME response.
    function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T> throws baml.errors.LlmClient {
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Anthropic forced-tool encoding of the schema; max_tokens is a real field.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "anthropic http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements ConfidenceProvider {
    // Anthropic exposes NO token-level probabilities. The ONLY honest signal is a
    // self-reported certainty we must ask for as part of the structured answer.
    // This is the leak the background warns about: the cascade's "confidence" means
    // something *different* here than on OpenAI, and it costs an extra schema field.
    function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T> throws baml.errors.LlmClient {
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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
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
          return self.strong.call<T>(prompt);
        },
        // cheap tier reports no signal -> we can't gate on confidence at all.
        _ => {
          baml.telemetry.emit("cascade.no_signal", { tier: "cheap" });
          return self.cheap.call<T>(prompt);
        },
      }
    }
  }

  // Cascade forwards ConfidenceProvider too: the escalated answer can itself be
  // scored if the strong tier supports it, so a Cascade can nest in a Cascade.
  implements ConfidenceProvider {
    function call_scored<T>(self, prompt: baml.llm.PromptAst) -> Scored<T> throws baml.errors.LlmClient {
      match (self.cheap) {
        let c: ConfidenceProvider => {
          let s: Scored<T> = c.call_scored<T>(prompt);
          if (s.confidence >= self.threshold) { return s; }
          // escalate; re-score on the strong tier if it can, else mark "none".
          match (self.strong) {
            let cs: ConfidenceProvider => { return cs.call_scored<T>(prompt); },
            _ => { return Scored<T> { value: self.strong.call<T>(prompt), confidence: 1.0, source: "none" }; },
          }
        },
        _ => { return Scored<T> { value: self.cheap.call<T>(prompt), confidence: 1.0, source: "none" }; },
      }
    }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Combinator (2): Judge-gated escalation.
//
// A SEPARATE provider (the judge) scores the cheap output; below `min_score`
// the request is retried on the strong tier. The judge is itself a billed
// Provider.call — the file makes that explicit (background: "the router is itself
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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // 1. cheap produces a candidate.
      let candidate: T = self.cheap.call<T>(prompt);

      // 2. the judge scores it. NOTE: we render a NEW prompt for the judge from
      //    the original prompt + the candidate. This is a second billed call.
      let judge_prompt = baml.llm.render_judge(prompt, candidate);   // host: build the rubric prompt
      let v: Verdict = self.judge.call<Verdict>(judge_prompt);
      baml.telemetry.emit("judge.scored", { score: v.score, reason: v.reason });

      if (v.score >= self.min_score) { return candidate; }

      // 3. below threshold -> pay for the strong tier.
      baml.telemetry.emit("judge.escalate", { score: v.score });
      return self.strong.call<T>(prompt);
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

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // embed the incoming prompt + nearest-exemplar vote. This is a billed
      // embedding call (host) — the router-is-a-call cost, paid up front.
      let tier = baml.llm.nearest_exemplar_tier(prompt, self.exemplars)
                   catch (e) { _ => { "hard" } };   // fail safe to the strong tier
      baml.telemetry.emit("router.dispatch", { tier: tier });
      match (tier) {
        "easy" => { return self.easy.call<T>(prompt); },
        _      => { return self.hard.call<T>(prompt); },
      }
    }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// Fluent sugar: add cascade/judge as default methods so they chain like §6's
// with_retry / fallback_to. These EXTEND interface Provider with new defaults —
// flagged in evaluation.md as net-new surface on the base interface.
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
