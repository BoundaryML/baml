// A live OpenAI chat-completions client. render/invoke/ingest, stateless.
//
// The real feature lowers the journal to native messages inside the runtime;
// this reference renders a text transcript and speaks a small JSON protocol
// for actions. Agent-specific output parsing is injected as an ActionParser
// (in the runtime this is the generated step machinery, not user code).

// Classifies one assistant message into action events.
interface ActionParser {
    function parse(self, content: string, call_seq: int) -> Event[] throws unknown
}

class OaChoiceMessage {
    content: string?,
}

class OaChoice {
    message: OaChoiceMessage,
}

class OaUsage {
    prompt_tokens: int,
    completion_tokens: int,
}

class OaResponse {
    choices: OaChoice[],
    usage: OaUsage?,
}

class OpenAiClient {
    model: string,
    api_key: string,
    parser: ActionParser,
    calls: int,

    implements Client {
        function id(self) -> string {
            `openai/${self.model}`
        }

        function render(
            self,
            j: Journal<Event>,
            tb: Toolbox,
            output_schema: string,
        ) -> ProviderRequest {
            let prompt = `
                You are an agent working toward a final result.

                ## Tools
                ${tb.render()}

                ## Conversation and work so far
                ${render_transcript(j)}

                Respond with EXACTLY ONE JSON object, nothing else:
                - to call a tool:            {"kind": "tool", "tool": "<name>", "args": { ... }}
                - to reply conversationally: {"kind": "reply", "message": "..."}
                - to finish with the answer: {"kind": "final", "value": <the result>}

                The final value must match this schema:
                ${output_schema}
            `;
            let body = baml
                .json
                .to_string(
                    { "model": self.model, "messages": [{ "role": "user", "content": prompt }] },
                );
            ProviderRequest {
                url: "https://api.openai.com/v1/chat/completions",
                headers: { "authorization": `Bearer ${self.api_key}`, "content-type": "application/json" },
                body_json: body,
            }
        }

        function invoke(self, req: ProviderRequest) -> ProviderResponse {
            let resp = baml
                .http
                .send(
                    baml.http.Request {
                        method: "POST",
                        url: req.url,
                        headers: req.headers,
                        body: req.body_json,
                    },
                );
            ProviderResponse { status: resp.status_code, body_json: resp.text() }
        }

        function ingest(self, resp: ProviderResponse) -> Event[] {
            self.calls += 1;
            let out: Event[] = [];
            let parsed = baml.json.from_string<OaResponse>(resp.body_json) catch_all (e) {
                _ => {
                    throw baml.errors.InvalidArgument { message: `provider error ${resp.status}: ${resp.body_json}` }
                }
            };
            let content = parsed.choices.at(0)?.message.content ?? "";
            if let u: OaUsage = parsed.usage {
                out.push(
                    Usage { input_tokens: u.prompt_tokens, output_tokens: u.completion_tokens },
                );
            }
            //# action events first, so pending_tools is set before the
            //# AssistantMessage reaches the policy
            for (let e in self.parser.parse(content, self.calls)) {
                out.push(e);
            }
            out.push(
                AssistantMessage {
                    content: content,
                    raw_json: resp.body_json, // raw payload, verbatim
                    provider: `openai/${self.model}`,
                },
            );
            out
        }
    }
}
