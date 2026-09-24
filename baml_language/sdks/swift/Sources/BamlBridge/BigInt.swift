/// An arbitrary-precision BAML integer represented by canonical signed
/// lowercase hexadecimal text. The representation matches the bridge wire and
/// avoids narrowing through Swift's fixed-width `Int`.
public struct BamlBigInt: Equatable, Hashable, Sendable, CustomStringConvertible {
    /// Canonical signed lowercase hexadecimal text without a `0x` prefix.
    public let hexadecimal: String

    /// Construct an arbitrary-precision integer from canonical bridge-format
    /// hexadecimal text (`0`, `a`, or `-a`, for example).
    public init(hexadecimal: String) throws {
        guard Self.isCanonical(hexadecimal) else {
            throw BamlBigIntError.noncanonicalHexadecimal(hexadecimal)
        }
        self.hexadecimal = hexadecimal
    }

    /// Construct a BAML bigint from a fixed-width Swift integer.
    public init(_ value: Int) {
        if value < 0 {
            hexadecimal = "-" + String(value.magnitude, radix: 16)
        } else {
            hexadecimal = String(value, radix: 16)
        }
    }

    public var description: String { hexadecimal }

    private static func isCanonical(_ value: String) -> Bool {
        guard !value.isEmpty, value.first != "+" else { return false }
        let negative = value.first == "-"
        let digits = negative ? value.dropFirst() : value[...]
        guard !digits.isEmpty else { return false }
        guard digits.allSatisfy({ ("0"..."9").contains(String($0)) || ("a"..."f").contains(String($0)) }) else {
            return false
        }
        if digits.count > 1 && digits.first == "0" { return false }
        return !(negative && digits == "0")
    }
}

/// Construction failures for malformed public bigint representations.
public enum BamlBigIntError: Error, Equatable, CustomStringConvertible {
    case noncanonicalHexadecimal(String)

    public var description: String {
        switch self {
        case .noncanonicalHexadecimal(let value):
            return "BAML bigint must use canonical signed lowercase hexadecimal text; got \(value)"
        }
    }
}
