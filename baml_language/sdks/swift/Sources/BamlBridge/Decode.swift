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
    /// wrapper FIRST: generated unions use `selected_option_index` as
    /// the canonical arm identity, with the display name as a legacy fallback.
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

    /// Exact type selected by the outer union envelope. The wire index is
    /// resolved through the raw `self_type` options before it reaches a compact
    /// generated `BamlUnionN`: raw options may contain null holes or optional
    /// shells that are absent from the Swift union wrapper.
    public func unionSelectedType() throws -> BamlTypeDescriptor? {
        guard case .unionVariantValue(let variant) = raw.value,
              variant.hasSelectedOptionIndex
        else { return nil }
        guard variant.hasSelfType,
              case .union(let union)? = variant.selfType.ty
        else {
            throw BamlDecodeError.typeMismatch(
                expected: "union self_type",
                got: "indexed union without a union self_type"
            )
        }
        let selected = Int(variant.selectedOptionIndex)
        guard union.options.indices.contains(selected) else {
            throw BamlDecodeError.typeMismatch(
                expected: "union option in self_type",
                got: "union option index \(selected)"
            )
        }
        var selectedType = union.options[selected]
        while case .optional(let optional)? = selectedType.ty, optional.hasInner {
            selectedType = optional.inner
        }
        return BamlTypeDescriptor(selectedType)
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
///
/// A Swift type can be coarser than the BAML type it stands for (a
/// `"a"[] | "b"[]` is a `BamlUnion2<[String], [String]>`), so where the
/// generated code knows the declared type of a position it decodes through
/// that position's slot, as it encodes: a union then picks the case whose
/// declared arm the engine selected.
public protocol BamlDecodable {
    static func _bamlDecode(_ value: BamlOutboundValue) throws -> Self

    /// Decode `value` as a node of `slot`, the declared type of its
    /// position. Only a type whose children can be coarser than their
    /// declared types (a union, a container or class holding one) reads
    /// the slot; the rest decode as `_bamlDecode(_:)` does.
    static func _bamlDecode(_ value: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self

    /// Canonical BAML identity of this type when it appears as a union
    /// arm — matched against the wire's selected-arm name so union
    /// decode is metadata-driven, not guessed. `nil` opts into the
    /// structural fallback. A protocol REQUIREMENT (not just an
    /// extension member) so generic union decode dispatches to the
    /// concrete type's witness.
    static var _bamlArmIdentity: String? { get }

    /// Exact descriptor used to match an indexed outbound selection to this
    /// host arm. Static types that also conform to `BamlTyped` inherit their
    /// own exact type.
    static var _bamlDecodeType: BamlTypeDescriptor? { get }

    /// Whether an indexed outbound selection of type `selected` is this
    /// host arm. A requirement so generic union decode reaches a scalar's
    /// override.
    static func _bamlDecodeSelects(_ selected: BamlTypeDescriptor) -> Bool
}

extension BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue, as _: BamlSlot<Self>) throws -> Self {
        try _bamlDecode(value)
    }

    public static var _bamlArmIdentity: String? { nil }
    public static var _bamlDecodeType: BamlTypeDescriptor? {
        (Self.self as? any BamlTyped.Type)?._bamlStatedType
    }

    public static func _bamlDecodeSelects(_ selected: BamlTypeDescriptor) -> Bool {
        _bamlDecodeType == selected
    }
}

extension BamlSlot {
    /// Whether the engine's selected union member `selected` is this
    /// declared arm: the arm's own type, or one member of a merged leaf arm
    /// (`"a" | "b"`, one `String` case).
    public func _bamlSelects(_ selected: BamlTypeDescriptor) -> Bool {
        if type == selected { return true }
        guard case .leaf = node.shape, case .union(let merged)? = type.raw.ty else {
            return false
        }
        return merged.options.contains(selected.raw)
    }
}

/// A scalar arm also stands for the literals of its kind: the generator gives
/// a union one case for all its leaves of one Swift type (`"a" | "b" | int`
/// is `BamlUnion2<String, Int>`), so a selected `"b"` can only be the
/// `String` case.
private func selectsScalar(
    _ selected: BamlTypeDescriptor,
    _ own: BamlTypeDescriptor,
    literal matches: (BamlBridge_Cffi_V1_BamlTyLiteral.OneOf_Literal) -> Bool
) -> Bool {
    if selected == own { return true }
    guard case .literal(let literal)? = selected.raw.ty, let value = literal.literal else {
        return false
    }
    return matches(value)
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

/// How a nominal head under a type names its declaration.
enum BamlWireHead {
    /// A compiled declaration, by name.
    case name
    /// A runtime declaration: a key in the engine's handle table, owned like
    /// an engine handle (each occurrence the engine sends is one reference).
    case live(UInt64)
    /// Display text only, never an identity.
    case anonymousLabel
}

/// Visit every nominal head under `ty`, once per occurrence, through every
/// arm that nests a type.
func forEachHead(_ ty: BamlBridge_Cffi_V1_BamlTy, _ body: (BamlWireHead) -> Void) {
    func visit(_ ty: BamlBridge_Cffi_V1_BamlTy) {
        guard let arm = ty.ty else { return }
        switch arm {
        case .classTy(let cls):
            switch cls.head {
            case .name?: body(.name)
            case .live(let key)?: body(.live(key))
            case .anonymousLabel?: body(.anonymousLabel)
            case nil: break
            }
            cls.typeArgs.forEach(visit)
        case .enum(let enumType):
            switch enumType.head {
            case .name?: body(.name)
            case .live(let key)?: body(.live(key))
            case .anonymousLabel?: body(.anonymousLabel)
            case nil: break
            }
        case .typeAlias(let alias):
            switch alias.head {
            case .name?: body(.name)
            case .live(let key)?: body(.live(key))
            case .anonymousLabel?: body(.anonymousLabel)
            case nil: break
            }
            alias.typeArgs.forEach(visit)
        case .interface(let interface):
            switch interface.head {
            case .name?: body(.name)
            case .live(let key)?: body(.live(key))
            case .anonymousLabel?: body(.anonymousLabel)
            case nil: break
            }
            interface.typeArgs.forEach(visit)
            interface.bindings.forEach { visit($0.ty) }
        case .enumVariant(let variant):
            switch variant.head {
            case .name?: body(.name)
            case .live(let key)?: body(.live(key))
            case .anonymousLabel?: body(.anonymousLabel)
            case nil: break
            }
        case .list(let list): visit(list.item)
        case .map(let map): visit(map.key); visit(map.value)
        case .optional(let optional): visit(optional.inner)
        case .union(let union): union.options.forEach(visit)
        case .function(let function):
            function.params.forEach { visit($0.ty) }
            visit(function.ret)
            visit(function.throws)
        case .future(let future): visit(future.value); visit(future.error)
        case .associatedTypeProjection(let projection):
            visit(projection.base)
            visit(projection.interface)
        case .primitive, .literal, .unknown, .media, .rustType, .metaType,
            .resource, .promptAst, .typeVar, .never:
            break
        }
    }
    visit(ty)
}

/// The key of every `live` head under `ty`, once per occurrence. A live head
/// is a key in the engine's handle table, owned like an engine handle: each
/// occurrence the engine sends is one reference the host holds. The host
/// never resolves one; two are the same declaration to the host only when
/// their keys are equal.
func liveKeys(_ ty: BamlBridge_Cffi_V1_BamlTy) -> [UInt64] {
    var keys: [UInt64] = []
    forEachHead(ty) { head in
        switch head {
        case .live(let key): keys.append(key)
        case .name, .anonymousLabel: break
        }
    }
    return keys
}

/// Swift keeps no type value (no decodable accepts one), so a value arriving
/// from the engine gives back the references its type values hold at once.
func releaseTypeValues(_ value: BamlBridge_Cffi_V1_BamlOutboundValue) {
    switch value.value {
    case .tyValue(let ty): for key in liveKeys(ty) { _ = BamlApi.handleRelease(key) }
    case .listValue(let list): list.items.forEach(releaseTypeValues)
    case .mapValue(let map): for entry in map.entries { releaseTypeValues(entry.value) }
    case .classValue(let cls): for field in cls.fields { releaseTypeValues(field.value) }
    case .unionVariantValue(let variant): releaseTypeValues(variant.value)
    default: break
    }
}

extension Int: BamlDecodable {
    public static func _bamlDecodeSelects(_ selected: BamlTypeDescriptor) -> Bool {
        selectsScalar(selected, .int) { if case .intValue = $0 { true } else { false } }
    }

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

extension BamlBigInt: BamlDecodable {
    public static func _bamlDecodeSelects(_ selected: BamlTypeDescriptor) -> Bool {
        selectsScalar(selected, .bigint) { if case .bigintValue = $0 { true } else { false } }
    }

    public static var _bamlArmIdentity: String? { "bigint" }

    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlBigInt {
        let raw = value.normalized
        let hexadecimal: String
        switch raw.value {
        case .bigintValue(let bigint):
            hexadecimal = bigint
        case .literalValue(let literal):
            guard case .bigintValue(let bigint) = literal.literal else {
                throw BamlDecodeError.typeMismatch(expected: "BamlBigInt", got: wireArmName(raw))
            }
            hexadecimal = bigint
        default:
            throw BamlDecodeError.typeMismatch(expected: "BamlBigInt", got: wireArmName(raw))
        }
        do {
            return try BamlBigInt(hexadecimal: hexadecimal)
        } catch {
            throw BamlDecodeError.typeMismatch(expected: "canonical bigint", got: "bigint \(hexadecimal)")
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
    public static func _bamlDecodeSelects(_ selected: BamlTypeDescriptor) -> Bool {
        selectsScalar(selected, .bool) { if case .boolValue = $0 { true } else { false } }
    }

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
    public static func _bamlDecodeSelects(_ selected: BamlTypeDescriptor) -> Bool {
        selectsScalar(selected, .string) { if case .stringValue = $0 { true } else { false } }
    }

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
        try decode(value) { try Wrapped._bamlDecode($0) }
    }

    public static func _bamlDecode(_ value: BamlOutboundValue, as slot: BamlSlot<Wrapped?>) throws -> Wrapped? {
        guard case .optional(let wrapped) = slot.node.shape else { slot.node.mismatch("an optional") }
        return try decode(value) { try Wrapped._bamlDecode($0, as: BamlSlot(wrapped)) }
    }

    private static func decode(
        _ value: BamlOutboundValue,
        _ wrapped: (BamlOutboundValue) throws -> Wrapped
    ) throws -> Wrapped? {
        switch value.normalized.value {
        case nil, .nullValue: return nil
        // The value as it came, union wrapper and all: a union wrapped in
        // `Optional` selects its case from it.
        default: return try wrapped(value)
        }
    }
}

extension Array: BamlDecodable where Element: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> [Element] {
        try items(value).map { try Element._bamlDecode(BamlOutboundValue($0)) }
    }

    public static func _bamlDecode(_ value: BamlOutboundValue, as slot: BamlSlot<[Element]>) throws -> [Element] {
        guard case .list(let item) = slot.node.shape else { slot.node.mismatch("a list") }
        let itemSlot = BamlSlot<Element>(item)
        return try items(value).map { try Element._bamlDecode(BamlOutboundValue($0), as: itemSlot) }
    }

    private static func items(_ value: BamlOutboundValue) throws -> [BamlBridge_Cffi_V1_BamlOutboundValue] {
        let raw = value.normalized
        guard case .listValue(let list) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "Array", got: wireArmName(raw))
        }
        // `item_type` metadata is deliberately ignored, like Python.
        return list.items
    }
}

extension Dictionary: BamlDecodable where Key == String, Value: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> [String: Value] {
        try decode(value) { try Value._bamlDecode($0) }
    }

    public static func _bamlDecode(_ value: BamlOutboundValue, as slot: BamlSlot<[String: Value]>) throws -> [String: Value] {
        guard case .map(let valueNode) = slot.node.shape else { slot.node.mismatch("a map") }
        let valueSlot = BamlSlot<Value>(valueNode)
        return try decode(value) { try Value._bamlDecode($0, as: valueSlot) }
    }

    private static func decode(
        _ value: BamlOutboundValue,
        _ entry: (BamlOutboundValue) throws -> Value
    ) throws -> [String: Value] {
        let raw = value.normalized
        guard case .mapValue(let mapValue) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "Dictionary", got: wireArmName(raw))
        }
        var out: [String: Value] = [:]
        for item in mapValue.entries {
            out[item.key] = try entry(BamlOutboundValue(item.value))
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
        try T._bamlDecode(field(name))
    }

    /// Field `name`, decoded as a node of its declared type `slot`.
    public func _baml<T: BamlDecodable>(_ name: String, as slot: BamlSlot<T>) throws -> T {
        try T._bamlDecode(field(name), as: slot)
    }

    private func field(_ name: String) -> BamlOutboundValue {
        BamlOutboundValue(fields[name] ?? BamlBridge_Cffi_V1_BamlOutboundValue())
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
        releaseTypeValues(value)
        return BamlOutboundValue(value)
    case .error(let error):
        releaseTypeValues(error.value)
        // Same-process host-callable rehydration: a Swift error thrown
        // inside a passed-in callback comes back as the ORIGINAL error
        // object, not a BamlError wrapper (Python re-raises the exact
        // exception the same way).
        if let original = rehydrateHostError(error.value) {
            throw original
        }
        throw bamlError(from: error.value, trace: error.trace)
    case .panic(let panic):
        releaseTypeValues(panic.value)
        if panic.isExitPanic {
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

func bamlError(
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
