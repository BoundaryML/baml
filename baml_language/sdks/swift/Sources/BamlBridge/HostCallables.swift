import CBamlBridge
import Foundation

/// Host-callable support: BAML calling back into Swift.
///
/// Wire contract (mirrors bridge_python):
/// - Encoding a Swift closure registers it in [`HostCallableRegistry`]
///   and sends `InboundValue.handle{key, HOST_VALUE_CALLABLE}`.
/// - The engine invokes it through the process-global `HostDispatchFn`
///   with a protobuf `BamlToHostCall` args payload. Dispatch is
///   fire-and-return on an engine thread: we copy the bytes, hop to a
///   Task, and later resolve with `complete_host_call(call_id, …)` —
///   exactly once per call_id, on every exit path.
/// - Success: `is_error = 0`, payload = the result as an
///   `InboundValue`. Host throw: `is_error = 1`, payload = the thrown
///   value as an `InboundValue` — either a real BAML error class
///   (thrown as [`BamlThrownValue`] or a `BamlError` with a decodable
///   payload… not re-encodable host-side, so use `BamlThrownValue`) or
///   an opaque `baml.errors.HostCallable` envelope whose `_handle`
///   keeps the original Swift `Error` alive in the registry for
///   same-process rehydration.
/// - `HostReleaseFn` fires when the engine drops its last reference;
///   the registry entry (closure or stored error) is evicted.

/// Decoded arguments of one engine → host invocation. The engine sends
/// only *supplied* args, in declared order: required ones positionally,
/// optional ones tagged with their name.
public struct BamlHostArgs: Sendable {
    let positional: [BamlOutboundValue]
    let named: [String: BamlOutboundValue]

    init(_ call: BamlBridge_Cffi_V1_BamlToHostCall) {
        var positional: [BamlOutboundValue] = []
        var named: [String: BamlOutboundValue] = [:]
        for arg in call.args {
            if arg.isOptionalArg {
                named[arg.argName] = BamlOutboundValue(arg.value)
            } else {
                positional.append(BamlOutboundValue(arg.value))
            }
        }
        self.positional = positional
        self.named = named
    }

    /// Decode required positional arg `index`.
    public func required<T: BamlDecodable>(_ index: Int) throws -> T {
        guard index < positional.count else {
            throw BamlDecodeError.typeMismatch(
                expected: "positional arg \(index)",
                got: "\(positional.count) positional args"
            )
        }
        return try T._bamlDecode(positional[index])
    }

    /// Decode optional named arg `name`: omitted → `.unset`, explicit
    /// null → `.null`.
    public func optional<T: BamlDecodable>(_ name: String) throws -> BamlOptional<T> {
        guard let value = named[name] else { return .unset }
        if case nil = value.normalized.value { return .null }
        if case .nullValue = value.normalized.value { return .null }
        return .value(try T._bamlDecode(value))
    }
}

/// Throw this from a host callable to deliver a typed BAML error value
/// back into the engine (matched against the callable's declared
/// `throws` contract) — the Swift analog of Python raising
/// `BamlError(SomeGeneratedModel(...))` inside a callback.
public struct BamlThrownValue: Error {
    // `Error` implies `Sendable`, so the stored payload must be too
    // (strict-concurrency checking on newer compilers rejects a bare
    // `any BamlEncodable` here). Every realistic conformer already is:
    // generated models are Sendable value types.
    public let value: any BamlEncodable & Sendable

    public init(_ value: any BamlEncodable & Sendable) {
        self.value = value
    }
}

/// Type-erased host callable, built by generated code (which knows the
/// parameter types). Encoding it registers the body and ships a
/// HOST_VALUE_CALLABLE handle.
public struct BamlHostCallable: BamlEncodable, Sendable {
    let body: @Sendable (BamlHostArgs) async throws -> BamlInboundValue

    public init(_ body: @escaping @Sendable (BamlHostArgs) async throws -> BamlInboundValue) {
        self.body = body
    }

    public func _bamlEncode() -> BamlInboundValue {
        let key = HostCallableRegistry.shared.register(.callable(body))
        var handle = BamlBridge_Cffi_V1_BamlHandle()
        handle.key = key
        handle.handleType = .hostValueCallable
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.handle = handle
        return BamlInboundValue(v)
    }
}

/// One per-process table for everything the engine holds host-side
/// references to: registered callables and opaque thrown errors —
/// the same single-registry design as bridge_python. Keys are
/// process-unique and never 0.
final class HostCallableRegistry: @unchecked Sendable {
    static let shared = HostCallableRegistry()

    enum Entry {
        case callable(@Sendable (BamlHostArgs) async throws -> BamlInboundValue)
        case opaqueError(any Error)
    }

    private let lock = NSLock()
    private var entries: [UInt64: Entry] = [:]
    private var nextKey: UInt64 = 1

    func register(_ entry: Entry) -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        let key = nextKey
        nextKey &+= 1
        entries[key] = entry
        return key
    }

    func lookup(_ key: UInt64) -> Entry? {
        lock.lock()
        defer { lock.unlock() }
        return entries[key]
    }

    func release(_ key: UInt64) {
        lock.lock()
        defer { lock.unlock() }
        entries.removeValue(forKey: key)
    }

    /// Engine → host invocation, already off the engine thread.
    /// Resolves the call_id exactly once on every path.
    func dispatch(key: UInt64, callId: UInt32, argsData: Data) async {
        guard case .callable(let body) = lookup(key) else {
            // Missing callable = bridge fault. An empty error payload
            // is itself a BridgeFailure engine-side, which is the
            // correct surfacing (SdkPanic).
            completeHostCall(callId: callId, isError: 1, payload: Data())
            return
        }
        do {
            let call = try BamlBridge_Cffi_V1_BamlToHostCall(serializedBytes: argsData)
            let result = try await body(BamlHostArgs(call))
            completeHostCall(callId: callId, isError: 0, payload: try result.raw.serializedData())
        } catch let thrown as BamlThrownValue {
            // Typed BAML throw: rides as the real class value so the
            // engine can match it against the declared contract.
            let payload = (try? thrown.value._bamlEncode().raw.serializedData()) ?? Data()
            completeHostCall(callId: callId, isError: 1, payload: payload)
        } catch {
            // Any other Swift error → opaque baml.errors.HostCallable
            // envelope; the original object stays in the registry for
            // same-process rehydration on the way back out.
            let payload = (try? opaqueHostCallableEnvelope(error).raw.serializedData()) ?? Data()
            completeHostCall(callId: callId, isError: 1, payload: payload)
        }
    }

    private func opaqueHostCallableEnvelope(_ error: any Error) -> BamlInboundValue {
        let key = register(.opaqueError(error))
        var handle = BamlBridge_Cffi_V1_BamlHandle()
        handle.key = key
        handle.handleType = .hostValueOpaque
        var rawHandleValue = BamlBridge_Cffi_V1_InboundValue()
        rawHandleValue.handle = handle
        let handleValue = BamlInboundValue(rawHandleValue)
        return .baml_class(
            "baml.errors.HostCallable",
            [
                ("message", String(describing: error)),
                ("class_name", String(describing: type(of: error))),
                ("language", "swift"),
                // Declared `string?` on the BAML class — the engine
                // requires the field present, so send explicit null.
                ("traceback", Swift.String?.none),
                ("_handle", RawInbound(handleValue)),
            ]
        )
    }
}

/// Tiny adapter so a prebuilt raw inbound value can ride through the
/// `baml_class` field builder.
private struct RawInbound: BamlEncodable {
    let value: BamlInboundValue
    init(_ value: BamlInboundValue) { self.value = value }
    func _bamlEncode() -> BamlInboundValue { value }
}

private func completeHostCall(callId: UInt32, isError: Int32, payload: Data) {
    payload.withUnsafeBytes { buf in
        BamlApi.completeHostCall(
            callId,
            isError,
            buf.baseAddress?.assumingMemoryBound(to: Int8.self),
            buf.count
        )
    }
}

/// Process-global dispatch entry: copy the Rust-owned bytes, then
/// fire-and-return (the engine thread must not be blocked; the
/// no-synchronous-re-entrancy rule is honored because the body runs
/// on a detached Task).
let bamlHostDispatch: BamlHostDispatchCallback = { key, callId, args, length in
    let data: Data
    if let args, length > 0 {
        data = Data(bytes: args, count: Int(length))
    } else {
        data = Data()
    }
    Task.detached {
        await HostCallableRegistry.shared.dispatch(key: key, callId: callId, argsData: data)
    }
}

let bamlHostRelease: BamlHostReleaseCallback = { key in
    HostCallableRegistry.shared.release(key)
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
            handle.handleType == .hostValueOpaque || handle.handleType == .hostValueCallable,
            case .opaqueError(let original) = HostCallableRegistry.shared.lookup(handle.key)
        {
            return original
        }
    }
    return nil
}
