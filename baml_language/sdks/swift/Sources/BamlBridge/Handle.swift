import CBamlBridge
import Foundation

/// An engine-owned resource riding in a generated model's `$rust_type`
/// field (`File._handle`, `Response._body`, media `_data`, …). The
/// wire carries only a key into the engine's handle table; the Swift
/// object owns one release of that key: deinit performs it, and
/// encoding clones one more ownership for the wire (a fresh key for
/// media, the same refcounted key for an engine-heap handle) so this
/// instance stays independently droppable (Python's
/// `_clone_key_for_wire` semantics).
public final class BamlHandle: @unchecked Sendable {
    let key: UInt64
    let handleType: BamlBridge_Cffi_V1_BamlHandleType
    /// Root class identity carried by an outbound tagged handle's `ty`.
    let classFQN: String?
    /// Media handles cache their portable wire value when they are created or
    /// decoded. Encoding never has to perform a fallible native read.
    let portableMedia: BamlBridge_Cffi_V1_BamlValueMedia?

    init(
        key: UInt64,
        handleType: BamlBridge_Cffi_V1_BamlHandleType,
        classFQN: String? = nil,
        portableMedia: BamlBridge_Cffi_V1_BamlValueMedia? = nil
    ) {
        self.key = key
        self.handleType = handleType
        self.classFQN = classFQN
        self.portableMedia = portableMedia
    }

    deinit {
        // Host-value keys live in the per-bridge registry, NOT the
        // engine handle table — releasing them here would evict a
        // numerically-colliding engine entry (same rule as Python's
        // BamlPyHandle::Drop).
        switch keySpace {
        case .engine: _ = BamlApi.handleRelease(key)
        case .host: break
        }
    }
}

extension BamlHandle: Equatable {
    /// Identity is the key within its key space. An engine handle's key is
    /// a row of the engine's handle table, which issues one key per heap
    /// object, so two handles to the same object compare EQUAL however they
    /// were minted; identity-free rows (media) get a key per minting and
    /// compare unequal even for the same resource. A host value's key is a
    /// registration of this host's own, numbered independently, so it never
    /// equals an engine handle with the same number. Generated structs
    /// holding handles compare by this identity, mirroring Python where
    /// private handle attrs sit outside pydantic equality.
    public static func == (lhs: BamlHandle, rhs: BamlHandle) -> Bool {
        lhs.keySpace == rhs.keySpace && lhs.key == rhs.key
    }
}

extension BamlHandle {
    /// The two independent key spaces a handle's key belongs to.
    enum KeySpace: Equatable {
        /// A row of the engine's handle table.
        case engine
        /// A registration in this host's own registry.
        case host
    }

    var keySpace: KeySpace {
        Self.keySpace(of: handleType)
    }

    static func keySpace(of handleType: BamlBridge_Cffi_V1_BamlHandleType) -> KeySpace {
        switch handleType {
        case .hostValueCallable, .hostValueOpaque:
            return .host
        case .handleUnspecified, .untaggedRustData, .untaggedBexHeap, .functionRef, .adtMediaImage,
            .adtMediaAudio, .adtMediaVideo, .adtMediaPdf, .adtMediaGeneric, .adtPromptAst, .adtType,
            .adtTaggedHeapHandle, .adtFunctionSpec, .adtRuntimeValue, .invocationState,
            .traceReservation, .UNRECOGNIZED:
            return .engine
        }
    }
}

/// A handle states nothing: its type is the engine's, checked live. It has
/// no Swift-side type of its own, so it is not `BamlTyped`; a `$rust_type`
/// field's slot is the field's declared type.
extension BamlHandle: BamlLeafEncodable {
    public func _bamlEncode(as _: BamlSlot<BamlHandle>) throws -> BamlInboundValue {
        if let portableMedia {
            return BamlMedia.encodePortable(portableMedia)
        }
        // A host value is the engine's reference to this host's own object:
        // its key goes as it is. An engine handle clones a fresh key for the
        // wire; the engine drains it while this instance keeps its own.
        var wireKey: UInt64 = key
        switch keySpace {
        case .engine:
            let status = BamlApi.handleClone(key, &wireKey)
            guard status == BAML_CFFI_STATUS_OK.rawValue else {
                throw BamlBridgeFailure(message: "handle_clone failed with status \(status)")
            }
        case .host:
            break
        }
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
        if case .mediaValue(let media) = raw.value {
            return try BamlMedia.decodePortable(media)
        }
        guard case .handleValue(let handle) = raw.value else {
            throw BamlDecodeError.typeMismatch(expected: "handle", got: wireArmName(raw))
        }
        guard handle.handleType != .handleUnspecified else {
            throw BamlDecodeError.typeMismatch(expected: "tagged handle", got: "HANDLE_UNSPECIFIED")
        }
        // An anonymous label is display text, never an identity.
        let classFQN: String?
        if handle.hasTy, case .classTy(let classType)? = handle.ty.ty,
           case .name(let fqn)? = classType.head, !fqn.isEmpty
        {
            classFQN = fqn
        } else {
            classFQN = nil
        }
        let portableMedia: BamlBridge_Cffi_V1_BamlValueMedia?
        if BamlMedia.isPortableHandle(handle.handleType) {
            portableMedia = try BamlMedia.snapshotPortable(
                key: handle.key,
                handleType: handle.handleType
            )
        } else {
            portableMedia = nil
        }
        return BamlHandle(
            key: handle.key,
            handleType: handle.handleType,
            classFQN: classFQN,
            portableMedia: portableMedia
        )
    }
}

extension BamlHandle {
    /// The engine's reference to an instance of the opaque standard-library
    /// class `fqn` (`reflect.Package`, `sap._ParseCache<T>`, …) at exactly
    /// `typeArguments`: an `ADT_RUNTIME_VALUE` handle whose type names that
    /// class at those arguments. A generated opaque type holds it and sends it
    /// back as itself; a host never reads the instance's fields.
    public static func _bamlDecodeOpaque(
        _ value: BamlOutboundValue,
        of fqn: String,
        typeArguments: [BamlTypeDescriptor] = []
    ) throws -> BamlHandle {
        let handle = try _bamlDecode(value)
        guard handle.handleType == .adtRuntimeValue, handle.classFQN == fqn else {
            throw BamlDecodeError.typeMismatch(
                expected: "\(fqn) instance (runtime value handle)",
                got: "handle type \(handle.handleType) of \(handle.classFQN ?? "no class")"
            )
        }
        // Generic arguments are invariant: an instance at other arguments is
        // not this type. Compared as the engine states them.
        guard case .handleValue(let outbound) = value.normalized.value,
              case .classTy(let classType)? = outbound.ty.ty,
              classType.typeArgs.map(BamlTypeDescriptor.init) == typeArguments
        else {
            throw BamlDecodeError.typeMismatch(
                expected: "\(fqn) instance at its type arguments",
                got: "an instance at other type arguments"
            )
        }
        return handle
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
        // A parameter the function's type leaves nameless is bound by the
        // name the engine gives it, its position's.
        let names = functionType.params.enumerated().map { index, parameter in
            parameter.hasName && !parameter.name.isEmpty ? parameter.name : "arg\(index)"
        }
        return BamlFunctionHandle(
            handle: handle,
            parameterNames: names
        )
    }

    public func callRaw(
        args: [BamlArgument],
        baml: BamlInvocationOptions = .init()
    ) async throws -> BamlOutboundValue {
        try await BamlRuntime.shared.callHandleRaw(handle.key, args: args, baml: baml)
    }
    public func callRawSync(args: [BamlArgument], baml: BamlInvocationOptions = .init()) throws -> BamlOutboundValue {
        try BamlRuntime.shared.callHandleRawSync(handle.key, args: args, baml: baml)
    }

}
