import Foundation

/// Portable bridge representation of `ai.Prompt`.
///
/// This value owns a copied prompt tree, never an engine handle. The generated
/// stdlib facade can add rich `text()`/`messages()` helpers while using this
/// type as its lossless transport and persistence layer.
public struct BamlPrompt: Sendable, Equatable {
    private let value: BamlBridge_Cffi_V1_BamlValuePromptAst

    private init(_ value: BamlBridge_Cffi_V1_BamlValuePromptAst) {
        self.value = value
    }

    /// Reconstruct a prompt from its canonical protobuf payload.
    public init(serializedData: Data) throws {
        let value = try BamlBridge_Cffi_V1_BamlValuePromptAst(serializedBytes: serializedData)
        guard value.value != nil else {
            throw BamlDecodeError.typeMismatch(expected: "prompt tree", got: "empty prompt")
        }
        self.value = value
    }

    /// A detached payload suitable for persistence or another runtime.
    public func serializedData() throws -> Data {
        try value.serializedData()
    }

    /// Render readable prompt text through the canonical `ai.Prompt` method.
    /// The portable tree is copied back into the runtime on every call, so the
    /// same value remains reusable after persistence or across runtimes.
    public func text() throws -> String {
        try BamlRuntime.shared.callSync("ai.Prompt.text", args: [("self", self)])
    }

    public func textAsync() async throws -> String {
        try await BamlRuntime.shared.call("ai.Prompt.text", args: [("self", self)])
    }

    /// Return a portable structural view of the prompt's ordered messages.
    public func messages() throws -> [BamlPromptMessage] {
        try BamlRuntime.shared.callSync("ai.Prompt.messages", args: [("self", self)])
    }

    public func messagesAsync() async throws -> [BamlPromptMessage] {
        try await BamlRuntime.shared.call("ai.Prompt.messages", args: [("self", self)])
    }
}

extension BamlPrompt: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { .prompt }

    public func _bamlEncode() -> BamlInboundValue {
        var inbound = BamlBridge_Cffi_V1_InboundValue()
        inbound.promptAstValue = value
        return BamlInboundValue(inbound)
    }
}

extension BamlPrompt: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlPrompt {
        let raw = value.normalized
        guard case .promptAstValue(let prompt) = raw.value, prompt.value != nil else {
            throw BamlDecodeError.typeMismatch(expected: "BamlPrompt", got: wireArmName(raw))
        }
        return BamlPrompt(prompt)
    }
}

/// Portable media arm retained inside a rendered prompt part.
public enum BamlPromptMedia: Sendable, Equatable {
    case image(BamlHandle)
    case audio(BamlHandle)
    case video(BamlHandle)
    case pdf(BamlHandle)
}

/// One structural part of a rendered prompt message.
public enum BamlPromptPart: Sendable, Equatable, BamlDecodable {
    case text(String)
    case media(BamlPromptMedia)

    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlPromptPart {
        let raw = value.normalized
        if case .stringValue(let text) = raw.value {
            return .text(text)
        }
        if case .literalValue(let literal) = raw.value,
           case .stringValue(let text) = literal.literal
        {
            return .text(text)
        }

        let handle = try BamlHandle._bamlDecode(value)
        switch handle.handleType {
        case .adtMediaImage: return .media(.image(handle))
        case .adtMediaAudio: return .media(.audio(handle))
        case .adtMediaVideo: return .media(.video(handle))
        case .adtMediaPdf: return .media(.pdf(handle))
        default:
            throw BamlDecodeError.typeMismatch(
                expected: "prompt text or media",
                got: "handle type \(handle.handleType)"
            )
        }
    }
}

/// JSON value used for per-message provider metadata.
public indirect enum BamlPromptJSON: Sendable, Equatable, BamlDecodable {
    case null
    case string(String)
    case int(Int)
    case bigint(String)
    case float(Double)
    case bool(Bool)
    case list([BamlPromptJSON])
    case object([String: BamlPromptJSON])

    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlPromptJSON {
        let raw = value.normalized
        switch raw.value {
        case nil, .nullValue:
            return .null
        case .stringValue(let value):
            return .string(value)
        case .intValue(let value):
            return .int(Int(value))
        case .bigintValue(let value):
            return .bigint(value)
        case .floatValue(let value):
            return .float(value)
        case .boolValue(let value):
            return .bool(value)
        case .literalValue(let literal):
            switch literal.literal {
            case .stringValue(let value): return .string(value)
            case .intValue(let value): return .int(Int(value))
            case .bigintValue(let value): return .bigint(value)
            case .floatValue(let value):
                guard let number = Double(value) else {
                    throw BamlDecodeError.typeMismatch(
                        expected: "JSON float",
                        got: "float literal \(value)"
                    )
                }
                return .float(number)
            case .boolValue(let value): return .bool(value)
            case nil:
                throw BamlDecodeError.typeMismatch(expected: "JSON literal", got: "empty literal")
            }
        case .listValue(let list):
            return .list(try list.items.map { try _bamlDecode(BamlOutboundValue($0)) })
        case .mapValue(let map):
            return .object(
                try Dictionary(uniqueKeysWithValues: map.entries.map {
                    ($0.key, try _bamlDecode(BamlOutboundValue($0.value)))
                })
            )
        default:
            throw BamlDecodeError.typeMismatch(expected: "JSON", got: wireArmName(raw))
        }
    }
}

/// Structural view returned by `BamlPrompt.messages()`.
public struct BamlPromptMessage: Sendable, Equatable, BamlDecodable {
    public let role: String
    public let content: String
    public let parts: [BamlPromptPart]
    public let metadata: [String: BamlPromptJSON]

    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlPromptMessage {
        let fields = try value.classFields()
        return BamlPromptMessage(
            role: try fields._baml("role"),
            content: try fields._baml("content"),
            parts: try fields._baml("parts"),
            metadata: try fields._baml("metadata")
        )
    }
}
