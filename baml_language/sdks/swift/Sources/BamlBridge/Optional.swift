/// Three-state optional-argument slot for BAML functions with
/// defaulted parameters. Mirrors Python's `UNSET` sentinel design:
///
/// - `.unset` — omit the argument entirely, so the engine evaluates
///   the BAML default (`opt1: int? = 5`, `opt2: int? = make_opt2()`).
/// - `.null` — pass an explicit BAML null. Spelled `nil` at call sites
///   (`ExpressibleByNilLiteral`).
/// - `.value(v)` — pass `v`.
///
/// Generated signatures default every optional parameter to `.unset`,
/// so omitting it in Swift and omitting it in Python behave
/// identically.
public enum BamlOptional<Wrapped>: Sendable where Wrapped: Sendable {
    case unset
    case null
    case value(Wrapped)
}

extension BamlOptional: Equatable where Wrapped: Equatable {}
extension BamlOptional: Hashable where Wrapped: Hashable {}

extension BamlOptional: ExpressibleByNilLiteral {
    public init(nilLiteral: ()) {
        self = .null
    }
}

extension BamlOptional where Wrapped: BamlEncodable {
    /// Append this slot to a kwargs array unless it is `.unset` —
    /// the Swift spelling of Python's "`_build_kwargs` drops UNSET".
    public func _appendIfSet(
        _ name: String,
        to args: inout [(String, (any BamlEncodable)?)]
    ) {
        switch self {
        case .unset: return
        case .null: args.append((name, nil))
        case .value(let wrapped): args.append((name, wrapped))
        }
    }
}
