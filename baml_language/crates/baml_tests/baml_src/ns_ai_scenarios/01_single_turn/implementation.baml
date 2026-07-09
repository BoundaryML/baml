// scenario 01 — single-turn text: ../../../../../llm-provider/ideas/scenarios/01-single-turn-text/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design's implement.baml hand-rolled three providers (Anthropic / OpenAI /
// Gemini) plus a ResponseMeta interface, Usage, Supported<T>, and per-provider
// wire classes. On the REAL surface all of those are already stdlib:
//   - baml.ai.Anthropic / baml.ai.OpenAi / baml.ai.Gemini are real HttpProviders
//     (each `implements HttpProvider` and inherits `call`/`call_with` from the
//     codec default — exactly the design's "reused unchanged" claim in §a).
//   - baml.ai.ResponseMeta / Usage / Unavailable are stdlib; the design's
//     `Supported<T> = T | Unsupported` is spelled inline as `T | Unavailable`.
//   - Body decoding uses stdlib baml.json / baml.sap (no invented json_path).
//
// So the ONLY genuinely net-new surface this scenario introduces is the
// REFUSAL CHANNEL (design §b.4): a refusal is HTTP 200 with no usable T and a
// reason in a separate field — neither the answer nor a transport failure. We
// model it as a concrete error class on the `CallError` channel, and prove the
// design's load-bearing rule: `parse` must inspect the refusal channel BEFORE
// it ever calls SAP, or it surfaces "" and misclassifies a decline as a
// malformed answer. The two offline fakes below let the usage.baml tests
// exercise refusal and the usage sidecar deterministically (no network).


// ---------------------------------------------------------------------------
// NET-NEW: a refusal is a typed channel, not the answer and not a thrown
// transport error. The model returned 200 OK, produced no usable T, and told
// us why in a separate field (Anthropic stop_reason "refusal", OpenAI
// choices[0].message.refusal, Gemini finishReason "SAFETY"). We surface it as a
// CONCRETE class on the `baml.errors.CallError` channel so callers branch on it
// (vs a transport failure). All three call classifiers are false — it is none
// of network/rate-limit/parse — but on the Failure super-interface it honestly
// reports `is_policy_refusal() == true`.
// ---------------------------------------------------------------------------
class s01_Refused {
    // Provider-specific category, normalized best-effort (e.g. Anthropic
    // stop_details.category, OpenAI "policy", Gemini finishReason).
    category: string,
    message: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }

    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #1 — a provider that DECLINES. It runs the real codec path
// (build_request -> send -> parse) so the refusal is caught exactly where the
// design puts it: inside `parse`, BEFORE SAP. `send` returns a canned wire body
// carrying the decline; `parse` inspects the refusal field first and throws
// s01_Refused — it never reaches `baml.sap.parse`. This is the design's
// "check refusal before parsing" rule made executable with no network.
// ---------------------------------------------------------------------------
class s01_RefusingProvider {
    // The category the wire body reports (e.g. "policy", "safety").
    category: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            // A real provider would re-shape `messages` into its envelope here;
            // for the offline decline the request is inert.
            baml.http.Request { method: "POST", url: "https://example.invalid/refuse", headers: {}, body: "" }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            // Canned 200-OK wire body: the decline lands in a `refusal` field,
            // and the answer text you'd read is empty.
            "{\"refusal\": \"" + self.category + "\", \"content\": \"\"}"
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.CallError | baml.errors.UnknownError {
            // REFUSAL CHANNEL FIRST — decode, and if the decline field is set,
            // throw before ever touching SAP. Reading the (empty) content blindly
            // would surface "" and misclassify a refusal as a malformed answer.
            let j = baml.json.parse(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["refuse parse"] },
            };
            let refusal = baml.json.path_or<string>(j, ".refusal", "");
            if (refusal != "") {
                throw s01_Refused { category: refusal, message: "provider declined" };
            }
            // Only now would we SAP-parse the answer text (unreached on a decline).
            baml.sap.parse<T>(baml.json.path_or<string>(j, ".content", "")) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["refuse content parse"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #2 — a provider that reports SCRIPTED token usage, so the usage
// sidecar (design §b.3: `call_with` + ResponseMeta projection) carries real
// numbers offline. `logprobs()` is `Unavailable` — the design's "typed can't,
// not a silent zero" (`Supported<T>`), which the app can branch on.
// ---------------------------------------------------------------------------
class s01_MeteredMeta {
    inp: int,
    out: int,
    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.inp, output_tokens: self.out }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "chat: no reasoning trace" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            // The three chat providers here can't report logprobs — a TYPED
            // "can't", not a zeroed array (design §b.3 Supported<T>).
            baml.ai.Unavailable { reason: "chat: logprobs not requested" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
    }
}

class s01_MeteredEcho {
    reply: string,
    inp: int,
    out: int,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            baml.http.Request { method: "POST", url: "https://example.invalid/echo", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            self.reply
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["metered parse"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s01_MeteredMeta { inp: self.inp, out: self.out }
        }
    }
}
