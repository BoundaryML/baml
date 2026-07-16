import Foundation

/// The BAML `null` type's Swift spelling. Swift has no untyped `nil`,
/// so standalone `null` params/returns (`function f(x: null) -> null`)
/// surface as this unit-like value. Encodes as an absent oneof;
/// decodes from the null arm.
public struct BamlNull: Equatable, Hashable, Sendable {
    public init() {}
}

/// Opaque wrapper around the inbound wire value so the protobuf types
/// stay internal to BamlBridge while generated code (a separate
/// module) can still conform its types to `BamlEncodable`.
public struct BamlInboundValue: Sendable {
    var raw: BamlBridge_Cffi_V1_InboundValue

    init(_ raw: BamlBridge_Cffi_V1_InboundValue = .init()) {
        self.raw = raw
    }
}

/// A value that can cross the boundary Swift → BAML. Mirrors the
/// shape-driven dispatch of Python's `_set_inbound_value`: encoding is
/// structural, carries no declared parameter types, and the engine
/// re-validates against the BAML signature after deserialization.
public protocol BamlEncodable {
    func _bamlEncode() -> BamlInboundValue
}

/// Constraint bundle for generic parameters of generated generic types
/// and functions (`class Wrapper<T>`, `function deep_copy<T>`): a `T`
/// must cross the boundary both ways and satisfy the struct
/// conformances. Type arguments are NOT sent on the wire — the engine
/// infers them from values (inbound inference is first-class; an
/// uninferable TypeVar is an engine-side error).
public typealias BamlCodableValue = BamlEncodable & BamlDecodable & Equatable & Sendable

extension Int: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        // Swift Int is 64-bit on all Apple targets, so it always fits
        // the wire's int64. (Arbitrary-precision bigint is a separate
        // BamlBigInt type, later phase.)
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.intValue = Int64(self)
        return BamlInboundValue(v)
    }
}

extension Double: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.floatValue = self
        return BamlInboundValue(v)
    }
}

extension Bool: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.boolValue = self
        return BamlInboundValue(v)
    }
}

extension String: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.stringValue = self
        return BamlInboundValue(v)
    }
}

extension Data: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.uint8ArrayValue = self
        return BamlInboundValue(v)
    }
}

extension BamlNull: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        // Absent oneof = BAML null.
        BamlInboundValue()
    }
}

extension Optional: BamlEncodable where Wrapped: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        switch self {
        case .none: return BamlInboundValue() // explicit null
        case .some(let wrapped): return wrapped._bamlEncode()
        }
    }
}

extension Array: BamlEncodable where Element: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        var list = BamlBridge_Cffi_V1_InboundListValue()
        list.values = map { $0._bamlEncode().raw }
        var v = BamlBridge_Cffi_V1_InboundValue()
        // Assigning the message sets the oneof case even when the list
        // is empty — the Swift equivalent of Python's `SetInParent()`,
        // so `[]` arrives as an empty list, not null.
        v.listValue = list
        return BamlInboundValue(v)
    }
}

extension Dictionary: BamlEncodable where Key == String, Value: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        var mapValue = BamlBridge_Cffi_V1_InboundMapValue()
        mapValue.entries = map { key, value in
            var entry = BamlBridge_Cffi_V1_InboundMapEntry()
            entry.stringKey = key
            entry.value = value._bamlEncode().raw
            return entry
        }
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.mapValue = mapValue // presence even when empty, as above
        return BamlInboundValue(v)
    }
}

extension BamlInboundValue {
    /// Build a class value for a generated model conformance. The FQN
    /// is baked into generated code (Python derives it via the reverse
    /// typemap; Swift types know their own). Fields encode
    /// shape-driven, `nil` as explicit null.
    public static func baml_class(
        _ fqn: String,
        _ fields: [(String, (any BamlEncodable)?)]
    ) -> BamlInboundValue {
        var cls = BamlBridge_Cffi_V1_InboundClassValue()
        cls.classTy.name = fqn
        cls.fields = fields.map { name, value in
            var entry = BamlBridge_Cffi_V1_InboundMapEntry()
            entry.stringKey = name
            entry.value = value?._bamlEncode().raw ?? BamlBridge_Cffi_V1_InboundValue()
            return entry
        }
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.classValue = cls
        return BamlInboundValue(v)
    }

    /// Build an enum value: `name` is the BAML enum FQN, `variant`
    /// the member's raw value.
    public static func baml_enum(_ fqn: String, _ variant: String) -> BamlInboundValue {
        var e = BamlBridge_Cffi_V1_InboundEnumValue()
        e.name = fqn
        e.value = variant
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.enumValue = e
        return BamlInboundValue(v)
    }
}

extension BamlIndirect: BamlEncodable where Value: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        wrappedValue._bamlEncode()
    }
}

/// Serialize one call's kwargs to `CallFunctionArgs` bytes.
/// `nil` in an argument slot encodes an explicit BAML null (the UNSET
/// omission sentinel is a later phase alongside optional args).
func encodeCallArgs(
    _ args: [(String, (any BamlEncodable)?)],
    callId: UInt64
) throws -> Data {
    precondition(callId != 0, "call_id must be nonzero")
    var msg = BamlBridge_Cffi_V1_CallFunctionArgs()
    msg.callID = callId
    msg.kwargs = args.map { name, value in
        var entry = BamlBridge_Cffi_V1_InboundMapEntry()
        entry.stringKey = name
        entry.value = value?._bamlEncode().raw ?? BamlBridge_Cffi_V1_InboundValue()
        return entry
    }
    return try msg.serializedData()
}
