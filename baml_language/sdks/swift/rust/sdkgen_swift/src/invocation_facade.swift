import Foundation
import BamlBridge

extension Baml {
    public typealias CancelToken = Baml.baml.spawn.CancelToken
    public enum Trace {
        public typealias Options = Baml.vendor.trace.Options
        public typealias ReservedSpan = Baml.vendor.trace.ReservedSpan
        public enum Selection: Sendable {
            case options(Options)
            case reserved(ReservedSpan)
            fileprivate var handle: BamlHandle? { switch self { case .options(let value): value._handle; case .reserved(let value): value._handle } }
        }
    }
    public struct BamlOptions: Sendable {
        public let trace: Trace.Selection?
        public let cancel: CancelToken?
        public let timeoutMs: Int64?
        public init(trace: Trace.Selection? = nil, cancel: CancelToken? = nil, timeoutMs: Int64? = nil) { self.trace = trace; self.cancel = cancel; self.timeoutMs = timeoutMs }
        internal func lowered() throws -> BamlInvocationOptions {
            if trace != nil && trace?.handle == nil { throw BamlDecodeError.unsupported("trace selection has no live handle") }
            var encodeCancel: (@Sendable () -> BamlInboundValue)?
            if let token = cancel { encodeCancel = { token._bamlEncode() } }
            return BamlInvocationOptions(trace: trace?.handle, cancel: encodeCancel, timeoutMs: timeoutMs)
        }
    }
    public struct Invocation: Sendable {
        private let captured: BamlInvocationCapture
        private init(_ captured: BamlInvocationCapture) { self.captured = captured }
        public static var current: Invocation? { BamlInvocationCapture.current.map(Invocation.init) }
        public var cancel: CancelToken { try! CancelToken._bamlDecode(captured.cancelValue()) }
        public func run<R>(_ body: () throws -> R) rethrows -> R { try captured.run(body) }
        public func run<R>(_ body: () async throws -> R) async rethrows -> R { try await captured.run(body) }
    }
}
