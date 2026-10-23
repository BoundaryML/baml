import CBamlBridge
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
///
/// The message is boxed: the encoder recurses once per nesting level and
/// returns one of these from every frame, and an inline message (over 200
/// bytes) would be reserved in each of them, several times over in an
/// unoptimized build, on a thread that may have only 512 KiB of stack.
public struct BamlInboundValue: Sendable {
    private final class Box: Sendable {
        let raw: BamlBridge_Cffi_V1_InboundValue

        init(_ raw: BamlBridge_Cffi_V1_InboundValue) {
            self.raw = raw
        }
    }

    private let box: Box

    var raw: BamlBridge_Cffi_V1_InboundValue { box.raw }

    init(_ raw: BamlBridge_Cffi_V1_InboundValue = .init()) {
        box = Box(raw)
    }
}

/// Public, protobuf-independent wrapper around an exact BAML type descriptor:
/// what a list, map, class or callable node states as its own type.
public struct BamlTypeDescriptor: @unchecked Sendable, Equatable {
    var raw: BamlBridge_Cffi_V1_BamlTy

    init(_ raw: BamlBridge_Cffi_V1_BamlTy) {
        self.raw = raw
    }

    /// Wire equality: a `live` head equals another only when their keys are
    /// equal.
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
    public static var bigint: BamlTypeDescriptor { primitive(.bamlTyPrimitiveBigint) }
    public static var float: BamlTypeDescriptor { primitive(.bamlTyPrimitiveFloat) }
    public static var bool: BamlTypeDescriptor { primitive(.bamlTyPrimitiveBool) }
    public static var null: BamlTypeDescriptor { primitive(.bamlTyPrimitiveNull) }
    public static var bytes: BamlTypeDescriptor { primitive(.bamlTyPrimitiveBytes) }

    /// The top type: every value is a member.
    public static var unknown: BamlTypeDescriptor {
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.unknown = BamlBridge_Cffi_V1_BamlTyUnknown()
        return BamlTypeDescriptor(ty)
    }

    /// The empty type: the throws type of a callable that cannot fail.
    public static var never: BamlTypeDescriptor {
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.never = BamlBridge_Cffi_V1_BamlTyNever()
        return BamlTypeDescriptor(ty)
    }

    /// The type of a callable: its parameters, return type and throws type. A
    /// host callable has no type of its own, so the generated code states the
    /// one its slot declares beside the callable's handle.
    public static func function(
        params: [BamlFunctionParamDescriptor],
        returns: BamlTypeDescriptor,
        throws throwsType: BamlTypeDescriptor
    ) -> BamlTypeDescriptor {
        var function = BamlBridge_Cffi_V1_BamlTyFunction()
        function.params = params.map { param in
            var wire = BamlBridge_Cffi_V1_BamlTyFunctionParam()
            if let name = param.name {
                wire.name = name
            }
            wire.ty = param.type.raw
            wire.mode = param.optional ? .optional : .required
            return wire
        }
        function.ret = returns.raw
        function.throws = throwsType.raw
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.function = function
        return BamlTypeDescriptor(ty)
    }

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

    /// A class with every type argument: the engine refines no statement
    /// from context, so a generic class without its arguments is refused.
    public static func classType(
        _ fqn: String,
        typeArguments: [BamlTypeDescriptor] = []
    ) -> BamlTypeDescriptor {
        var cls = BamlBridge_Cffi_V1_BamlTyClass()
        cls.name = fqn
        cls.typeArgs = typeArguments.map(\.raw)
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

    /// A recursive alias, by name: the one kind of alias that exists at run
    /// time (a non-recursive alias is spelled by its body).
    public static func typeAlias(_ fqn: String) -> BamlTypeDescriptor {
        var alias = BamlBridge_Cffi_V1_BamlTyTypeAlias()
        alias.name = fqn
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.typeAlias = alias
        return BamlTypeDescriptor(ty)
    }

    /// The media type `image`, `audio`, `video` or `pdf`. Never the stdlib
    /// class that wraps it host-side.
    public static func media(_ kind: BamlMedia.Kind) -> BamlTypeDescriptor {
        var media = BamlBridge_Cffi_V1_BamlTyMedia()
        // `BamlMedia.Kind` carries the C ABI numbering, which orders `pdf`
        // and `video` differently from the type descriptor's.
        switch kind {
        case .image: media.kind = .image
        case .audio: media.kind = .audio
        case .video: media.kind = .video
        case .pdf: media.kind = .pdf
        case .generic: media.kind = .generic
        }
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.media = media
        return BamlTypeDescriptor(ty)
    }

    /// Opaque engine-owned state: a `$rust_type` field.
    public static var rustType: BamlTypeDescriptor {
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.rustType = BamlBridge_Cffi_V1_BamlTyRustType()
        return BamlTypeDescriptor(ty)
    }

    private static func literal(
        _ value: BamlBridge_Cffi_V1_BamlTyLiteral.OneOf_Literal
    ) -> BamlTypeDescriptor {
        var literal = BamlBridge_Cffi_V1_BamlTyLiteral()
        literal.literal = value
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.literal = literal
        return BamlTypeDescriptor(ty)
    }

    public static func literal(_ value: String) -> BamlTypeDescriptor {
        literal(.stringValue(value))
    }

    public static func literal(_ value: Int) -> BamlTypeDescriptor {
        literal(.intValue(Int64(value)))
    }

    public static func literal(_ value: Bool) -> BamlTypeDescriptor {
        literal(.boolValue(value))
    }

    /// A bigint literal, in base ten.
    public static func literal(bigint decimal: String) -> BamlTypeDescriptor {
        literal(.bigintValue(decimal))
    }
}

/// One parameter of a function type: an optional (by-name) parameter carries
/// its wire name, a positional one has none.
public struct BamlFunctionParamDescriptor: Sendable {
    public let name: String?
    public let type: BamlTypeDescriptor
    public let optional: Bool

    public init(name: String? = nil, type: BamlTypeDescriptor, optional: Bool = false) {
        self.name = name
        self.type = type
        self.optional = optional
    }
}

/// The declared BAML type of one value position, shaped like the Swift type
/// `Value` that fills it.
///
/// A Swift type can be coarser than the BAML type it stands for: a
/// `"draft" | "sent"` is a `String`, and so is a `Box<"a">`'s field. The
/// engine converts nothing and refines no statement from context, so a list,
/// map or class must state the type its position declares, never one read
/// back off the Swift type. Generated code builds a slot from each declared
/// type; a generic parameter contributes its Swift type's own
/// [`BamlTyped._bamlSlot`]. The phantom `Value` keeps a slot and the value it
/// encodes in step: an `Array` only ever receives a list slot, a `BamlUnionN`
/// only a slot with one arm per case.
public struct BamlSlot<Value>: @unchecked Sendable {
    let node: BamlSlotNode

    init(_ node: BamlSlotNode) {
        self.node = node
    }

    /// The exact BAML type of this position.
    public var type: BamlTypeDescriptor { node.type }

}

/// The untyped content of a [`BamlSlot`]: the position's exact type and what
/// a value encoding there needs for its children.
struct BamlSlotNode: @unchecked Sendable {
    let type: BamlTypeDescriptor
    let shape: Shape

    indirect enum Shape {
        /// The value's node implies its type, or (a recursive alias) the
        /// value carries its own arms: nothing below comes from this slot.
        case leaf
        case list(item: BamlSlotNode)
        case map(value: BamlSlotNode)
        case classType(arguments: [BamlSlotNode])
        /// A Swift `Optional`: `wrapped` is the slot of the non-null value.
        case optional(wrapped: BamlSlotNode)
        /// A `BamlUnionN`: one arm per case, in case order.
        case union(arms: [BamlSlotNode])
    }

    /// The single shape-mismatch exit. Slots are built only through typed
    /// constructors, so reaching this is a bug in the bridge or the
    /// generator, never a property of a user's value.
    func mismatch(_ expected: String) -> Never {
        preconditionFailure("BamlSlot for \(expected) has shape \(shape)")
    }
}

/// Types whose value needs nothing from its slot but the slot's type: every
/// node whose arm implies its type (scalars, enums, media, prompts, handles)
/// and the recursive-alias enums, whose arms their own declaration fixes.
public protocol BamlLeafEncodable: BamlEncodable {}

extension BamlSlot where Value: BamlLeafEncodable {
    /// The slot of a leaf value of BAML type `type`.
    public static func _leaf(_ type: BamlTypeDescriptor) -> BamlSlot<Value> {
        BamlSlot(BamlSlotNode(type: type, shape: .leaf))
    }

    /// Several BAML types one Swift type stands for (`"draft" | "sent"` is a
    /// `String`): a leaf states nothing, so its slot only has to spell the
    /// union for an enclosing statement.
    public static func oneOf(_ members: BamlSlot<Value>...) -> BamlSlot<Value> {
        precondition(!members.isEmpty, "BamlSlot.oneOf needs at least one member")
        return _leaf(BamlSlots.unionType(members.map(\.node)))
    }
}

extension BamlSlot where Value == String {
    public static func literal(_ value: String) -> BamlSlot<String> { _leaf(.literal(value)) }
}

extension BamlSlot where Value == Int {
    public static func literal(_ value: Int) -> BamlSlot<Int> { _leaf(.literal(value)) }
}

extension BamlSlot where Value == Bool {
    public static func literal(_ value: Bool) -> BamlSlot<Bool> { _leaf(.literal(value)) }
}

extension BamlSlot where Value == BamlBigInt {
    /// A bigint literal, in base ten.
    public static func literal(bigint decimal: String) -> BamlSlot<BamlBigInt> {
        _leaf(.literal(bigint: decimal))
    }
}

extension BamlSlot {
    /// The slot of a generated class instance: the class with every type
    /// argument, each given as the slot its fields are encoded under.
    public static func _classType<each Argument>(
        _ fqn: String,
        _ arguments: repeat BamlSlot<each Argument>
    ) -> BamlSlot<Value> {
        var nodes: [BamlSlotNode] = []
        for argument in repeat each arguments {
            nodes.append(argument.node)
        }
        return BamlSlot(BamlSlotNode(
            type: .classType(fqn, typeArguments: nodes.map(\.type)),
            shape: .classType(arguments: nodes)
        ))
    }

    /// Type argument `index` of a class slot, as the slot of the class's
    /// generic parameter at that position.
    public func _classArgument<Argument>(
        _ index: Int,
        as _: Argument.Type = Argument.self
    ) -> BamlSlot<Argument> {
        guard case .classType(let arguments) = node.shape, arguments.indices.contains(index) else {
            node.mismatch("type argument \(index) of a class")
        }
        return BamlSlot<Argument>(arguments[index])
    }
}

/// Constructors of the structural slots. Each result's Swift type follows
/// from its arguments, so generated slot expressions need no annotations.
public enum BamlSlots {
    public static func list<Element>(_ item: BamlSlot<Element>) -> BamlSlot<[Element]> {
        BamlSlot(BamlSlotNode(type: .list(item.type), shape: .list(item: item.node)))
    }

    public static func map<Value>(_ value: BamlSlot<Value>) -> BamlSlot<[String: Value]> {
        BamlSlot(BamlSlotNode(
            type: .map(key: .string, value: value.type),
            shape: .map(value: value.node)
        ))
    }

    /// `Wrapped | null`, filled by a Swift `Optional`.
    public static func optional<Wrapped>(_ wrapped: BamlSlot<Wrapped>) -> BamlSlot<Wrapped?> {
        BamlSlot(BamlSlotNode(
            type: .optional(wrapped.type),
            shape: .optional(wrapped: wrapped.node)
        ))
    }

    /// A type that already admits null (the stdlib `json` alias), filled by a
    /// Swift `Optional` whose `nil` is that null: the type stays as it is.
    public static func nullable<Wrapped>(_ wrapped: BamlSlot<Wrapped>) -> BamlSlot<Wrapped?> {
        BamlSlot(BamlSlotNode(type: wrapped.type, shape: .optional(wrapped: wrapped.node)))
    }

    /// The type of a union slot. A merged leaf arm (`"a" | "b"`) is itself a
    /// union; its members join the enclosing one rather than nest in it.
    static func unionType(_ arms: [BamlSlotNode]) -> BamlTypeDescriptor {
        var options: [BamlBridge_Cffi_V1_BamlTy] = []
        for arm in arms {
            if case .union(let nested)? = arm.type.raw.ty {
                options.append(contentsOf: nested.options)
            } else {
                options.append(arm.type.raw)
            }
        }
        var union = BamlBridge_Cffi_V1_BamlTyUnion()
        union.options = options
        var ty = BamlBridge_Cffi_V1_BamlTy()
        ty.union = union
        return BamlTypeDescriptor(ty)
    }

    /// The slot of a `BamlUnionN`: one arm per case, in case order.
    static func union<Value>(_ arms: [BamlSlotNode]) -> BamlSlot<Value> {
        BamlSlot(BamlSlotNode(type: unionType(arms), shape: .union(arms: arms)))
    }
}

extension BamlSlot {
    /// Arm `index` of a union slot, as the slot of the case's payload.
    func unionArm<Arm>(_ index: Int, as _: Arm.Type = Arm.self) -> BamlSlot<Arm> {
        guard case .union(let arms) = node.shape, arms.indices.contains(index) else {
            node.mismatch("arm \(index) of a union")
        }
        return BamlSlot<Arm>(arms[index])
    }
}

/// A value that can cross the boundary Swift → BAML. A value encodes as a
/// node of the slot it fills: scalars by their arm, containers, classes and
/// callables with the slot's exact type stated beside them.
///
/// Encoding throws when the value cannot cross: one nested deeper than
/// [`BamlValueNesting.maxDepth`] is refused before the encoder descends any
/// further. A leaf never throws, and may say so in its own witness.
public protocol BamlEncodable {
    func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue
}

/// How deeply the lists, maps and classes of one value may nest.
///
/// The engine decodes at most [`wireNestingLimit`] nested protobuf messages
/// (`WIRE_NESTING_LIMIT` in `bridge_ctypes::wire_depth`). Every list, map or
/// class costs at least two of them (its own message and each item's
/// `InboundValue`) and states its type in one more, so a value nested more
/// than half the limit deep can never cross. The encoder recurses once per
/// level, on whatever thread encodes (a cooperative thread has 512 KiB of
/// stack), so it refuses such a value at the level that exceeds the bound
/// instead of descending further.
///
/// Only a list, map or class adds a wire level, and only the three encoders
/// here build one (`BamlInboundValue` is opaque outside this module), so no
/// other encoder can nest without being counted. Encoding is synchronous, so
/// one value's levels are all entered and left on one thread: the count is
/// per thread, and costs no frame of its own on the recursion.
enum BamlValueNesting {
    /// The engine's `WIRE_NESTING_LIMIT`.
    static let wireNestingLimit = 100
    /// The deepest a value's lists, maps and classes may nest.
    static let maxDepth = wireNestingLimit / 2

    private static let depthKey: pthread_key_t = {
        var key = pthread_key_t()
        let status = pthread_key_create(&key, nil)
        precondition(status == 0, "pthread_key_create failed with status \(status)")
        return key
    }()

    /// How many lists, maps and classes enclose the node being encoded on
    /// this thread (stored as the slot's bit pattern: no allocation).
    private static var depth: Int {
        get { Int(bitPattern: pthread_getspecific(depthKey)) }
        set { pthread_setspecific(depthKey, UnsafeRawPointer(bitPattern: newValue)) }
    }

    /// Enter one more list, map or class, or refuse it with the error the
    /// engine raises for an argument it cannot read. Every successful
    /// `enter()` is paired with a `leave()` (a `defer`).
    static func enter() throws {
        let level = depth + 1
        guard level <= maxDepth else {
            throw BamlError.invalidArgument(
                "the value nests lists, maps and classes more than \(maxDepth) deep, deeper than "
                    + "the boundary's \(wireNestingLimit) nested wire messages can carry"
            )
        }
        depth = level
    }

    static func leave() {
        assert(depth > 0, "BamlValueNesting.leave() without a matching enter()")
        depth -= 1
    }
}

extension BamlBridge_Cffi_V1_InboundMapEntry {
    /// A string-keyed entry: a map entry or a class field.
    init(_ entry: (key: String, value: BamlInboundValue)) {
        self.init()
        stringKey = entry.key
        value = entry.value.raw
    }
}

/// Release every engine handle clone `value` holds. Host values (callables
/// and opaque errors) are this host's own registry keys, not engine handles.
func releaseInbound(_ value: BamlBridge_Cffi_V1_InboundValue) {
    switch value.value {
    case .handle(let handle):
        if handle.handleType != .hostValueCallable && handle.handleType != .hostValueOpaque {
            _ = BamlApi.handleRelease(handle.key)
        }
    case .classValue(let cls): for field in cls.fields { releaseInbound(field.value) }
    case .listValue(let list): for item in list.values { releaseInbound(item) }
    case .mapValue(let map): for entry in map.entries { releaseInbound(entry.value) }
    case nil, .stringValue, .intValue, .floatValue, .boolValue, .enumValue, .uint8ArrayValue,
        .bigintValue, .tyValue, .mediaValue, .promptAstValue, .jsNumberValue:
        break
    }
}

extension BamlSlot where Value: BamlEncodable {
    /// `value` as a node of this slot, through `Value`'s conformance: a
    /// caller that knows `Value` concretely calls a leaf, which is never
    /// refused, and a container, which may be, alike.
    public func _bamlEncode(_ value: Value) throws -> BamlInboundValue {
        try value._bamlEncode(as: self)
    }
}

/// A Swift type that stands for one exact BAML type of its own. This is the
/// slot a generic parameter fills (`Box<T>` states `Box<int>` for
/// `Box<Swift.Int>`), so a generic class or function can take only these:
/// a type argument the host cannot state is a compile error, not a refusal.
public protocol BamlTyped: BamlEncodable {
    static var _bamlSlot: BamlSlot<Self> { get }

    /// The slot of `Self?`. `Self | null`, except where `Self`'s own type
    /// already admits null (the stdlib `json` alias).
    static var _bamlOptionalSlot: BamlSlot<Self?> { get }
}

extension BamlSlot where Value: BamlTyped {
    /// `Value`'s own slot. Generated code spells it here, with the type in a
    /// type position, because a type named `Type` cannot be followed by a
    /// member in an expression.
    public static var own: BamlSlot<Value> { Value._bamlSlot }
}

extension BamlTyped {
    public static var _bamlOptionalSlot: BamlSlot<Self?> { BamlSlots.optional(_bamlSlot) }

    /// The exact BAML type of this Swift type.
    public static var _bamlStatedType: BamlTypeDescriptor { _bamlSlot.type }

    /// Encode under this Swift type's own slot.
    public func _bamlEncode() throws -> BamlInboundValue {
        try _bamlEncode(as: Self._bamlSlot)
    }
}

/// Constraint bundle for generic parameters of generated generic types
/// and functions (`class Wrapper<T>`, `function deep_copy<T>`): a `T`
/// must cross the boundary both ways, state its own exact type, and
/// satisfy the struct conformances.
public typealias BamlCodableValue = BamlTyped & BamlDecodable & Equatable & Sendable

extension Int: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<Int> { ._leaf(.int) }

    public func _bamlEncode(as _: BamlSlot<Int>) -> BamlInboundValue {
        // Swift Int is 64-bit on all Apple targets, so it always fits
        // the wire's int64. Arbitrary-precision bigint uses BamlBigInt.
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.intValue = Int64(self)
        return BamlInboundValue(v)
    }
}

extension BamlBigInt: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<BamlBigInt> { ._leaf(.bigint) }

    public func _bamlEncode(as _: BamlSlot<BamlBigInt>) -> BamlInboundValue {
        var value = BamlBridge_Cffi_V1_InboundValue()
        value.bigintValue = hexadecimal
        return BamlInboundValue(value)
    }
}

extension Double: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<Double> { ._leaf(.float) }

    public func _bamlEncode(as _: BamlSlot<Double>) -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.floatValue = self
        return BamlInboundValue(v)
    }
}

extension Bool: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<Bool> { ._leaf(.bool) }

    public func _bamlEncode(as _: BamlSlot<Bool>) -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.boolValue = self
        return BamlInboundValue(v)
    }
}

extension String: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<String> { ._leaf(.string) }

    public func _bamlEncode(as _: BamlSlot<String>) -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.stringValue = self
        return BamlInboundValue(v)
    }
}

extension Data: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<Data> { ._leaf(.bytes) }

    public func _bamlEncode(as _: BamlSlot<Data>) -> BamlInboundValue {
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.uint8ArrayValue = self
        return BamlInboundValue(v)
    }
}

extension BamlNull: BamlTyped, BamlLeafEncodable {
    public static var _bamlSlot: BamlSlot<BamlNull> { ._leaf(.null) }

    public func _bamlEncode(as _: BamlSlot<BamlNull>) -> BamlInboundValue {
        // Absent oneof = BAML null.
        BamlInboundValue()
    }
}

extension Optional: BamlEncodable where Wrapped: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<Wrapped?>) throws -> BamlInboundValue {
        guard case .optional(let wrapped) = slot.node.shape else {
            slot.node.mismatch("an optional")
        }
        guard let value = self else { return BamlInboundValue() } // explicit null
        return try value._bamlEncode(as: BamlSlot(wrapped))
    }
}

extension Optional: BamlTyped where Wrapped: BamlTyped {
    public static var _bamlSlot: BamlSlot<Wrapped?> { Wrapped._bamlOptionalSlot }
}

extension Array: BamlEncodable where Element: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<[Element]>) throws -> BamlInboundValue {
        guard case .list(let item) = slot.node.shape else { slot.node.mismatch("a list") }
        let itemSlot = BamlSlot<Element>(item)
        try BamlValueNesting.enter()
        defer { BamlValueNesting.leave() }
        // The descent keeps no wire message in this frame: the list is
        // assembled once every item is encoded.
        var items: [BamlInboundValue] = []
        items.reserveCapacity(count)
        do {
            for element in self {
                items.append(try element._bamlEncode(as: itemSlot))
            }
        } catch {
            for item in items { releaseInbound(item.raw) }
            throw error
        }
        return .listNode(items, stating: slot.type)
    }
}

extension Array: BamlTyped where Element: BamlTyped {
    public static var _bamlSlot: BamlSlot<[Element]> { BamlSlots.list(Element._bamlSlot) }
}

extension Dictionary: BamlEncodable where Key == String, Value: BamlEncodable {
    public func _bamlEncode(as slot: BamlSlot<[String: Value]>) throws -> BamlInboundValue {
        guard case .map(let value) = slot.node.shape else { slot.node.mismatch("a map") }
        let valueSlot = BamlSlot<Value>(value)
        try BamlValueNesting.enter()
        defer { BamlValueNesting.leave() }
        var entries: [(key: String, value: BamlInboundValue)] = []
        entries.reserveCapacity(count)
        do {
            for (key, value) in self {
                entries.append((key, try value._bamlEncode(as: valueSlot)))
            }
        } catch {
            for entry in entries { releaseInbound(entry.value.raw) }
            throw error
        }
        return .mapNode(entries, stating: slot.type)
    }
}

extension Dictionary: BamlTyped where Key == String, Value: BamlTyped {
    public static var _bamlSlot: BamlSlot<[String: Value]> { BamlSlots.map(Value._bamlSlot) }
}

/// One named value of a class or a call, paired with the slot it fills.
public struct BamlField {
    let name: String
    let encode: () throws -> BamlInboundValue

    public init<Value: BamlEncodable>(_ name: String, _ value: Value, as slot: BamlSlot<Value>) {
        self.name = name
        self.encode = { try value._bamlEncode(as: slot) }
    }

    /// A prebuilt node, for runtime-owned values with no Swift model.
    init(_ name: String, encoded: BamlInboundValue) {
        self.name = name
        self.encode = { encoded }
    }
}

extension BamlInboundValue {
    /// Build a class value for a generated model conformance, stating the
    /// class with every type argument its slot carries.
    public static func baml_class(
        _ slot: BamlSlot<some BamlEncodable>,
        _ fields: [BamlField]
    ) throws -> BamlInboundValue {
        guard case .classType = slot.node.shape else { slot.node.mismatch("a class") }
        return try classValue(stating: slot.type, fields)
    }

    static func classValue(stating type: BamlTypeDescriptor, _ fields: [BamlField]) throws -> BamlInboundValue {
        try BamlValueNesting.enter()
        defer { BamlValueNesting.leave() }
        var entries: [(key: String, value: BamlInboundValue)] = []
        entries.reserveCapacity(fields.count)
        do {
            for field in fields {
                entries.append((field.name, try field.encode()))
            }
        } catch {
            for entry in entries { releaseInbound(entry.value.raw) }
            throw error
        }
        return .classNode(entries, stating: type)
    }

    /// A class node, assembled from its encoded fields.
    private static func classNode(
        _ fields: [(key: String, value: BamlInboundValue)],
        stating type: BamlTypeDescriptor
    ) -> BamlInboundValue {
        var cls = BamlBridge_Cffi_V1_InboundClassValue()
        cls.fields = fields.map(BamlBridge_Cffi_V1_InboundMapEntry.init)
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.valueType = type.raw
        v.classValue = cls
        return BamlInboundValue(v)
    }

    /// A list node, assembled from its encoded items.
    static func listNode(_ items: [BamlInboundValue], stating type: BamlTypeDescriptor) -> BamlInboundValue {
        var list = BamlBridge_Cffi_V1_InboundListValue()
        list.values = items.map(\.raw)
        var v = BamlBridge_Cffi_V1_InboundValue()
        // Assigning the message sets the oneof case even when the list is
        // empty — the Swift equivalent of Python's `SetInParent()`, so `[]`
        // arrives as an empty list, not null.
        v.listValue = list
        v.valueType = type.raw
        return BamlInboundValue(v)
    }

    /// A map node, assembled from its encoded entries.
    static func mapNode(
        _ entries: [(key: String, value: BamlInboundValue)],
        stating type: BamlTypeDescriptor
    ) -> BamlInboundValue {
        var map = BamlBridge_Cffi_V1_InboundMapValue()
        map.entries = entries.map(BamlBridge_Cffi_V1_InboundMapEntry.init)
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.mapValue = map // presence even when empty, as for a list
        v.valueType = type.raw
        return BamlInboundValue(v)
    }

    /// Build a media value for a generated media struct: the canonical
    /// media payload its `_data` handle holds, never a class wrapping it.
    /// A struct built without a handle has no media to send and goes as
    /// null, which no media slot admits.
    public static func baml_media(_ data: BamlHandle?) -> BamlInboundValue {
        data?._bamlEncode(as: ._leaf(.rustType)) ?? BamlInboundValue()
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

/// One keyword argument of a call: its wire name and how it encodes.
public struct BamlArgument {
    let name: String
    let source: Source

    /// What an argument encodes from.
    enum Source {
        /// A value, which may be refused.
        case value(() throws -> BamlInboundValue)
        /// A host callable (or null), whose encoding registers it. The engine
        /// releases the registration once it holds the handle, so a call that
        /// fails before then releases it itself.
        case callable(BamlHostCallable?)
    }

    private init(name: String, encode: @escaping () throws -> BamlInboundValue) {
        self.name = name
        self.source = .value(encode)
    }

    /// `value` as a node of its declared parameter type.
    public init<Value: BamlEncodable>(_ name: String, _ value: Value, as slot: BamlSlot<Value>) {
        self.init(name: name) { try value._bamlEncode(as: slot) }
    }

    /// `value` under its own Swift type's slot, for a parameter whose
    /// declared type no generated code spells (`client` of a function spec).
    public init(_ name: String, _ value: (any BamlTyped)?) {
        self.init(name: name) { try value?._bamlEncode() ?? BamlInboundValue() }
    }

    /// A host callable, stated as the function type it was built with.
    public init(_ name: String, _ callable: BamlHostCallable?) {
        self.name = name
        self.source = .callable(callable)
    }

    /// An explicit BAML null.
    public static func null(_ name: String) -> BamlArgument {
        BamlArgument(name: name) { BamlInboundValue() }
    }

    /// An engine handle used as a receiver: its type is the engine's.
    init(_ name: String, handle: BamlHandle) {
        self.init(name: name) { handle._bamlEncode(as: ._leaf(.rustType)) }
    }

    func encode() throws -> BamlInboundValue {
        switch source {
        case .value(let encode): return try encode()
        case .callable(let callable): return callable?._bamlEncode() ?? BamlInboundValue()
        }
    }

    /// Release what `encoded`, this argument's encoding, holds, for a call
    /// the engine never receives.
    func release(_ encoded: BamlBridge_Cffi_V1_InboundValue) {
        switch source {
        case .value: releaseInbound(encoded)
        case .callable:
            if case .handle(let handle)? = encoded.value {
                HostCallableRegistry.shared.release(handle.key)
            }
        }
    }
}

/// One type argument of a generic call: the type parameter's name and the
/// exact type it is bound to.
///
/// The engine binds a type parameter from the values only where a value pins
/// it, and refuses a call that leaves one unbound (`one_type_arg<T>()`). A
/// Swift generic always knows its `T`, so generated code names every type
/// parameter of the callee this way, and the arguments are encoded under the
/// same `T`'s slot.
public struct BamlTypeArgument: Sendable {
    let name: String
    let type: BamlTypeDescriptor

    public init(_ name: String, _ type: BamlTypeDescriptor) {
        self.name = name
        self.type = type
    }
}

/// Serialize one call's kwargs to `CallFunctionArgs` bytes. An omitted
/// optional argument is absent from `args` (see `BamlOptional`).
func encodeCallArgs(
    _ args: [BamlArgument],
    typeArgs: [BamlTypeArgument] = [],
    callId: UInt64,
    callTarget: BamlBridge_Cffi_V1_CallFunctionArgs.OneOf_CallTarget,
    baml: BamlInvocationOptions = .init()
) throws -> Data {
    guard callId != 0 else { throw BamlDecodeError.unsupported("could not allocate invocation") }
    var msg = BamlBridge_Cffi_V1_CallFunctionArgs()
    do {
    msg.callID = callId
    msg.callTarget = callTarget
    msg.invocation.inheritedState = HostInvocationScope.current?.state.key ?? 0
    msg.invocation.hostEnvironment = callId
    if let timeout = baml.timeoutMs {
        guard timeout >= 0 && timeout <= Int64(Int32.max) else { throw BamlDecodeError.unsupported("timeoutMs must be between 0 and 2147483647") }
        var clock: UInt64 = 0
        guard BamlApi.invocationClockNs(callId, &clock) == BAML_CFFI_STATUS_OK.rawValue else { throw BamlDecodeError.unsupported("invalid call allocation") }
        let (deadline, overflow) = clock.addingReportingOverflow(UInt64(timeout) * 1_000_000)
        guard !overflow else { throw BamlDecodeError.unsupported("deadline overflow") }
        msg.invocation.deadlineNs = deadline
    }
    if let trace = baml.trace {
        var buffer = BamlBuffer(); var owner: UInt64 = 0
        guard BamlApi.traceSelection(callId, trace.key, &buffer, &owner) == BAML_CFFI_STATUS_OK.rawValue else { throw BamlDecodeError.unsupported("invalid trace selection") }
        do { msg.invocation.trace = try BamlBridge_Cffi_V1_TraceSelection(serializedBytes: BamlApi.takeBuffer(buffer)) }
        catch { if owner != 0 { _ = BamlApi.handleRelease(owner) }; throw error }
    }
    if let cancel = baml.cancel { msg.invocation.cancel = try cancel().raw }
    // One argument at a time, so a refusal finds every argument before it
    // in `msg.kwargs` for the release below.
    for argument in args {
        var entry = BamlBridge_Cffi_V1_InboundMapEntry()
        entry.stringKey = argument.name
        entry.value = try argument.encode().raw
        msg.kwargs.append(entry)
    }
    msg.typeArgs = typeArgs.map { argument in
        var wire = BamlBridge_Cffi_V1_BamlTyArg()
        wire.typeVar = argument.name
        wire.typeValue = argument.type.raw
        return wire
    }
    return try msg.serializedData()
    } catch {
        if case .reservation(let owner)? = msg.invocation.trace.selection { _ = BamlApi.handleRelease(owner) }
        if msg.invocation.hasCancel { releaseInbound(msg.invocation.cancel) }
        for (argument, entry) in zip(args, msg.kwargs) { argument.release(entry.value) }
        _ = BamlApi.releaseFunctionCall(callId)
        throw error
    }
}
