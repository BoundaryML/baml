// The OpenAI provider — the first concrete implementation of the capability spine,
// written entirely in BAML (request/response logic in BAML; only leaf primitives —
// `baml.http.send`, `baml.json.*`, `baml.env` — are host functions). This is the
// reference every other provider (and a user's custom provider) follows.

// --- wire shapes (Chat Completions) ---
class OpenAiMessageWire {
    content: string,
}

class OpenAiChoiceWire {
    message: OpenAiMessageWire,
    finish_reason: string?,
}

class OpenAiResponseWire {
    choices: OpenAiChoiceWire[],
}

/// Lazy `ResponseMeta` over the raw HTTP response — each dimension re-reads the body.
class OpenAiResponseMeta {
    raw: root.http.Response,

    implements root.ai.ResponseMeta {
        function finish_reason(
            self,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let wire: OpenAiResponseWire = root.json.from_json<OpenAiResponseWire>(root.json.parse(self.raw.text())) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai meta parse failed"] }
                },
            };
            match (wire.choices.at(0)) {
                let c: OpenAiChoiceWire => c.finish_reason ?? "",
                _ => "",
            }
        }
    }
}

/// OpenAI Chat Completions provider. Provider-specific options are plain fields.
class OpenAi {
    model: string,
    api_key: string,
    base_url: string?,

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        type Body = root.http.Response

        function build_request<T>(
            self,
            prompt: string,
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            let body: json = { "model": self.model, "messages": [{ "role": "user", "content": prompt }] };
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: { "Authorization": "Bearer " + self.api_key, "Content-Type": "application/json" },
                body: root.json.stringify(body),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> root.http.Response throws root.errors.CallError | root.errors.UnknownError {
            root.http.send(request) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["openai send failed"] },
            }
        }

        // Extracts choices[0].message.content, then coerces the content to `T`.
        // For `T = string` the content passes through; for a class `T` the content is a
        // JSON string SAP/from_json can decode. (Full SAP repair-parse is a later step.)
        function parse<T>(
            self,
            from: root.http.Response,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            let wire: OpenAiResponseWire = root.json.from_json<OpenAiResponseWire>(root.json.parse(from.text())) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["openai parse failed"] },
            };
            match (wire.choices.at(0)) {
                let c: OpenAiChoiceWire => {
                    root.json.from_json<T>(c.message.content) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: e, message: ["openai content coerce failed"] }
                        },
                    }
                },
                _ => throw root.errors.UnknownError { data: null, message: ["openai: no choices"] },
            }
        }

        function meta_of(
            self,
            from: root.http.Response,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            OpenAiResponseMeta { raw: from }
        }
    }
}
