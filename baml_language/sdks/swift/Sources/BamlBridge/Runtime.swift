import CBamlBridge
import Foundation

/// The one global completion callback registered with the native
/// bridge (`register_callback` is first-call-wins for the process).
/// The payload buffer is Rust-owned and only valid for the duration of
/// the callback, so bytes are copied out before dispatch.
private let bamlGlobalCompletion: BamlResultCallback = { callbackId, content, length in
    let data: Data
    if let content, length > 0 {
        data = Data(bytes: content, count: Int(length))
    } else {
        data = Data()
    }
    BamlRuntime.shared.completePending(callbackId: callbackId, payload: data)
}

func reportUnhandledSpawnError(
    payload: Data,
    cancelled: Bool,
    hostDefault: (any Error) -> Void = { error in
        fatalError("Unhandled BAML spawn error: \(error)")
    }
) {
    do {
        _ = try unwrapEnvelope(payload)
        throw BamlDecodeError.unsupported("spawned work failed without an error result")
    } catch {
        if cancelled {
            FileHandle.standardError.write(Data("BAML spawned work was cancelled: \(error)\n".utf8))
        } else {
            hostDefault(error)
        }
    }
}

private let bamlGlobalUnhandledSpawnError: BamlUnhandledSpawnErrorCallback = {
    content, length, cancelled in
    let data: Data
    if let content, length > 0 {
        data = Data(bytes: content, count: Int(length))
    } else {
        data = Data()
    }
    reportUnhandledSpawnError(payload: data, cancelled: cancelled != 0)
}

private func bamlShutdownAtExit() {
    BamlRuntime.shared.shutdown()
}

/// Entry points into the native BAML runtime.
///
/// The C ABI is completion-callback based: `call_function` returns
/// immediately after decoding its argument buffer, and the result
/// envelope arrives on `bamlGlobalCompletion` from a Tokio worker
/// thread. Both call forms are built on that:
///
/// - async: `withCheckedThrowingContinuation`, with Task cancellation
///   forwarding to `cancel_function_call`;
/// - sync: a semaphore park. Safe from deadlock because the completion
///   is always delivered on an engine thread, never the caller's —
///   though blocking the main thread is still rude (debug-asserted).
public final class BamlRuntime: @unchecked Sendable {
    public static let shared = BamlRuntime()

    private let lock = NSLock()
    private var pending: [UInt32: @Sendable (Result<Data, Error>) -> Void] = [:]
    private var nextCallbackId: UInt32 = 1
    private var initialized = false
    private var shutdownHookRegistered = false

    private init() {}

    /// Version string reported by the native bridge.
    public static func nativeVersion() -> String {
        String(decoding: BamlApi.takeBuffer(BamlApi.version()), as: UTF8.self)
    }

    public static func toolchainVersion() -> String {
        BamlBridgeIdentity.toolchainVersion
    }

    public static func bridgeRuntimeVersion() -> String {
        BamlBridgeIdentity.bridgeRuntimeVersion
    }

    /// Load compiled BAML bytecode into the (process-global) native
    /// runtime, register this bridge's identity, and register the
    /// completion callback. Idempotent; generated SDK roots call this
    /// from their `_initialized` once.
    ///
    /// `sdkVersion` remains as a compatibility argument for older generated
    /// SDKs. Registration uses the bridge's stamped toolchain and package
    /// identities, while new generated SDKs provide `embeddedBamlToml`.
    public func initialize(
        bytecode: Data,
        sdkVersion: String? = nil,
        embeddedBamlToml: String? = nil
    ) {
        lock.lock()
        defer { lock.unlock() }
        guard !initialized else { return }

        _ = sdkVersion
        let versionBytes = Array(BamlBridgeIdentity.toolchainVersion.utf8)
        let nameBytes = Array(BamlBridgeIdentity.runtimeName.utf8)
        let runtimeVersionBytes = Array(BamlBridgeIdentity.bridgeRuntimeVersion.utf8)
        let registerError = versionBytes.withUnsafeBufferPointer { versionBuffer -> BamlBuffer in
            nameBytes.withUnsafeBufferPointer { nameBuffer in
                runtimeVersionBytes.withUnsafeBufferPointer { runtimeVersionBuffer in
                var info = BamlBridgeInfoV1(
                    struct_size: MemoryLayout<BamlBridgeInfoV1>.size,
                    language: BAML_BRIDGE_LANGUAGE_SWIFT.rawValue,
                    sdk_version: versionBuffer.baseAddress,
                    sdk_version_len: versionBuffer.count,
                    bridge_runtime_name: nameBuffer.baseAddress,
                    bridge_runtime_name_len: nameBuffer.count,
                    bridge_runtime_version: runtimeVersionBuffer.baseAddress,
                    bridge_runtime_version_len: runtimeVersionBuffer.count
                )
                return BamlApi.registerBridge(&info)
            }
            }
        }
        let message = String(decoding: BamlApi.takeBuffer(registerError), as: UTF8.self)
        if !message.isEmpty {
            fatalError(message)
        }

        BamlApi.registerUnhandledSpawnErrorCallback(bamlGlobalUnhandledSpawnError)

        let errorBuffer = bytecode.withUnsafeBytes { buf -> BamlBuffer in
            if let embeddedBamlToml {
                return embeddedBamlToml.withCString { manifest in
                    BamlApi.initializeRuntimeFromBytecodeWithMetadata(
                        buf.baseAddress?.assumingMemoryBound(to: UInt8.self),
                        buf.count,
                        manifest
                    )
                }
            }
            return BamlApi.initializeRuntimeFromBytecode(
                buf.baseAddress?.assumingMemoryBound(to: UInt8.self), buf.count)
        }
        let initError = String(decoding: BamlApi.takeBuffer(errorBuffer), as: UTF8.self)
        if !initError.isEmpty {
            // Init failure is unrecoverable misconfiguration (corrupt
            // inlined bytecode), not a runtime condition.
            fatalError(initError)
        }

        BamlApi.registerCallback(bamlGlobalCompletion)
        BamlApi.registerHostDispatchCallback(bamlHostDispatch)
        BamlApi.registerHostReleaseCallback(bamlHostRelease)
        if !shutdownHookRegistered {
            guard atexit(bamlShutdownAtExit) == 0 else {
                fatalError("BAML runtime shutdown hook registration failed")
            }
            shutdownHookRegistered = true
        }
        initialized = true
    }

    public func shutdown() {
        let message = String(decoding: BamlApi.takeBuffer(BamlApi.shutdownRuntime()), as: UTF8.self)
        if !message.isEmpty {
            fatalError("BAML runtime shutdown failed: \(message)")
        }
        lock.lock()
        initialized = false
        lock.unlock()
    }

    // MARK: - Public call surface

    public func callSync<R: BamlDecodable>(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) throws -> R {
        try R._bamlDecode(unwrapEnvelope(invokeSync(fqn, args: args, operation: operation)))
    }

    /// Undecoded ok-value variants — for callers that interpret the
    /// wire value themselves (BamlStream's next(), which must
    /// distinguish the `ai.stream.Done` sentinel from a partial).
    public func callRawSync(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) throws -> BamlOutboundValue {
        try unwrapEnvelope(invokeSync(fqn, args: args, operation: operation))
    }

    public func callRaw(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) async throws -> BamlOutboundValue {
        do {
            return try unwrapEnvelope(await invokeAsync(fqn, args: args, operation: operation))
        } catch let panic as BamlPanic where panic.className == "baml.panics.Cancelled" {
            throw CancellationError()
        }
    }

    public func callHandleRaw(
        _ handleKey: UInt64,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) async throws -> BamlOutboundValue {
        do {
            return try unwrapEnvelope(await invokeHandleAsync(handleKey, args: args, operation: operation))
        } catch let panic as BamlPanic where panic.className == "baml.panics.Cancelled" {
            throw CancellationError()
        }
    }

    public func callSyncVoid(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) throws {
        _ = try unwrapEnvelope(invokeSync(fqn, args: args, operation: operation))
    }

    public func call<R: BamlDecodable>(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) async throws -> R {
        do {
            return try R._bamlDecode(unwrapEnvelope(await invokeAsync(fqn, args: args, operation: operation)))
        } catch let panic as BamlPanic where panic.className == "baml.panics.Cancelled" {
            // Engine-confirmed cancellation surfaces as Swift's native
            // cancellation error (Python maps it to asyncio.CancelledError
            // the same way). Async-only — sync calls have no cancel path.
            throw CancellationError()
        }
    }

    public func callVoid(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation = .direct
    ) async throws {
        do {
            _ = try unwrapEnvelope(await invokeAsync(fqn, args: args, operation: operation))
        } catch let panic as BamlPanic where panic.className == "baml.panics.Cancelled" {
            throw CancellationError()
        }
    }

    // MARK: - Invocation plumbing

    private func invokeSync(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation
    ) throws -> Data {
        assertNotBlockingMainThreadInDebug(fqn)
        let protoCallId = BamlApi.newFunctionCall()
        let payload = try encodeCallArgs(
            args,
            callId: protoCallId,
            callTarget: .functionName(fqn),
            operation: operation
        )

        let box = ResultBox()
        let semaphore = DispatchSemaphore(value: 0)
        let callbackId = registerPending { result in
            box.store(result)
            semaphore.signal()
        }
        dispatch(payload: payload, callbackId: callbackId)
        semaphore.wait()
        return try box.take().get()
    }

    private func invokeAsync(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation
    ) async throws -> Data {
        let protoCallId = BamlApi.newFunctionCall()
        let payload = try encodeCallArgs(
            args,
            callId: protoCallId,
            callTarget: .functionName(fqn),
            operation: operation
        )

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let callbackId = registerPending { result in
                    continuation.resume(with: result)
                }
                dispatch(payload: payload, callbackId: callbackId)
            }
        } onCancel: {
            // Reserve-based cancel: the engine delivers a Cancelled
            // panic through the normal completion path, which resumes
            // the continuation. (Translating that into Swift's
            // CancellationError is the cancellation phase.)
            _ = BamlApi.cancelFunctionCall(protoCallId)
        }
    }

    private func invokeHandleAsync(
        _ handleKey: UInt64,
        args: [(String, (any BamlEncodable)?)],
        operation: BamlFunctionOperation
    ) async throws -> Data {
        precondition(handleKey != 0, "cannot invoke a zero BAML function handle")
        let protoCallId = BamlApi.newFunctionCall()
        let payload = try encodeCallArgs(
            args,
            callId: protoCallId,
            callTarget: .functionHandle(handleKey),
            operation: operation
        )

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let callbackId = registerPending { result in
                    continuation.resume(with: result)
                }
                dispatch(payload: payload, callbackId: callbackId)
            }
        } onCancel: {
            _ = BamlApi.cancelFunctionCall(protoCallId)
        }
    }

    private func registerPending(
        _ completion: @escaping @Sendable (Result<Data, Error>) -> Void
    ) -> UInt32 {
        lock.lock()
        defer { lock.unlock() }
        let id = nextCallbackId
        // 32-bit wrap is theoretical (4B in-flight calls) but stay
        // defensive: skip ids still pending.
        nextCallbackId = nextCallbackId &+ 1
        if nextCallbackId == 0 { nextCallbackId = 1 }
        pending[id] = completion
        return id
    }

    fileprivate func completePending(callbackId: UInt32, payload: Data) {
        lock.lock()
        let completion = pending.removeValue(forKey: callbackId)
        lock.unlock()
        // Unknown id = late delivery for an abandoned call; drop it.
        completion?(.success(payload))
    }

    private func dispatch(payload: Data, callbackId: UInt32) {
        // `call_function` fully decodes the args buffer before it
        // returns (verified in bridge_cffi::call_function_inner), so
        // scoping the pointers to this call is sound.
        payload.withUnsafeBytes { buf in
            BamlApi.callFunction(
                buf.baseAddress?.assumingMemoryBound(to: UInt8.self),
                buf.count,
                callbackId
            )
        }
    }

    private func assertNotBlockingMainThreadInDebug(_ fqn: String) {
        #if DEBUG
        // Legal but rude: a sync BAML call on the main thread beachballs
        // the UI for the duration of the engine call.
        if Thread.isMainThread, ProcessInfo.processInfo.environment["BAML_ALLOW_MAIN_THREAD_SYNC"] == nil {
            print("warning: sync BAML call `\(fqn)` on the main thread — prefer the async form (set BAML_ALLOW_MAIN_THREAD_SYNC=1 to silence)")
        }
        #endif
    }
}

/// Single-assignment box for handing a result across the semaphore
/// park in `invokeSync` under Swift 6 strict concurrency.
private final class ResultBox: @unchecked Sendable {
    private var value: Result<Data, Error>?
    private let lock = NSLock()

    func store(_ result: Result<Data, Error>) {
        lock.lock()
        defer { lock.unlock() }
        value = result
    }

    func take() -> Result<Data, Error> {
        lock.lock()
        defer { lock.unlock() }
        guard let value else {
            return .failure(BamlDecodeError.unsupported("completion signaled without a result"))
        }
        return value
    }
}
