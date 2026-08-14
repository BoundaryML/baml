import CBamlBridge
import Foundation

/// An engine-owned resource riding in a generated model's `$rust_type`
/// field (`File._handle`, `Response._body`, media `_data`, …). The
/// wire carries only a key into the engine's handle table; the Swift
/// object owns that key: deinit releases it, and encoding clones a
/// fresh key for the wire so this instance stays independently
/// droppable (Python's `_clone_key_for_wire` semantics).
public final class BamlHandle: @unchecked Sendable {
    let key: UInt64
    let handleType: BamlBridge_Cffi_V1_BamlHandleType
    /// Root class identity carried by an outbound tagged handle's `ty`.
    let classFQN: String?

    init(
        key: UInt64,
        handleType: BamlBridge_Cffi_V1_BamlHandleType,
        classFQN: String? = nil
    ) {
        self.key = key
        self.handleType = handleType
        self.classFQN = classFQN
    }

    deinit {
        // Host-value keys live in the per-bridge registry, NOT the
        // engine handle table — releasing them here would evict a
        // numerically-colliding engine entry (same rule as Python's
        // BamlPyHandle::Drop).
        if handleType != .hostValueCallable && handleType != .hostValueOpaque {
            _ = BamlApi.handleRelease(key)
        }
    }
}

extension BamlHandle: Equatable {
    /// Identity is the table key. Two handles to the same resource
    /// minted separately compare unequal — generated structs holding
    /// handles compare by resource identity, mirroring Python where
    /// private handle attrs sit outside pydantic equality.
    public static func == (lhs: BamlHandle, rhs: BamlHandle) -> Bool {
        lhs.key == rhs.key
    }
}

extension BamlHandle: BamlEncodable {
    public func _bamlEncode() -> BamlInboundValue {
        // Clone a fresh key for the wire; the engine drains it while
        // this instance keeps its own.
        var wireKey: UInt64 = 0
        let status = BamlApi.handleClone(key, &wireKey)
        precondition(
            status == BAML_CFFI_STATUS_OK.rawValue,
            "handle_clone failed with status \(status)"
        )
        var handle = BamlBridge_Cffi_V1_BamlHandle()
        handle.key = wireKey
        handle.handleType = handleType
        var v = BamlBridge_Cffi_V1_InboundValue()
        v.handle = handle
        return BamlInboundValue(v)
    }
}

extension BamlHandle: BamlDecodable {
    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> BamlHandle {
        let raw = value.normalized
        guard case .handleValue(let handle) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "handle", got: wireArmName(raw))
        }
        guard handle.handleType != .handleUnspecified else {
            throw BamlDecodeError.typeMismatch(expected: "tagged handle", got: "HANDLE_UNSPECIFIED")
        }
        let classFQN: String?
        if handle.hasTy, case .classTy(let classType)? = handle.ty.ty {
            classFQN = classType.name.isEmpty ? nil : classType.name
        } else {
            classFQN = nil
        }
        return BamlHandle(
            key: handle.key,
            handleType: handle.handleType,
            classFQN: classFQN
        )
    }
}

extension Optional where Wrapped == BamlHandle {
    /// Missing/null handle fields decode as nil via the standard
    /// Optional conformance; nothing extra needed — this extension
    /// exists only for documentation symmetry.
}

/// An owned BAML closure returned to Swift.
public final class BamlFunctionHandle: @unchecked Sendable {
    private let handle: BamlHandle
    public let parameterNames: [String]

    private init(handle: BamlHandle, parameterNames: [String]) {
        self.handle = handle
        self.parameterNames = parameterNames
    }

    public static func decode(_ value: BamlOutboundValue) throws -> BamlFunctionHandle {
        let raw = value.normalized
        guard case .handleValue(let outbound) = raw.value,
              outbound.handleType == .functionRef,
              outbound.key != 0
        else {
            throw BamlDecodeError.typeMismatch(expected: "function", got: wireArmName(raw))
        }
        let handle = BamlHandle(key: outbound.key, handleType: outbound.handleType)
        guard outbound.hasTy,
              case .function(let functionType)? = outbound.ty.ty
        else {
            throw BamlDecodeError.typeMismatch(expected: "function", got: wireArmName(raw))
        }
        let names = try functionType.params.enumerated().map { index, parameter in
            guard parameter.hasName, !parameter.name.isEmpty else {
                throw BamlDecodeError.unsupported("returned function parameter \(index) has no name")
            }
            return parameter.name
        }
        return BamlFunctionHandle(
            handle: handle,
            parameterNames: names
        )
    }

    public func callRaw(
        args: [(String, (any BamlEncodable)?)]
    ) async throws -> BamlOutboundValue {
        try await BamlRuntime.shared.callHandleRaw(handle.key, args: args)
    }
}
