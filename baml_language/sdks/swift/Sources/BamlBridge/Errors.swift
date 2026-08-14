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
