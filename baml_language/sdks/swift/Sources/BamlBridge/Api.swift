import CBamlBridge
import Foundation

/// The resolved `BamlApiV1` function table — the canonical bridge ABI.
///
/// `baml_get_api_v1()` is the only symbol a host bridge relies on; every
/// other native entry point is a function pointer in the returned table.
/// The table is immutable runtime-owned storage, valid for the life of
/// the process (we link the runtime statically, so it can never unload).
///
/// V1 is append-only: fields beyond the original prefix may exist in a
/// newer runtime (`struct_size` grows), and a truncated prefix means an
/// incompatible library — both are checked once here, at first use.
/// All V1-prefix function pointers are required, so they are unwrapped
/// once into non-optional members.
enum BamlApi {
    private static let v1: BamlApiV1 = {
        guard let ptr = baml_get_api_v1() else {
            fatalError("baml_get_api_v1() returned null — incompatible BAML native library")
        }
        guard baml_api_v1_is_compatible(ptr) else {
            fatalError(
                "BAML native library is not V1-compatible "
                    + "(abi_version \(ptr.pointee.abi_version), struct_size \(ptr.pointee.struct_size))"
            )
        }
        return ptr.pointee
    }()

    static let version = v1.version!
    static let initializeRuntimeFromBytecode = v1.initialize_runtime_from_bytecode!
    static let freeBuffer = v1.free_buffer!
    static let registerCallback = v1.register_callback!
    static let callFunction = v1.call_function!
    static let newFunctionCall = v1.new_function_call!
    static let cancelFunctionCall = v1.cancel_function_call!
    static let registerHostDispatchCallback = v1.register_host_dispatch_callback!
    static let registerHostReleaseCallback = v1.register_host_release_callback!
    static let completeHostCall = v1.complete_host_call!
    static let handleClone = v1.handle_clone!
    static let handleRelease = v1.handle_release!
    static let mediaFromUrl = v1.media_from_url!
    static let mediaFromFile = v1.media_from_file!
    static let mediaFromBase64 = v1.media_from_base64!
    static let mediaUrl = v1.media_url!
    static let mediaFile = v1.media_file!
    static let mediaBase64 = v1.media_base64!
    static let mediaMimeType = v1.media_mime_type!
    static let registerBridge = v1.register_bridge!

    /// Copy a runtime-owned buffer to a `Data` and release it exactly
    /// once via the table's `free_buffer`. A zero-length buffer may have
    /// a null or non-null pointer and must still be released.
    static func takeBuffer(_ buffer: BamlBuffer) -> Data {
        defer { freeBuffer(buffer) }
        guard let ptr = buffer.ptr, buffer.len > 0 else { return Data() }
        return Data(bytes: ptr, count: Int(buffer.len))
    }
}
