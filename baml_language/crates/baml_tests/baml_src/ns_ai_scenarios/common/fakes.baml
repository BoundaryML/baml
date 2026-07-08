// Offline fake providers for scenario tests (DCP §1.5/§1.6): pure BAML, no
// network. The offline tier runs against these; the live tier lives in
// `testset "integ-test"` blocks.

/// Always answers `reply` (SAP-parsed into the caller's `T`); metadata is the
/// stdlib best-effort `LegacyMeta`. Overrides `call_messages_with`, so the
/// whole HttpProvider surface (call/call_with/call_messages) works offline.
class EchoProvider {
    reply: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["echo has no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["echo does not send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["echo parse failed"] },
            }
        }
        function meta_of(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}

/// A Provider with no capabilities at all — the negotiation-failure fixture.
class NoCapProvider {
    implements baml.ai.Provider {}
}

/// Always fails with a typed CallError-implementing error (Unsupported) —
/// the reliability-combinator fixture.
class AlwaysFailProvider {
    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["always fails"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["always fails"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["always fails"] }
        }
        function meta_of(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }
    }
}
