/// A BAML `throws` value surfaced to Swift (the `error` arm of the
/// `BamlOutboundResult` envelope). Mirrors Python's `BamlError`
/// wrapper contract: the thrown BAML value rides along and is
/// decodable into its generated model via `value(as:)`; `className`
/// carries the thrown value's FQN (peeled through any union-throws
/// wrapper); `bamlTrace` carries the engine stack.
public struct BamlError: Error {
    public let message: String
    public let className: String?
    public let bamlTrace: [String]
    /// The raw thrown value. Python decodes eagerly via its typemap
    /// (`.value`); Swift decodes on demand against the expected type.
    public let payload: BamlOutboundValue?

    public init(
        message: String,
        className: String? = nil,
        bamlTrace: [String] = [],
        payload: BamlOutboundValue? = nil
    ) {
        self.message = message
        self.className = className
        self.bamlTrace = bamlTrace
        self.payload = payload
    }

    /// Decode the thrown value as a generated model — the typed analog
    /// of Python's `exc.value` + `isinstance`.
    public func value<T: BamlDecodable>(as type: T.Type) throws -> T {
        guard let payload else {
            throw BamlDecodeError.unsupported("thrown value has no payload")
        }
        return try T._bamlDecode(payload)
    }
}

extension BamlError {
    /// The error the engine raises for an argument it cannot read, a
    /// `baml.errors.InvalidArgument`, for a value the host refuses before
    /// sending it: one `catch` handles both, and `value(as:)` decodes both.
    static func invalidArgument(_ message: String) -> BamlError {
        var field = BamlBridge_Cffi_V1_BamlOutboundMapEntry()
        field.key = "message"
        field.value.stringValue = message
        var cls = BamlBridge_Cffi_V1_BamlValueClass()
        cls.name = "baml.errors.InvalidArgument"
        cls.fields = [field]
        var value = BamlBridge_Cffi_V1_BamlOutboundValue()
        value.classValue = cls
        return bamlError(from: value, trace: [])
    }
}

/// A BAML panic (the `panic` arm of the envelope, non-exit).
public struct BamlPanic: Error {
    public let message: String
    public let className: String?
    public let bamlTrace: [String]
    public let payload: BamlOutboundValue?

    public init(
        message: String,
        className: String? = nil,
        bamlTrace: [String] = [],
        payload: BamlOutboundValue? = nil
    ) {
        self.message = message
        self.className = className
        self.bamlTrace = bamlTrace
        self.payload = payload
    }

    public func value<T: BamlDecodable>(as type: T.Type) throws -> T {
        guard let payload else {
            throw BamlDecodeError.unsupported("panic value has no payload")
        }
        return try T._bamlDecode(payload)
    }
}

/// The bridge's own failure: it could not carry a value it was handed in
/// good faith (an engine handle it could not clone). Never a property of
/// the BAML program.
public struct BamlBridgeFailure: Error, CustomStringConvertible {
    public let message: String

    public var description: String { "BAML bridge failure: \(message)" }
}
