import CBamlBridge
import Foundation

/// Host-callable support: BAML calling back into Swift.
///
/// Wire contract (ABI.md, "A callback has one completion channel"):
/// - Encoding a Swift closure registers it in [`HostCallableRegistry`]
///   and sends `InboundValue.handle{key, HOST_VALUE_CALLABLE}` stating
///   the closure's function type.
/// - The engine invokes it through the process-global `HostDispatchFn`
///   with a protobuf `BamlToHostCall` args payload. Dispatch is
///   fire-and-return on an engine thread: we copy the bytes, hop to a
///   Task, and later resolve with `complete_host_call(call_id, payload)` —
///   exactly once per call_id, on every exit path.
/// - The payload is a `HostCallCompletion` with one outcome, decided by
///   where the call ended:
///   - `returned`: the result, encoded against the declared return type.
///   - `thrown`: a BAML value. A [`BamlThrownValue`] is encoded against
///     the declared throws type; a `BamlError` or `BamlPanic` forwarded
///     from a nested BAML call goes back as the value it carried (a
///     `baml.panics.*` value is a panic whatever the callable declares);
///     a `CancellationError`, Swift's spelling of a cancelled BAML call,
///     goes back as `baml.panics.Cancelled`.
///   - `raised`: any other Swift error, the user's own: registered so the
///     engine builds `baml.errors.HostCallable` around its `_handle` and
///     this process can rehydrate it.
///   - `bridge_failure`: the bridge's own fault, never the callable's:
///     arguments it could not decode or that do not match the declared
///     parameters, a result or thrown value it could not encode (too deep
///     for the wire, a handle it could not clone), a call cancelled before
///     it ran.
///   A value the engine finds not to inhabit its declared type is the
///   callable's breach of contract, which the engine reports itself.
/// - Every send of a key takes one reference to it in the registry; the
///   engine gives them back through `HostReleaseFn`, and the last one evicts
///   the entry (closure or stored error).

/// The bridge's own failure while it serves a callback: a fault in the
/// arguments it was handed or in the value it was to send back. Only the
/// bridge throws it, outside the callable's body, so a callable can never
/// raise one as its own.
struct BamlHostFault: Error {
    let message: String
}

/// Decoded arguments of one engine → host invocation. The engine sends
/// only *supplied* args, in declared order: required ones positionally,
/// optional ones tagged with their name.
public struct BamlHostArgs: Sendable {
    let positional: [BamlOutboundValue]
    let named: [String: BamlOutboundValue]

    /// The arguments of `call`, which must be those `signature` declares:
    /// one positional argument per required parameter, and only optional
    /// parameters, each at most once, by name.
    init(_ call: BamlBridge_Cffi_V1_BamlToHostCall, signature: BamlTypeDescriptor) throws(BamlHostFault) {
        guard case .function(let function)? = signature.raw.ty else {
            throw BamlHostFault(message: "the host callable is not stated as a function type")
        }
        let required = function.params.filter { $0.mode != .optional }.count
        let optional = Set(function.params.filter { $0.mode == .optional }.map(\.name))
        var positional: [BamlOutboundValue] = []
        var named: [String: BamlOutboundValue] = [:]
        for arg in call.args {
            if arg.isOptionalArg {
                guard optional.contains(arg.argName) else {
                    throw BamlHostFault(message: "the callback was passed `\(arg.argName)`, which is not one of its optional parameters")
                }
                guard named.updateValue(BamlOutboundValue(arg.value), forKey: arg.argName) == nil else {
                    throw BamlHostFault(message: "the callback was passed `\(arg.argName)` twice")
                }
            } else {
                positional.append(BamlOutboundValue(arg.value))
            }
        }
        guard positional.count == required else {
            throw BamlHostFault(
                message: "the callback takes \(required) positional argument(s) but was passed \(positional.count)"
            )
        }
        self.positional = positional
        self.named = named
    }

    /// Decode required positional arg `index` as a node of its declared
    /// type `slot`.
    public func required<T: BamlDecodable>(_ index: Int, as slot: BamlSlot<T>) throws -> T {
        guard positional.indices.contains(index) else {
            throw BamlHostFault(message: "the callback has no positional argument \(index)")
        }
        do {
            return try T._bamlDecode(positional[index], as: slot)
        } catch {
            throw BamlHostFault(message: "positional argument \(index) could not be decoded: \(error)")
        }
    }

    /// Decode optional named arg `name` as a node of `slot`, its declared
    /// type without the null: omitted → `.unset`, explicit null → `.null`.
    public func optional<T: BamlDecodable>(_ name: String, as slot: BamlSlot<T>) throws -> BamlOptional<T> {
        guard let value = named[name] else { return .unset }
        switch value.normalized.value {
        case nil, .nullValue: return .null
        default:
            do {
                return .value(try T._bamlDecode(value, as: slot))
            } catch {
                throw BamlHostFault(message: "argument `\(name)` could not be decoded: \(error)")
            }
        }
    }

    /// The callable's result, encoded against its declared return type
    /// `slot`. A result that cannot be encoded is the bridge's failure, not
    /// the callable's.
    public func result<Value: BamlEncodable>(_ value: Value, as slot: BamlSlot<Value>) throws -> BamlInboundValue {
        do {
            return try value._bamlEncode(as: slot)
        } catch {
            throw BamlHostFault(message: "the callback's result could not be encoded: \(error)")
        }
    }
}

/// Throw this from a host callable to deliver a typed BAML error value
/// back into the engine (matched against the callable's declared
/// `throws` contract) — the Swift analog of Python raising
/// `BamlError(SomeGeneratedModel(...))` inside a callback.
public struct BamlThrownValue: Error {
    // `Error` implies `Sendable`, so the stored payload must be too
    // (strict-concurrency checking on newer compilers rejects a bare
    // `any BamlTyped` here). Every realistic conformer already is:
    // generated models are Sendable value types.
    public let value: any BamlTyped & Sendable

    public init(_ value: any BamlTyped & Sendable) {
        self.value = value
    }
}

/// One type a callable's thrown value is encoded under: the declared throws
/// type, or one member of a declared union. Generated code lists them, the
/// whole declared type first; a [`BamlThrownValue`] is encoded under the
/// first whose Swift type its value has.
public struct BamlThrownSlot: Sendable {
    private let slot: any ThrownSlot

    public init<Value: BamlEncodable>(_ slot: BamlSlot<Value>) {
        self.slot = TypedThrownSlot(slot: slot)
    }

    /// `value` encoded under this slot, or `nil` when it is not of the
    /// slot's Swift type.
    func encode(_ value: any BamlTyped & Sendable) throws -> BamlInboundValue? {
        try slot.encode(value)
    }
}

private protocol ThrownSlot: Sendable {
    func encode(_ value: any BamlTyped & Sendable) throws -> BamlInboundValue?
}

private struct TypedThrownSlot<Value: BamlEncodable>: ThrownSlot {
    let slot: BamlSlot<Value>

    func encode(_ value: any BamlTyped & Sendable) throws -> BamlInboundValue? {
        guard let value = value as? Value else { return nil }
        return try value._bamlEncode(as: slot)
    }
}

/// Type-erased host callable, built by generated code (which knows the
/// declared function type). Encoding it registers the body and ships a
/// HOST_VALUE_CALLABLE handle with that type stated beside it: a host
/// callable has no type of its own, so the one its slot declares is what the
/// engine binds it to. It crosses only as a call argument (`BamlArgument`).
///
/// `body` decodes its arguments through [`BamlHostArgs`] and encodes its
/// result with [`BamlHostArgs.result(_:as:)`], so every failure of the
/// bridge's own surfaces as one, apart from what the callable throws.
public struct BamlHostCallable: Sendable {
    let functionType: BamlTypeDescriptor
    let thrown: [BamlThrownSlot]
    let body: @Sendable (BamlHostArgs) async throws -> BamlInboundValue

    /// `thrown` holds the declared throws type's slots, empty when the
    /// throws type is open (`unknown`, a value states its own type) or
    /// `never`.
    public init(
        functionType: BamlTypeDescriptor,
        thrown: [BamlThrownSlot],
        _ body: @escaping @Sendable (BamlHostArgs) async throws -> BamlInboundValue
    ) {
        self.functionType = functionType
        self.thrown = thrown
        self.body = body
    }

    func _bamlEncode() -> BamlInboundValue {
        let key = HostCallableRegistry.shared.register(.callable(self))
        var handle = BamlBridge_Cffi_V1_BamlHandle()
        handle.key = key
        handle.handleType = .hostValueCallable
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.valueType = functionType.raw
        v.handle = handle
        return BamlInboundValue(v)
    }

    /// Run one invocation and decide its outcome (see the wire contract
    /// above). A value the wire cannot carry (deeper than the engine
    /// decodes) never leaves: its handle clones are released and the call
    /// ends as the bridge's failure instead.
    func invoke(_ argsData: Data) async -> BamlBridge_Cffi_V1_HostCallCompletion.OneOf_Outcome {
        let outcome = await run(argsData)
        switch outcome {
        case .returned(let value), .thrown(let value):
            guard let refusal = BamlWireDepth.refusal(of: value, below: BamlWireDepth.resultDepth) else {
                return outcome
            }
            releaseInbound(value)
            return .bridgeFailure("the callback's value cannot cross: \(refusal)")
        case .raised, .contractViolation, .bridgeFailure:
            return outcome
        }
    }

    private func run(_ argsData: Data) async -> BamlBridge_Cffi_V1_HostCallCompletion.OneOf_Outcome {
        let call: BamlBridge_Cffi_V1_BamlToHostCall
        do {
            call = try BamlBridge_Cffi_V1_BamlToHostCall(serializedBytes: argsData)
        } catch {
            return .bridgeFailure("the callback's arguments could not be read: \(error)")
        }
        // The rows Swift keeps no owner for go back at once, whatever happens
        // to the call.
        for arg in call.args { releaseUnownedRows(arg.value) }
        // The engine drops a call it cancelled; one that never ran has no
        // outcome of the callable's.
        guard !Task.isCancelled else {
            return .bridgeFailure("the callback was cancelled before it ran")
        }
        let args: BamlHostArgs
        do throws(BamlHostFault) {
            args = try BamlHostArgs(call, signature: functionType)
        } catch {
            return .bridgeFailure(error.message)
        }
        do {
            return .returned(try await body(args).raw)
        } catch let fault as BamlHostFault {
            return .bridgeFailure(fault.message)
        } catch let thrown as BamlThrownValue {
            return thrownOutcome(thrown.value)
        } catch let error as BamlError {
            return forwardedOutcome(error.payload, error)
        } catch let panic as BamlPanic {
            return forwardedOutcome(panic.payload, panic)
        } catch is CancellationError {
            return .thrown(cancelledPanic())
        } catch {
            return .raised(HostCallableRegistry.shared.hostException(error))
        }
    }

    /// A [`BamlThrownValue`] encoded under the first declared throws slot
    /// its value fills. Under an open throws type, or when no declared slot
    /// takes it (a panic, or a value outside the declaration, which the
    /// engine refuses), it states its own type.
    private func thrownOutcome(
        _ value: any BamlTyped & Sendable
    ) -> BamlBridge_Cffi_V1_HostCallCompletion.OneOf_Outcome {
        do {
            for slot in thrown {
                if let encoded = try slot.encode(value) {
                    return .thrown(encoded.raw)
                }
            }
            return .thrown(try value._bamlEncode().raw)
        } catch {
            return .bridgeFailure("the callback's thrown value could not be encoded: \(error)")
        }
    }

    /// A `BamlError` or `BamlPanic` from a nested BAML call, thrown back as
    /// the value it carried. One built host-side with no value is the
    /// callable's own exception.
    private func forwardedOutcome(
        _ payload: BamlOutboundValue?,
        _ error: any Error
    ) -> BamlBridge_Cffi_V1_HostCallCompletion.OneOf_Outcome {
        guard let payload else {
            return .raised(HostCallableRegistry.shared.hostException(error))
        }
        do throws(BamlHostFault) {
            return .thrown(try restate(payload.raw))
        } catch {
            return .bridgeFailure("the forwarded BAML error could not be sent back: \(error.message)")
        }
    }
}

/// The `baml.panics.Cancelled` a `CancellationError` stands for: the runtime
/// surfaces a cancelled BAML call as Swift's `CancellationError`, and a
/// callable that lets one escape is cancelled in turn.
private func cancelledPanic() -> BamlBridge_Cffi_V1_InboundValue {
    var message = BamlBridge_Cffi_V1_InboundMapEntry()
    message.stringKey = "message"
    message.value.stringValue = "the Swift host callable was cancelled"
    var cls = BamlBridge_Cffi_V1_InboundClassValue()
    cls.fields = [message]
    var value = BamlBridge_Cffi_V1_InboundValue()
    value.valueType = BamlTypeDescriptor.classType("baml.panics.Cancelled").raw
    value.classValue = cls
    return value
}

/// One per-process table for everything the engine holds host-side
/// references to: registered callables and opaque thrown errors —
/// the same single-registry design as bridge_python. Keys are
/// process-unique and never 0.
///
/// Each key counts the references to it this host handed out: a
/// registration holds the one its first send takes, each further send takes
/// one more, and the engine's release, or the rollback of a send that never
/// reached it, gives them back. The last one removes the entry.
final class HostCallableRegistry: @unchecked Sendable {
    static let shared = HostCallableRegistry()

    enum Entry {
        case callable(BamlHostCallable)
        case opaqueError(any Error)
    }

    /// A registered key: its entry, and how many references to the key are
    /// out.
    private struct Registration {
        let entry: Entry
        var count: UInt64
    }

    private let lock = NSLock()
    private var registrations: [UInt64: Registration] = [:]
    private var nextKey: UInt64 = 1

    /// Register `entry` under a fresh key, holding the one reference the send
    /// it is registered for takes.
    func register(_ entry: Entry) -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        let key = nextKey
        nextKey &+= 1
        registrations[key] = Registration(entry: entry, count: 1)
        return key
    }

    /// Take one more reference to `key`, for sending it again. The check and
    /// the count are one step under the lock, so a release racing the send
    /// either comes after the count or refuses the send.
    func sendAgain(_ key: UInt64) throws(BamlHostKeyGone) {
        lock.lock()
        defer { lock.unlock() }
        guard registrations[key] != nil else { throw BamlHostKeyGone(key: key) }
        registrations[key]!.count += 1
    }

    /// Give back `count` references to `key`; the last one removes it. A key
    /// that is not registered is ignored: a reference given back twice is
    /// the giver's bug, never applied to another key.
    func release(_ key: UInt64, count: UInt64) {
        lock.lock()
        defer { lock.unlock() }
        guard let registration = registrations[key] else { return }
        assert(count > 0 && count <= registration.count, "released \(count) of \(registration.count) references to host key \(key)")
        if count >= registration.count {
            registrations.removeValue(forKey: key)
        } else {
            registrations[key]!.count -= count
        }
    }

    func lookup(_ key: UInt64) -> Entry? {
        lock.lock()
        defer { lock.unlock() }
        return registrations[key]?.entry
    }

    /// How many references to `key` are out; 0 when it is not registered.
    func references(to key: UInt64) -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        return registrations[key]?.count ?? 0
    }

    /// Every key registered now.
    var registeredKeys: Set<UInt64> {
        lock.lock()
        defer { lock.unlock() }
        return Set(registrations.keys)
    }

    /// Engine → host invocation, already off the engine thread.
    /// Resolves the call_id exactly once on every path.
    func dispatch(key: UInt64, callId: UInt32, argsData: Data) async {
        guard case .callable(let callable) = lookup(key) else {
            // Missing callable = bridge fault, surfaced engine-side as SdkPanic.
            completeHostCall(
                callId: callId,
                outcome: .bridgeFailure("no Swift host callable is registered for the dispatched key")
            )
            return
        }
        completeHostCall(callId: callId, outcome: await callable.invoke(argsData))
    }

    /// `error`, the callable's own, registered so that this process can
    /// rehydrate it when it comes back.
    func hostException(_ error: any Error) -> BamlBridge_Cffi_V1_HostException {
        var exception = BamlBridge_Cffi_V1_HostException()
        exception.message = String(describing: error)
        exception.className = String(describing: type(of: error))
        exception.language = "swift"
        exception.handle = register(.opaqueError(error))
        return exception
    }
}

/// A send of a host key whose references were all given back: sent, it would
/// hand the engine a key this host no longer resolves.
struct BamlHostKeyGone: Error, CustomStringConvertible {
    let key: UInt64

    var description: String { "host value \(key) is no longer registered" }
}

/// Send `outcome` for `callId`.
private func completeHostCall(
    callId: UInt32,
    outcome: BamlBridge_Cffi_V1_HostCallCompletion.OneOf_Outcome
) {
    var completion = BamlBridge_Cffi_V1_HostCallCompletion()
    completion.outcome = outcome
    let payload: Data
    do {
        payload = try completion.serializedData()
    } catch {
        switch outcome {
        case .returned(let value), .thrown(let value): releaseInbound(value)
        case .raised(let exception): HostCallableRegistry.shared.release(exception.handle, count: 1)
        case .contractViolation, .bridgeFailure: break
        }
        var failure = BamlBridge_Cffi_V1_HostCallCompletion()
        failure.outcome = .bridgeFailure("the callback's completion could not be serialized: \(error)")
        // A completion holding one short string always serializes; were it
        // ever to fail, the empty payload is itself a bridge failure.
        payload = (try? failure.serializedData()) ?? Data()
    }
    // The engine decodes the completion on the calling thread: see
    // `BamlNativeThread`.
    BamlNativeThread.shared.run {
        payload.withUnsafeBytes { buf in
            BamlApi.completeHostCall(
                callId,
                buf.baseAddress?.assumingMemoryBound(to: Int8.self),
                buf.count
            )
        }
    }
}

/// Process-global dispatch entry: copy the Rust-owned bytes, then
/// fire-and-return (the engine thread must not be blocked; the
/// no-synchronous-re-entrancy rule is honored because the body runs
/// on a detached Task).
enum HostInvocationScope {
    @TaskLocal static var current: BamlInvocationCapture?
}

/// Release every engine row `value`, a control value this process kept,
/// holds.
func releaseControlValue(_ value: BamlBridge_Cffi_V1_BamlOutboundValue) {
    for row in engineRows(value) {
        switch row {
        case .handle(let key), .pin(let key), .typeValue(let key): _ = BamlApi.handleRelease(key)
        }
    }
}

private final class HostInvocationFrame: @unchecked Sendable {
    let wire: BamlBridge_Cffi_V1_HostInvocation
    let capture: BamlInvocationCapture
    init(_ wire: BamlBridge_Cffi_V1_HostInvocation) {
        self.wire = wire
        capture = BamlInvocationCapture(state: BamlHandle(key: wire.effectiveState, handleType: .invocationState), cancel: wire.cancel)
    }

}

private final class HostInvocationTasks: @unchecked Sendable {
    static let shared = HostInvocationTasks()
    private let lock = NSLock()
    private var tasks: [UInt32: Task<Void, Never>] = [:]
    func launch(_ frame: HostInvocationFrame) {
        lock.lock()
        let task = Task.detached {
            await HostInvocationScope.$current.withValue(frame.capture) {
                await HostCallableRegistry.shared.dispatch(key: frame.wire.hostValueKey, callId: frame.wire.callbackID, argsData: frame.wire.applicationArgs)
            }
            self.remove(frame.wire.callbackID)
        }
        tasks[frame.wire.callbackID] = task
        lock.unlock()
    }
    func cancel(_ id: UInt32) {
        lock.lock()
        let task = tasks[id]
        lock.unlock()
        task?.cancel()
    }
    private func remove(_ id: UInt32) {
        lock.lock()
        tasks.removeValue(forKey: id)
        lock.unlock()
    }
}

let bamlHostDispatch: BamlHostDispatchV2 = { request, length in
    guard let request, length <= Int.max,
        let wire = try? BamlBridge_Cffi_V1_HostInvocation(serializedBytes: Data(bytes: request, count: Int(length))) else { return }
    HostInvocationTasks.shared.launch(HostInvocationFrame(wire))
}

let bamlHostCancel: BamlHostCancel = { callbackID in
    HostInvocationTasks.shared.cancel(callbackID)
}

let bamlHostRelease: BamlHostReleaseCallback = { key, count in
    HostCallableRegistry.shared.release(key, count: count)
}

/// Same-process rehydration: if a thrown `baml.errors.HostCallable`
/// envelope's `_handle` still resolves to the original Swift error in
/// the registry, surface THAT object (identity preserved), not a
/// wrapper. Called from the envelope decode path.
func rehydrateHostError(_ payload: BamlBridge_Cffi_V1_BamlOutboundValue) -> (any Error)? {
    var current = payload
    while case .unionVariantValue(let variant) = current.value {
        current = variant.value
    }
    guard case .classValue(let cls) = current.value, cls.name == "baml.errors.HostCallable" else {
        return nil
    }
    for entry in cls.fields where entry.key == "_handle" {
        if case .handleValue(let handle) = entry.value.value,
            BamlHandle.keySpace(of: handle.handleType) == .host,
            case .opaqueError(let original) = HostCallableRegistry.shared.lookup(handle.key)
        {
            return original
        }
    }
    return nil
}

/// `value`, received from the engine, restated as the host sends a value:
/// each list, map and class states the exact type the engine gave it, a
/// union selection is its selected value, and each engine handle is a fresh
/// clone (the received key stays with the error that carried it, which may
/// be forwarded again), and each host value takes one more reference to its
/// key. A value that cannot be restated (a runtime declaration, whose rows
/// went back on arrival, a host value no longer registered, or a map keyed
/// by other than strings) is refused, its clones and references released.
func restate(_ value: BamlBridge_Cffi_V1_BamlOutboundValue) throws(BamlHostFault) -> BamlBridge_Cffi_V1_InboundValue {
    var restated = BamlBridge_Cffi_V1_InboundValue()
    guard let arm = value.value else { return restated }
    switch arm {
    case .nullValue: break
    case .stringValue(let string): restated.stringValue = string
    case .intValue(let int): restated.intValue = int
    case .floatValue(let float): restated.floatValue = float
    case .boolValue(let bool): restated.boolValue = bool
    case .bigintValue(let bigint): restated.bigintValue = bigint
    case .uint8ArrayValue(let bytes): restated.uint8ArrayValue = bytes
    case .mediaValue(let media): restated.mediaValue = media
    case .promptAstValue(let prompt): restated.promptAstValue = prompt
    case .literalValue(let literal):
        switch literal.literal {
        case .stringValue(let string)?: restated.stringValue = string
        case .intValue(let int)?: restated.intValue = int
        case .boolValue(let bool)?: restated.boolValue = bool
        case .bigintValue(let bigint)?: restated.bigintValue = bigint
        case .floatValue?, nil:
            throw BamlHostFault(message: "a float literal value cannot be restated")
        }
    case .enumValue(let enumValue):
        guard !enumValue.isDynamic else {
            throw BamlHostFault(message: "a value of the runtime enum `\(enumValue.name)` cannot be restated")
        }
        var inbound = BamlBridge_Cffi_V1_InboundEnumValue()
        inbound.name = enumValue.name
        inbound.value = enumValue.value
        restated.enumValue = inbound
    case .unionVariantValue(let variant):
        return try restate(variant.value)
    case .tyValue(let ty):
        try requireNamedHeads(ty)
        restated.tyValue = ty
    case .handleValue(let handle):
        var inbound = BamlBridge_Cffi_V1_BamlHandle()
        inbound.handleType = handle.handleType
        inbound.key = handle.key
        // A host value is this host's own registration and goes under its
        // own key; its pin stays with the value that carried it.
        switch BamlHandle.keySpace(of: handle.handleType) {
        case .engine:
            let status = BamlApi.handleClone(handle.key, &inbound.key)
            guard status == BAML_CFFI_STATUS_OK.rawValue else {
                throw BamlHostFault(message: "handle_clone failed with status \(status)")
            }
        case .host:
            do throws(BamlHostKeyGone) {
                try HostCallableRegistry.shared.sendAgain(handle.key)
            } catch {
                throw BamlHostFault(message: error.description)
            }
        }
        restated.handle = inbound
    case .listValue(let list):
        guard list.hasItemType else {
            throw BamlHostFault(message: "a list without its item type cannot be restated")
        }
        try requireNamedHeads(list.itemType)
        var items: [BamlBridge_Cffi_V1_InboundValue] = []
        items.reserveCapacity(list.items.count)
        do throws(BamlHostFault) {
            for item in list.items { items.append(try restate(item)) }
        } catch {
            items.forEach(releaseInbound)
            throw error
        }
        restated.valueType = BamlTypeDescriptor.list(BamlTypeDescriptor(list.itemType)).raw
        restated.listValue.values = items
    case .mapValue(let map):
        guard map.hasKeyType, map.hasValueType, map.keyType == BamlTypeDescriptor.string.raw else {
            throw BamlHostFault(message: "a map keyed by other than strings cannot be restated")
        }
        try requireNamedHeads(map.valueType)
        var entries: [BamlBridge_Cffi_V1_InboundMapEntry] = []
        entries.reserveCapacity(map.entries.count)
        do throws(BamlHostFault) {
            for entry in map.entries {
                var inbound = BamlBridge_Cffi_V1_InboundMapEntry()
                inbound.stringKey = entry.key
                inbound.value = try restate(entry.value)
                entries.append(inbound)
            }
        } catch {
            for entry in entries { releaseInbound(entry.value) }
            throw error
        }
        restated.valueType = BamlTypeDescriptor.map(
            key: .string,
            value: BamlTypeDescriptor(map.valueType)
        ).raw
        restated.mapValue.entries = entries
    case .classValue(let cls):
        for argument in cls.typeArgs { try requireNamedHeads(argument) }
        var fields: [BamlBridge_Cffi_V1_InboundMapEntry] = []
        fields.reserveCapacity(cls.fields.count)
        do throws(BamlHostFault) {
            for field in cls.fields {
                var inbound = BamlBridge_Cffi_V1_InboundMapEntry()
                inbound.stringKey = field.key
                inbound.value = try restate(field.value)
                fields.append(inbound)
            }
        } catch {
            for field in fields { releaseInbound(field.value) }
            throw error
        }
        var classType = BamlBridge_Cffi_V1_BamlTyClass()
        classType.name = cls.name
        classType.typeArgs = cls.typeArgs
        restated.valueType.classTy = classType
        restated.classValue.fields = fields
    }
    return restated
}

/// Refuse a type that names a declaration other than by its name: a `live`
/// row went back when the value arrived, and an anonymous label is display
/// text the engine refuses in a statement.
private func requireNamedHeads(_ ty: BamlBridge_Cffi_V1_BamlTy) throws(BamlHostFault) {
    var unnamed = false
    forEachHead(ty) { head in
        switch head {
        case .name: break
        case .live, .anonymousLabel: unnamed = true
        }
    }
    guard !unnamed else {
        throw BamlHostFault(message: "a value of a runtime declaration cannot be restated")
    }
}
