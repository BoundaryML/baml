// Offline fake provider for the workflow-reference scenarios: pure BAML, no
// network and no dependency on builtin provider implementations.
class EchoProvider {
    reply: string,

    implements root.ai.Provider {
        function display_name(self) -> string throws never { "echo" }
        function render_shorthand(self) -> string throws never { "openai/echo" }
    }

    implements root.ai.Generate {
        function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
            throws baml.errors.UnknownError {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["echo parse failed"] },
            };
            root.ai.Response<T> {
                value: value,
                meta: root.ai.Meta {
                    provider: "echo",
                    model: "fake",
                    request_id: null,
                    finish_reason: "stop",
                    usage: null,
                    attributes: {},
                    raw: null,
                },
            }
        }
    }
}
