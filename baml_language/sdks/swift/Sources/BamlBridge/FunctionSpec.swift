import Foundation

/// Opaque, engine-owned `ai.FunctionSpec<Final>` capability.
///
/// Generated `Fn_spec(...)` entry points obtain this value by invoking the
/// Spec operation on the authored function. Every other operation is an
/// ordinary method on this proxy; no synthetic `$parse`, `$render_prompt`,
/// or `$build_request` function is involved. Streaming remains a separate
/// `Fn_stream(...)` projection because it carries the PPIR partial type.
public final class BamlFunctionSpec<Final>: @unchecked Sendable {
    private let handle: BamlHandle

    init(handle: BamlHandle) {
        self.handle = handle
    }

    /// Execute the bound recipe and decode its final output.
    public func call(
        client: (any BamlEncodable)? = nil,
        onEvent: BamlHostCallable? = nil
    ) throws -> Final where Final: BamlDecodable {
        try handle.runtime.callSync(
            "ai.FunctionSpec.call",
            args: [("self", self), ("client", client), ("on_event", onEvent)]
        )
    }

    /// Execute the bound recipe asynchronously and decode its final output.
    public func callAsync(
        client: (any BamlEncodable)? = nil,
        onEvent: BamlHostCallable? = nil
    ) async throws -> Final where Final: BamlDecodable {
        try await handle.runtime.call(
            "ai.FunctionSpec.call",
            args: [("self", self), ("client", client), ("on_event", onEvent)]
        )
    }

    /// Parse an existing model response with this spec's realized output type.
    public func parse(json: String) throws -> Final where Final: BamlDecodable {
        try handle.runtime.callSync(
            "ai.FunctionSpec.parse",
            args: [("self", self), ("json", json)]
        )
    }

    public func parseAsync(json: String) async throws -> Final where Final: BamlDecodable {
        try await handle.runtime.call(
            "ai.FunctionSpec.parse",
            args: [("self", self), ("json", json)]
        )
    }

    /// Render the portable, provider-neutral prompt for this recipe.
    public func prompt() throws -> BamlPrompt {
        try handle.runtime.callSync("ai.FunctionSpec.prompt", args: [("self", self)])
    }

    public func promptAsync() async throws -> BamlPrompt {
        try await handle.runtime.call("ai.FunctionSpec.prompt", args: [("self", self)])
    }

    /// Build the provider HTTP request without invoking the model.
    ///
    /// `Request` is inferred from the assignment/return context. Generated
    /// SDKs normally use their `baml.http.Request` model here.
    public func buildRequest<Request: BamlDecodable>(
        as _: Request.Type = Request.self,
        client: (any BamlEncodable)? = nil
    ) throws -> Request {
        try handle.runtime.callSync(
            "ai.FunctionSpec.build_request",
            args: [("self", self), ("client", client)]
        )
    }

    public func buildRequestAsync<Request: BamlDecodable>(
        as _: Request.Type = Request.self,
        client: (any BamlEncodable)? = nil
    ) async throws -> Request {
        try await handle.runtime.call(
            "ai.FunctionSpec.build_request",
            args: [("self", self), ("client", client)]
        )
    }

    public func name() throws -> String {
        try handle.runtime.callSync("ai.FunctionSpec.name", args: [("self", self)])
    }

    public func nameAsync() async throws -> String {
        try await handle.runtime.call("ai.FunctionSpec.name", args: [("self", self)])
    }
}

extension BamlFunctionSpec: Equatable {
    /// Specs are live capabilities; equality is engine-resource identity.
    public static func == (lhs: BamlFunctionSpec, rhs: BamlFunctionSpec) -> Bool {
        lhs.handle == rhs.handle
    }
}

extension BamlFunctionSpec: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        handle._bamlEncode()
    }
}

extension BamlFunctionSpec: BamlDecodable {
    public static func _bamlDecode(
        _ value: BamlOutboundValue
    ) throws -> BamlFunctionSpec<Final> {
        let handle = try BamlHandle._bamlDecode(value)
        guard handle.handleType == .adtFunctionSpec else {
            throw BamlDecodeError.typeMismatch(
                expected: "FunctionSpec handle",
                got: "handle type \(handle.handleType)"
            )
        }
        return BamlFunctionSpec(handle: handle)
    }
}
