import Foundation
import CBamlBridge

/// Private lowering used by a generated SDK's BamlOptions.
public struct BamlInvocationOptions: Sendable {
    let trace: BamlHandle?
    let cancel: (@Sendable () -> BamlInboundValue)?
    let timeoutMs: Int64?
    public init(trace: BamlHandle? = nil, cancel: (@Sendable () -> BamlInboundValue)? = nil, timeoutMs: Int64? = nil) {
        self.trace = trace; self.cancel = cancel; self.timeoutMs = timeoutMs
    }
}

public final class BamlInvocationCapture: @unchecked Sendable {
    let state: BamlHandle
    private let cancel: BamlBridge_Cffi_V1_BamlOutboundValue
    init(state: BamlHandle, cancel: BamlBridge_Cffi_V1_BamlOutboundValue) { self.state = state; self.cancel = cancel }
    deinit { releaseControlValue(cancel) }
    public static var current: BamlInvocationCapture? { HostInvocationScope.current }
    public func cancelValue() throws -> BamlOutboundValue { try BamlOutboundValue(cloneControlValue(cancel)) }
    public func run<R>(_ body: () throws -> R) rethrows -> R { try HostInvocationScope.$current.withValue(self, operation: body) }
    public func run<R>(_ body: () async throws -> R) async rethrows -> R { try await HostInvocationScope.$current.withValue(self, operation: body) }
}

func cloneControlValue(_ value: BamlBridge_Cffi_V1_BamlOutboundValue) throws -> BamlBridge_Cffi_V1_BamlOutboundValue {
    var owned: [UInt64] = []
    func clone(_ value: BamlBridge_Cffi_V1_BamlOutboundValue) throws -> BamlBridge_Cffi_V1_BamlOutboundValue {
        var output = value
        switch value.value {
        case .handleValue(var handle) where handle.handleType != .hostValueCallable && handle.handleType != .hostValueOpaque:
            var key: UInt64 = 0
            guard BamlApi.handleClone(handle.key, &key) == BAML_CFFI_STATUS_OK.rawValue else { throw BamlDecodeError.unsupported("inactive invocation handle") }
            owned.append(key); handle.key = key; output.handleValue = handle
        case .classValue(var cls):
            cls.fields = try cls.fields.map { entry in var field = entry; field.value = try clone(entry.value); return field }; output.classValue = cls
        case .listValue(var list): list.items = try list.items.map(clone); output.listValue = list
        case .mapValue(var map): map.entries = try map.entries.map { entry in var field = entry; field.value = try clone(entry.value); return field }; output.mapValue = map
        case .unionVariantValue(var variant): variant.value = try clone(variant.value); output.unionVariantValue = variant
        default: break
        }
        return output
    }
    do { return try clone(value) } catch { for key in owned { _ = BamlApi.handleRelease(key) }; throw error }
}

extension BamlRuntime {
    public func currentTraceContext<R: BamlDecodable>() throws -> R {
        var buffer = BamlBuffer()
        guard BamlApi.invocationContext(HostInvocationScope.current?.state.key ?? 0, &buffer) == BAML_CFFI_STATUS_OK.rawValue else { throw BamlDecodeError.unsupported("inactive invocation") }
        return try R._bamlDecode(BamlOutboundValue(BamlBridge_Cffi_V1_BamlOutboundValue(serializedBytes: BamlApi.takeBuffer(buffer))))
    }
}
