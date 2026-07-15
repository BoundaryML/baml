/// A BAML `throws` value surfaced to Swift (the `error` arm of the
/// `BamlOutboundResult` envelope). Phase 0 stub — the decoded payload,
/// trace, and the `baml.errors.TypeMismatch` special case land in
/// Phase 1.
public struct BamlError: Error {
    public let message: String
    public let className: String?
    public let bamlTrace: [String]

    public init(message: String, className: String? = nil, bamlTrace: [String] = []) {
        self.message = message
        self.className = className
        self.bamlTrace = bamlTrace
    }
}

/// A BAML panic (the `panic` arm of the envelope, non-exit).
public struct BamlPanic: Error {
    public let message: String
    public let className: String?
    public let bamlTrace: [String]

    public init(message: String, className: String? = nil, bamlTrace: [String] = []) {
        self.message = message
        self.className = className
        self.bamlTrace = bamlTrace
    }
}
