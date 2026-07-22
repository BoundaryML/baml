import CBamlBridge
import Foundation

/// Opaque wrapper around the outbound wire value (see
/// `BamlInboundValue` for the visibility rationale).
public struct BamlOutboundValue: Sendable {
    let raw: BamlBridge_Cffi_V1_BamlOutboundValue

    init(_ raw: BamlBridge_Cffi_V1_BamlOutboundValue) {
        self.raw = raw
    }

    /// Union wrappers unwrap for non-union decode paths (Python
    /// discards the metadata the same way). Union decode reads the
    /// wrapper FIRST via `unionSelectedArm()` — the metadata names the
    /// selected arm and is authoritative there.
    var normalized: BamlBridge_Cffi_V1_BamlOutboundValue {
        var current = raw
        while case .unionVariantValue(let variant) = current.value {
            current = variant.value
        }
        return current
    }

    /// The wire's name for the selected union arm (`value_option_name`
    /// on the outermost `union_variant_value` wrapper), or nil when the
    /// value arrived without a wrapper / with an empty name.
    public func unionSelectedArm() -> String? {
        guard case .unionVariantValue(let variant) = raw.value else { return nil }
        let name = variant.valueOptionName
        return name.isEmpty ? nil : name
    }

    /// The wire class FQN if this value's arm is a class, else nil.
    /// Used by union decode to pick class arms by identity.
    public func wireClassFQN() -> String? {
        if case .classValue(let cls) = normalized.value {
            return cls.name
        }
        return nil
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

    /// Canonical BAML identity of this type when it appears as a union
    /// arm — matched against the wire's selected-arm name so union
    /// decode is metadata-driven, not guessed. `nil` opts into the
    /// structural fallback. A protocol REQUIREMENT (not just an
    /// extension member) so generic union decode dispatches to the
    /// concrete type's witness.
    static var _bamlArmIdentity: String? { get }
}

extension BamlDecodable {
    public static var _bamlArmIdentity: String? { nil }
}

func wireArmName(_ v: BamlBridge_Cffi_V1_BamlOutboundValue) -> String {
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
    public static var _bamlArmIdentity: String? { "int" }

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
    public static var _bamlArmIdentity: String? { "float" }

    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Double {
        let raw = value.normalized
        if case .floatValue(let f) = raw.value { return f }
        throw BamlDecodeError.typeMismatch(expected: "Double", got: wireArmName(raw))
    }
}

extension Bool: BamlDecodable {
    public static var _bamlArmIdentity: String? { "bool" }

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
    public static var _bamlArmIdentity: String? { "string" }

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
    public static var _bamlArmIdentity: String? { "uint8array" }

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

/// Decoded class fields keyed by name — what a generated model's
/// `_bamlDecode` walks. Missing fields decode as null (so optional
/// properties come back `nil` and required ones throw `typeMismatch`).
public struct BamlClassFields: Sendable {
    let fields: [String: BamlBridge_Cffi_V1_BamlOutboundValue]

    public func _baml<T: BamlDecodable>(_ name: String) throws -> T {
        try T._bamlDecode(
            BamlOutboundValue(fields[name] ?? BamlBridge_Cffi_V1_BamlOutboundValue())
        )
    }
}

extension BamlOutboundValue {
    /// Interpret this value as a class and expose its fields. The wire
    /// FQN is deliberately not validated against the expected type —
    /// decoding is driven by the generated signature, mirroring
    /// Python's tolerance (its typemap lookup falls back rather than
    /// enforcing).
    public func classFields() throws -> BamlClassFields {
        let raw = normalized
        guard case .classValue(let cls) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "class", got: wireArmName(raw))
        }
        var out: [String: BamlBridge_Cffi_V1_BamlOutboundValue] = [:]
        for entry in cls.fields {
            out[entry.key] = entry.value
        }
        return BamlClassFields(fields: out)
    }

    /// Interpret this value as an enum and return its variant name.
    /// Generated enum conformances construct the member from it and
    /// throw if the variant is absent (Python raises the same way).
    public func enumVariant() throws -> String {
        let raw = normalized
        guard case .enumValue(let e) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "enum", got: wireArmName(raw))
        }
        return e.value
    }
}

extension BamlIndirect: BamlDecodable where Value: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlIndirect<Value> {
        BamlIndirect(wrappedValue: try Value._bamlDecode(value))
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
        // Same-process host-callable rehydration: a Swift error thrown
        // inside a passed-in callback comes back as the ORIGINAL error
        // object, not a BamlError wrapper (Python re-raises the exact
        // exception the same way).
        if let original = rehydrateHostError(error.value) {
            throw original
        }
        throw bamlError(from: error.value, trace: error.trace)
    case .panic(let panic):
        if panic.isExitPanic {
            // (The pre-V1 ABI had a flush_events() hook here; event
            // production was removed upstream, so exit directly.)
            exit(Int32(truncatingIfNeeded: panic.exitCode))
        }
        let (message, className) = describeThrownValue(panic.value)
        throw BamlPanic(
            message: message,
            className: className,
            bamlTrace: panic.trace,
            payload: BamlOutboundValue(panic.value)
        )
    }
}

private func bamlError(
    from value: BamlBridge_Cffi_V1_BamlOutboundValue,
    trace: [String]
) -> BamlError {
    let (message, className) = describeThrownValue(value)
    return BamlError(
        message: message,
        className: className,
        bamlTrace: trace,
        payload: BamlOutboundValue(value)
    )
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
