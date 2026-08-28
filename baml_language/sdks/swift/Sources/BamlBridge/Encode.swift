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

/// Semantic projection of an authored BAML function. `direct` is raw value
/// zero so an omitted protobuf field preserves the historical call behavior.
public enum BamlFunctionOperation: Int, Sendable {
    case direct = 0
    case spec = 1
    case stream = 2

    var wire: BamlBridge_Cffi_V1_FunctionOperation {
        BamlBridge_Cffi_V1_FunctionOperation(rawValue: rawValue)!
    }
}

/// Public, protobuf-independent wrapper around an exact BAML type descriptor.
/// Generated/static Swift values expose this cheaply from their host type; the
/// encoder attaches it only at a selected union boundary or for nominal class
/// identity, without walking the runtime value to infer a type.
public struct BamlTypeDescriptor: @unchecked Sendable, Equatable {
    var raw: BamlBridge_Cffi_V1_BamlTy

    init(_ raw: BamlBridge_Cffi_V1_BamlTy) {
        self.raw = raw
    }

    public static func == (lhs: BamlTypeDescriptor, rhs: BamlTypeDescriptor) -> Bool {
        lhs.raw == rhs.raw
    }

    private static func primitive(
        _ kind: BamlBridge_Cffi_V1_BamlTyPrimitiveKind
    ) -> BamlTypeDescriptor {
        var primitive = BamlBridge_Cffi_V1_BamlTyPrimitive()
        primitive.kind = kind
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.primitive = primitive
        return BamlTypeDescriptor(ty)
    }

    public static var string: BamlTypeDescriptor { primitive(.bamlTyPrimitiveString) }
    public static var int: BamlTypeDescriptor { primitive(.bamlTyPrimitiveInt) }
    public static var float: BamlTypeDescriptor { primitive(.bamlTyPrimitiveFloat) }
    public static var bool: BamlTypeDescriptor { primitive(.bamlTyPrimitiveBool) }
    public static var null: BamlTypeDescriptor { primitive(.bamlTyPrimitiveNull) }
    public static var bytes: BamlTypeDescriptor { primitive(.bamlTyPrimitiveBytes) }

    public static var prompt: BamlTypeDescriptor {
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.promptAst = BamlBridge_Cffi_V1_BamlTyPromptAst()
        return BamlTypeDescriptor(ty)
    }

    public static func list(_ item: BamlTypeDescriptor) -> BamlTypeDescriptor {
        var list = BamlBridge_Cffi_V1_BamlTyList()
        list.item = item.raw
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.list = list
        return BamlTypeDescriptor(ty)
    }

    public static func map(
        key: BamlTypeDescriptor,
        value: BamlTypeDescriptor
    ) -> BamlTypeDescriptor {
        var map = BamlBridge_Cffi_V1_BamlTyMap()
        map.key = key.raw
        map.value = value.raw
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.map = map
        return BamlTypeDescriptor(ty)
    }

    public static func optional(_ inner: BamlTypeDescriptor) -> BamlTypeDescriptor {
        var optional = BamlBridge_Cffi_V1_BamlTyOptional()
        optional.inner = inner.raw
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.optional = optional
        return BamlTypeDescriptor(ty)
    }

    public static func union(_ options: [BamlTypeDescriptor]) -> BamlTypeDescriptor {
        var union = BamlBridge_Cffi_V1_BamlTyUnion()
        union.options = options.map(\.raw)
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.union = union
        return BamlTypeDescriptor(ty)
    }

    public static func classType(
        _ fqn: String,
        typeArguments: [BamlTypeDescriptor?] = []
    ) -> BamlTypeDescriptor {
        var cls = BamlBridge_Cffi_V1_BamlTyClass()
        cls.name = fqn
        // If even one generic argument is host-erased, retain nominal identity
        // and let the contextual BAML type refine the complete argument list.
        if typeArguments.allSatisfy({ $0 != nil }) {
            cls.typeArgs = typeArguments.compactMap { $0?.raw }
        }
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.classTy = cls
        return BamlTypeDescriptor(ty)
    }

    public static func enumType(_ fqn: String) -> BamlTypeDescriptor {
        var enumType = BamlBridge_Cffi_V1_BamlTyEnum()
        enumType.name = fqn
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.enum = enumType
        return BamlTypeDescriptor(ty)
    }
}

extension BamlInboundValue {
    /// Attach the exact selected-arm type while keeping the selected payload
    /// bare. Root union/optional descriptors are programmer errors: they name
    /// a set of choices rather than the choice this value represents.
    public func _bamlAnnotatingSelectedType(
        _ type: BamlTypeDescriptor?
    ) -> BamlInboundValue {
        guard let type else { return self }
        switch type.raw.ty {
        case .union?, .optional?:
            preconditionFailure("selected inbound value_type must not be a root union or optional")
        default:
            var annotated = raw
            annotated.valueType = type.raw
            return BamlInboundValue(annotated)
        }
    }
}

/// A value that can cross the boundary Swift → BAML. Mirrors the
/// shape-driven dispatch of Python's `_set_inbound_value`: encoding is
/// structural, carries no declared parameter types, and the engine
/// re-validates against the BAML signature after deserialization.
public protocol BamlEncodable {
    func _bamlEncode() -> BamlInboundValue

    /// Exact BAML type represented by this generated/static Swift type. `nil`
    /// means the host type is intentionally erased or runtime-owned.
    static var _bamlType: BamlTypeDescriptor? { get }
}

extension BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { nil }
}

/// Constraint bundle for generic parameters of generated generic types
/// and functions (`class Wrapper<T>`, `function deep_copy<T>`): a `T`
/// must cross the boundary both ways and satisfy the struct conformances.
/// Generated types expose their BAML descriptor so concrete generic class
/// arguments can travel in sparse nominal metadata; erased conformers still
/// fall back to engine-side contextual inference.
public typealias BamlCodableValue = BamlEncodable & BamlDecodable & Equatable & Sendable

extension Int: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { .int }

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
    public static var _bamlType: BamlTypeDescriptor? { .float }

    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.floatValue = self
        return BamlInboundValue(v)
    }
}

extension Bool: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { .bool }

    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.boolValue = self
        return BamlInboundValue(v)
    }
}

extension String: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { .string }

    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.stringValue = self
        return BamlInboundValue(v)
    }
}

extension Data: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { .bytes }

    public func _bamlEncode() -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.uint8ArrayValue = self
        return BamlInboundValue(v)
    }
}

extension BamlNull: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? { .null }

    public func _bamlEncode() -> BamlInboundValue {
        // Absent oneof = BAML null.
        BamlInboundValue()
    }
}

extension Optional: BamlEncodable where Wrapped: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? {
        Wrapped._bamlType.map(BamlTypeDescriptor.optional)
    }

    public func _bamlEncode() -> BamlInboundValue {
        switch self {
        case .none: return BamlInboundValue() // explicit null
        case .some(let wrapped): return wrapped._bamlEncode()
        }
    }
}

extension Array: BamlEncodable where Element: BamlEncodable {
    public static var _bamlType: BamlTypeDescriptor? {
        Element._bamlType.map(BamlTypeDescriptor.list)
    }

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
    public static var _bamlType: BamlTypeDescriptor? {
        Value._bamlType.map { .map(key: .string, value: $0) }
    }

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
        typeArguments: [BamlTypeDescriptor?] = [],
        _ fields: [(String, (any BamlEncodable)?)]
    ) -> BamlInboundValue {
        // Generated media classes contain only `_data: BamlHandle?`. The
        // handle encoder lowers that field to the canonical media payload;
        // flatten the wrapper too so media never crosses as class+handle
        // identity.
        if fields.count == 1, fields[0].0 == "_data", let data = fields[0].1 {
            let encoded = data._bamlEncode()
            if case .mediaValue? = encoded.raw.value {
                return encoded
            }
        }
        var cls = BamlBridge_Cffi_V1_InboundClassValue()
        cls.fields = fields.map { name, value in
            var entry = BamlBridge_Cffi_V1_InboundMapEntry()
            entry.stringKey = name
            entry.value = value?._bamlEncode().raw ?? BamlBridge_Cffi_V1_InboundValue()
            return entry
        }
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.valueType = BamlTypeDescriptor
            .classType(fqn, typeArguments: typeArguments)
            .raw
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
    public static var _bamlType: BamlTypeDescriptor? { Value._bamlType }

    public func _bamlEncode() -> BamlInboundValue {
        wrappedValue._bamlEncode()
    }
}

/// Serialize one call's kwargs to `CallFunctionArgs` bytes.
/// `nil` in an argument slot encodes an explicit BAML null (the UNSET
/// omission sentinel is a later phase alongside optional args).
func encodeCallArgs(
    _ args: [(String, (any BamlEncodable)?)],
    callId: UInt64,
    callTarget: BamlBridge_Cffi_V1_CallFunctionArgs.OneOf_CallTarget,
    operation: BamlFunctionOperation = .direct
) throws -> Data {
    precondition(callId != 0, "call_id must be nonzero")
    var msg = BamlBridge_Cffi_V1_CallFunctionArgs()
    msg.callID = callId
    msg.callTarget = callTarget
    msg.operation = operation.wire
    msg.kwargs = args.map { name, value in
        var entry = BamlBridge_Cffi_V1_InboundMapEntry()
        entry.stringKey = name
        entry.value = value?._bamlEncode().raw ?? BamlBridge_Cffi_V1_InboundValue()
        return entry
    }
    return try msg.serializedData()
}
