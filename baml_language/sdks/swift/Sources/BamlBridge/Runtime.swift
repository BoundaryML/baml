import CBamlBridge
import Foundation

/// Entry points into the native BAML runtime (the `CBamlBridge` C ABI).
///
/// Phase 0 stub: only `nativeVersion()` is wired — it proves the whole
/// link chain (SwiftPM → XCFramework → Rust staticlib) end to end.
/// Runtime initialization, `call`/`callSync`, and the completion-callback
/// plumbing land in Phase 1.
public enum BamlRuntime {
    /// Version string reported by the native bridge.
    public static func nativeVersion() -> String {
        let buf = version()
        defer { free_buffer(buf) }
        guard let ptr = buf.ptr, buf.len > 0 else { return "" }
        let data = Data(bytes: ptr, count: Int(buf.len))
        return String(decoding: data, as: UTF8.self)
    }
}
