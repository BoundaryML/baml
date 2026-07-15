import CBamlBridge
import Foundation

/// Opaque wrapper around the outbound wire value (see
/// `BamlInboundValue` for the visibility rationale).
public struct BamlOutboundValue: Sendable {
    let raw: BamlBridge_Cffi_V1_BamlOutboundValue

    init(_ raw: BamlBridge_Cffi_V1_BamlOutboundValue) {
        self.raw = raw
    }

    /// Union wrappers never survive into host values (Python discards
    /// the metadata the same way; Swift's generated union enums will
    /// consume it in a later phase). Literal wrappers likewise unwrap
    /// to their base value.
    var normalized: BamlBridge_Cffi_V1_BamlOutboundValue {
        var current = raw
        while case .unionVariantValue(let variant) = current.value {
            current = variant.value
        }
        return current
    }
}

/// Decode-side failure: the wire value's shape didn't match what the
/// generated signature expected, or needs a capability that hasn't
/// landed yet.
public enum BamlDecodeError: Error, CustomStringConvertible {
    case typeMismatch(expected: String, got: String)
    case unsupported(String)

    public var description: String {
        switch self {
        case .typeMismatch(let expected, let got):
            return "BAML decode: expected \(expected), got wire value \(got)"
        case .unsupported(let what):
            return "BAML decode: \(what) is not supported yet"
        }
    }
}

/// A value that can cross the boundary BAML → Swift. Decoding is
/// wire-shape-driven, exactly like Python's `decode_value`: the
/// runtime never sees the expected return type; the generic parameter
/// on `BamlRuntime.call` picks the conformance.
public protocol BamlDecodable {
    static func _bamlDecode(_ value: BamlOutboundValue) throws -> Self
}

private func wireArmName(_ v: BamlBridge_Cffi_V1_BamlOutboundValue) -> String {
    guard let value = v.value else { return "null (absent oneof)" }
    switch value {
    case .nullValue: return "null"
    case .stringValue: return "string"
    case .intValue: return "int"
    case .floatValue: return "float"
    case .boolValue: return "bool"
    case .classValue(let c): return "class \(c.name)"
    case .enumValue(let e): return "enum \(e.name)"
    case .literalValue: return "literal"
    case .listValue: return "list"
    case .mapValue: return "map"
    case .unionVariantValue: return "union variant"
    case .handleValue: return "handle"
    case .mediaValue: return "media"
    case .promptAstValue: return "prompt ast"
    case .uint8ArrayValue: return "uint8array"
    case .bigintValue: return "bigint"
    case .tyValue: return "type reference"
    }
}

extension Int: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Int {
        let raw = value.normalized
        switch raw.value {
        case .intValue(let i):
            return Int(i)
        case .bigintValue(let hex):
            guard let parsed = parseHexBigintFittingInt(hex) else {
                throw BamlDecodeError.typeMismatch(expected: "Int", got: "bigint \(hex)")
            }
            return parsed
        case .literalValue(let lit):
            if case .intValue(let i) = lit.literal { return Int(i) }
            throw BamlDecodeError.typeMismatch(expected: "Int", got: wireArmName(raw))
        default:
            throw BamlDecodeError.typeMismatch(expected: "Int", got: wireArmName(raw))
        }
    }
}

extension Double: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Double {
        let raw = value.normalized
        if case .floatValue(let f) = raw.value { return f }
        throw BamlDecodeError.typeMismatch(expected: "Double", got: wireArmName(raw))
    }
}

extension Bool: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Bool {
        let raw = value.normalized
        switch raw.value {
        case .boolValue(let b): return b
        case .literalValue(let lit):
            if case .boolValue(let b) = lit.literal { return b }
            throw BamlDecodeError.typeMismatch(expected: "Bool", got: wireArmName(raw))
        default:
            throw BamlDecodeError.typeMismatch(expected: "Bool", got: wireArmName(raw))
        }
    }
}

extension String: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> String {
        let raw = value.normalized
        switch raw.value {
        case .stringValue(let s): return s
        case .literalValue(let lit):
            if case .stringValue(let s) = lit.literal { return s }
            throw BamlDecodeError.typeMismatch(expected: "String", got: wireArmName(raw))
        default:
            throw BamlDecodeError.typeMismatch(expected: "String", got: wireArmName(raw))
        }
    }
}

extension Data: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Data {
        let raw = value.normalized
        if case .uint8ArrayValue(let d) = raw.value { return d }
        throw BamlDecodeError.typeMismatch(expected: "Data", got: wireArmName(raw))
    }
}

extension BamlNull: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlNull {
        let raw = value.normalized
        switch raw.value {
        case nil, .nullValue: return BamlNull()
        default:
            throw BamlDecodeError.typeMismatch(expected: "BamlNull", got: wireArmName(raw))
        }
    }
}

extension Optional: BamlDecodable where Wrapped: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Wrapped? {
        let raw = value.normalized
        switch raw.value {
        case nil, .nullValue: return nil
        default: return try Wrapped._bamlDecode(BamlOutboundValue(raw))
        }
    }
}

extension Array: BamlDecodable where Element: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> [Element] {
        let raw = value.normalized
        guard case .listValue(let list) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "Array", got: wireArmName(raw))
        }
        // `item_type` metadata is deliberately ignored, like Python.
        return try list.items.map { try Element._bamlDecode(BamlOutboundValue($0)) }
    }
}

extension Dictionary: BamlDecodable where Key == String, Value: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> [String: Value] {
        let raw = value.normalized
        guard case .mapValue(let mapValue) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "Dictionary", got: wireArmName(raw))
        }
        var out: [String: Value] = [:]
        for entry in mapValue.entries {
            out[entry.key] = try Value._bamlDecode(BamlOutboundValue(entry.value))
        }
        return out
    }
}

/// Strict lowercase-hex bigint parse (sign-prefixed), succeeding only
/// when the value fits Swift's 64-bit `Int`.
private func parseHexBigintFittingInt(_ hex: String) -> Int? {
    let negative = hex.hasPrefix("-")
    let digits = negative ? String(hex.dropFirst()) : hex
    guard let magnitude = UInt64(digits, radix: 16) else { return nil }
    if negative {
        guard magnitude <= UInt64(Int.max) + 1 else { return nil }
        return magnitude == UInt64(Int.max) + 1 ? Int.min : -Int(magnitude)
    }
    guard magnitude <= UInt64(Int.max) else { return nil }
    return Int(magnitude)
}

// MARK: - Result envelope

/// Decode a `BamlOutboundResult` envelope: return the ok value or
/// throw the error/panic arm. Mirrors Python's `decode_call_result`
/// (the TypeMismatch → native-TypeError special case and host-callable
/// rehydration arrive with the error phase).
func unwrapEnvelope(_ data: Data) throws -> BamlOutboundValue {
    let envelope = try BamlBridge_Cffi_V1_BamlOutboundResult(serializedBytes: data)
    switch envelope.result {
    case nil:
        // Absent oneof decodes as the default ok value (= null), same
        // as Python.
        return BamlOutboundValue(BamlBridge_Cffi_V1_BamlOutboundValue())
    case .ok(let value):
        return BamlOutboundValue(value)
    case .error(let error):
        throw bamlError(from: error.value, trace: error.trace)
    case .panic(let panic):
        if panic.isExitPanic {
            flush_events()
            exit(Int32(truncatingIfNeeded: panic.exitCode))
        }
        let (message, className) = describeThrownValue(panic.value)
        throw BamlPanic(message: message, className: className, bamlTrace: panic.trace)
    }
}

private func bamlError(
    from value: BamlBridge_Cffi_V1_BamlOutboundValue,
    trace: [String]
) -> BamlError {
    let (message, className) = describeThrownValue(value)
    return BamlError(message: message, className: className, bamlTrace: trace)
}

/// Best-effort human-readable rendering of a thrown value plus its
/// class FQN. Typed thrown-value decoding (into generated error
/// models) is a later phase.
private func describeThrownValue(
    _ value: BamlBridge_Cffi_V1_BamlOutboundValue
) -> (message: String, className: String?) {
    var current = value
    while case .unionVariantValue(let variant) = current.value {
        current = variant.value
    }
    switch current.value {
    case .classValue(let cls):
        let fields = cls.fields
            .map { entry in "\(entry.key): \(scalarPreview(entry.value))" }
            .joined(separator: ", ")
        return ("\(cls.name) { \(fields) }", cls.name)
    case .stringValue(let s):
        return (s, nil)
    default:
        return (wireArmName(current), nil)
    }
}

private func scalarPreview(_ value: BamlBridge_Cffi_V1_BamlOutboundValue) -> String {
    switch value.value {
    case .stringValue(let s): return "\"\(s)\""
    case .intValue(let i): return String(i)
    case .floatValue(let f): return String(f)
    case .boolValue(let b): return String(b)
    case nil, .nullValue: return "null"
    default: return "<\(wireArmName(value))>"
    }
}
