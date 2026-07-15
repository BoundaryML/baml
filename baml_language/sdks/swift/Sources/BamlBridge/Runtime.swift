import CBamlBridge
import Foundation

/// The one global completion callback registered with the native
/// bridge (`register_callback` is first-call-wins for the process).
/// The payload buffer is Rust-owned and only valid for the duration of
/// the callback, so bytes are copied out before dispatch.
private let bamlGlobalCompletion: @convention(c) (
    UInt32, UnsafePointer<Int8>?, UInt
) -> Void = { callbackId, content, length in
    let data: Data
    if let content, length > 0 {
        data = Data(bytes: content, count: Int(length))
    } else {
        data = Data()
    }
    BamlRuntime.shared.completePending(callbackId: callbackId, payload: data)
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

    private init() {}

    /// Version string reported by the native bridge.
    public static func nativeVersion() -> String {
        let buf = version()
        defer { free_buffer(buf) }
        guard let ptr = buf.ptr, buf.len > 0 else { return "" }
        return String(decoding: Data(bytes: ptr, count: Int(buf.len)), as: UTF8.self)
    }

    /// Load compiled BAML bytecode into the (process-global) native
    /// runtime and register the completion callback. Idempotent;
    /// generated SDK roots call this from their `_initialized` once.
    public func initialize(bytecode: Data) {
        lock.lock()
        defer { lock.unlock() }
        guard !initialized else { return }

        let errorBuffer = bytecode.withUnsafeBytes { buf -> Buffer in
            initialize_runtime_from_bytecode(
                buf.baseAddress?.assumingMemoryBound(to: UInt8.self),
                UInt(buf.count)
            )
        }
        if let ptr = errorBuffer.ptr, errorBuffer.len > 0 {
            let message = String(
                decoding: Data(bytes: ptr, count: Int(errorBuffer.len)),
                as: UTF8.self
            )
            free_buffer(errorBuffer)
            // Init failure is unrecoverable misconfiguration (corrupt
            // inlined bytecode), not a runtime condition.
            fatalError("BAML runtime initialization failed: \(message)")
        }

        register_callback(bamlGlobalCompletion)
        initialized = true
    }

    // MARK: - Public call surface

    public func callSync<R: BamlDecodable>(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)]
    ) throws -> R {
        try R._bamlDecode(unwrapEnvelope(invokeSync(fqn, args: args)))
    }

    public func callSyncVoid(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)]
    ) throws {
        _ = try unwrapEnvelope(invokeSync(fqn, args: args))
    }

    public func call<R: BamlDecodable>(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)]
    ) async throws -> R {
        try R._bamlDecode(unwrapEnvelope(await invokeAsync(fqn, args: args)))
    }

    public func callVoid(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)]
    ) async throws {
        _ = try unwrapEnvelope(await invokeAsync(fqn, args: args))
    }

    // MARK: - Invocation plumbing

    private func invokeSync(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)]
    ) throws -> Data {
        assertNotBlockingMainThreadInDebug(fqn)
        let protoCallId = new_function_call()
        let payload = try encodeCallArgs(args, callId: protoCallId)

        let box = ResultBox()
        let semaphore = DispatchSemaphore(value: 0)
        let callbackId = registerPending { result in
            box.store(result)
            semaphore.signal()
        }
        dispatch(fqn, payload: payload, callbackId: callbackId)
        semaphore.wait()
        return try box.take().get()
    }

    private func invokeAsync(
        _ fqn: String,
        args: [(String, (any BamlEncodable)?)]
    ) async throws -> Data {
        let protoCallId = new_function_call()
        let payload = try encodeCallArgs(args, callId: protoCallId)

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let callbackId = registerPending { result in
                    continuation.resume(with: result)
                }
                dispatch(fqn, payload: payload, callbackId: callbackId)
            }
        } onCancel: {
            // Reserve-based cancel: the engine delivers a Cancelled
            // panic through the normal completion path, which resumes
            // the continuation. (Translating that into Swift's
            // CancellationError is the cancellation phase.)
            _ = cancel_function_call(protoCallId)
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

    private func dispatch(_ fqn: String, payload: Data, callbackId: UInt32) {
        // `call_function` fully decodes the args buffer before it
        // returns (verified in bridge_cffi::call_function_inner), so
        // scoping the pointers to this call is sound.
        payload.withUnsafeBytes { buf in
            fqn.withCString { name in
                call_function(
                    name,
                    buf.baseAddress?.assumingMemoryBound(to: UInt8.self),
                    UInt(buf.count),
                    callbackId
                )
            }
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
