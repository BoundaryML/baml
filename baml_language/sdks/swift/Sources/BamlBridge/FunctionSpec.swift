import Foundation

/// Opaque, engine-owned `ai.FunctionSpec<Final>` capability.
///
/// Generated `Fn_spec(...)` entry points obtain this value by invoking the
/// Spec operation on the authored function. Every other operation is an
/// ordinary method on this proxy; no synthetic `$parse`, `$render_prompt`,
/// or `$build_request` function is involved. Streaming remains a separate
/// `Fn_stream(...)` projection because it carries the partial type.
///
/// The final output decodes as the spec's declared final type: the spec keeps
/// the slot it was decoded under, so a final type with no `BamlDecodable`
/// conformance (a native tuple) decodes too.
public final class BamlFunctionSpec<Final>: @unchecked Sendable {
    private let handle: BamlHandle
    private let decodeFinal: (BamlOutboundValue) throws -> Final

    init(handle: BamlHandle, decodeFinal: @escaping (BamlOutboundValue) throws -> Final) {
        self.handle = handle
        self.decodeFinal = decodeFinal
    }

    /// Execute the bound recipe and decode its final output.
    public func callSync(
        client: (any BamlTyped)? = nil,
        onEvent: BamlHostCallable? = nil
    ) throws -> Final {
        try decodeFinal(BamlRuntime.shared.callRawSync(
            "ai.FunctionSpec.call",
            args: [BamlArgument("self", handle: handle), BamlArgument("client", client), BamlArgument("on_event", onEvent)]
        ))
    }

    /// Execute the bound recipe asynchronously and decode its final output.
    public func call(
        client: (any BamlTyped)? = nil,
        onEvent: BamlHostCallable? = nil
    ) async throws -> Final {
        try decodeFinal(await BamlRuntime.shared.callRaw(
            "ai.FunctionSpec.call",
            args: [BamlArgument("self", handle: handle), BamlArgument("client", client), BamlArgument("on_event", onEvent)]
        ))
    }

    /// Parse an existing model response with this spec's realized output type.
    public func parseSync(json: String) throws -> Final {
        try decodeFinal(BamlRuntime.shared.callRawSync(
            "ai.FunctionSpec.parse",
            args: [BamlArgument("self", handle: handle), BamlArgument("json", json, as: String._bamlSlot)]
        ))
    }

    public func parse(json: String) async throws -> Final {
        try decodeFinal(await BamlRuntime.shared.callRaw(
            "ai.FunctionSpec.parse",
            args: [BamlArgument("self", handle: handle), BamlArgument("json", json, as: String._bamlSlot)]
        ))
    }

    /// Render the portable, provider-neutral prompt for this recipe.
    public func promptSync() throws -> BamlPrompt {
        try BamlRuntime.shared.callSync("ai.FunctionSpec.prompt", args: [BamlArgument("self", handle: handle)])
    }

    public func prompt() async throws -> BamlPrompt {
        try await BamlRuntime.shared.call("ai.FunctionSpec.prompt", args: [BamlArgument("self", handle: handle)])
    }

    /// Build the provider HTTP request without invoking the model.
    ///
    /// `Request` is inferred from the assignment/return context. Generated
    /// SDKs normally use their `baml.http.Request` model here.
    public func buildRequestSync<Request: BamlDecodable>(
        as _: Request.Type = Request.self,
        client: (any BamlTyped)? = nil
    ) throws -> Request {
        try BamlRuntime.shared.callSync(
            "ai.FunctionSpec.build_request",
            args: [BamlArgument("self", handle: handle), BamlArgument("client", client)]
        )
    }

    public func buildRequest<Request: BamlDecodable>(
        as _: Request.Type = Request.self,
        client: (any BamlTyped)? = nil
    ) async throws -> Request {
        try await BamlRuntime.shared.call(
            "ai.FunctionSpec.build_request",
            args: [BamlArgument("self", handle: handle), BamlArgument("client", client)]
        )
    }

    public func nameSync() throws -> String {
        try BamlRuntime.shared.callSync("ai.FunctionSpec.name", args: [BamlArgument("self", handle: handle)])
    }

    public func name() async throws -> String {
        try await BamlRuntime.shared.call("ai.FunctionSpec.name", args: [BamlArgument("self", handle: handle)])
    }
}

extension BamlFunctionSpec: Equatable {
    /// Specs are live capabilities; equality is engine-resource identity.
    public static func == (lhs: BamlFunctionSpec, rhs: BamlFunctionSpec) -> Bool {
        lhs.handle == rhs.handle
    }
}

extension BamlFunctionSpec: BamlLeafEncodable {
    /// A spec rides as its bare handle, whose type the engine knows.
    public func _bamlEncode(as _: BamlSlot<BamlFunctionSpec<Final>>) throws -> BamlInboundValue {
        try handle._bamlEncode(as: ._leaf(.rustType))
    }
}

extension BamlFunctionSpec: BamlTyped where Final: BamlTyped {
    public static var _bamlSlot: BamlSlot<BamlFunctionSpec<Final>> {
        BamlSlots.functionSpec("ai.FunctionSpec", Final._bamlSlot)
    }
}

extension BamlSlots {
    /// The slot of the function spec class `fqn` with final type `final`. A
    /// spec rides as its handle, so the slot states only its type, and
    /// decodes a spec whose final output decodes under `final`.
    public static func functionSpec<Final>(
        _ fqn: String,
        _ final: BamlSlot<Final>
    ) -> BamlSlot<BamlFunctionSpec<Final>> {
        let type = BamlTypeDescriptor.classType(fqn, typeArguments: [final.type])
        let codec = BamlSlotCodec(
            encode: { try ($0 as! BamlFunctionSpec<Final>)._bamlEncode(as: ._leaf(type)) },
            decode: { try BamlFunctionSpec<Final>.decode($0, decodeFinal: final._bamlDecode) },
            equal: { ($0 as! BamlFunctionSpec<Final>) == ($1 as! BamlFunctionSpec<Final>) }
        )
        return BamlSlot(BamlSlotNode(type: type, shape: .leaf, codec: codec))
    }
}

extension BamlFunctionSpec: BamlDecodable {
    /// A spec decoded with no declared slot decodes its final output through
    /// its own `BamlDecodable` conformance.
    public static func _bamlDecode(
        _ value: BamlOutboundValue
    ) throws -> BamlFunctionSpec<Final> {
        try decode(value) { try decodeOwn($0, as: Final.self) }
    }

    static func decode(
        _ value: BamlOutboundValue,
        decodeFinal: @escaping (BamlOutboundValue) throws -> Final
    ) throws -> BamlFunctionSpec<Final> {
        let handle = try BamlHandle._bamlDecode(value)
        guard handle.handleType == .adtFunctionSpec else {
            throw BamlDecodeError.typeMismatch(
                expected: "FunctionSpec handle",
                got: "handle type \(handle.handleType)"
            )
        }
        return BamlFunctionSpec(handle: handle, decodeFinal: decodeFinal)
    }
}
