/// Copy-on-write heap box for recursive struct fields.
///
/// A Swift struct cannot directly contain itself (`struct Node { var
/// next: Node? }` has infinite size — `Optional` stores its wrapped
/// value inline). BAML's recursive classes (`IntBinaryTree`, mutual
/// A/B recursion, larger SCCs) therefore box their cycle-forming
/// fields:
///
/// ```swift
/// public struct IntBinaryTree: Equatable {
///     public var value: Int
///     @BamlIndirect public var left: IntBinaryTree?
///     @BamlIndirect public var right: IntBinaryTree?
/// }
/// ```
///
/// `sdkgen_swift` applies the wrapper automatically to any field whose
/// (optional-stripped) class type can reach the containing class
/// through direct (non-List/Map) references. List/Map fields never
/// need it — their storage is already heap-allocated.
///
/// The box copies on write, so value semantics are preserved: mutating
/// a copy never shows through the original.
@propertyWrapper
public struct BamlIndirect<Value> {
    private final class Box {
        var value: Value
        init(_ value: Value) {
            self.value = value
        }
    }

    private var box: Box

    public init(wrappedValue: Value) {
        box = Box(wrappedValue)
    }

    public var wrappedValue: Value {
        get { box.value }
        set {
            if isKnownUniquelyReferenced(&box) {
                box.value = newValue
            } else {
                box = Box(newValue)
            }
        }
    }
}

extension BamlIndirect: Equatable where Value: Equatable {
    public static func == (lhs: BamlIndirect, rhs: BamlIndirect) -> Bool {
        lhs.wrappedValue == rhs.wrappedValue
    }
}

extension BamlIndirect: Hashable where Value: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(wrappedValue)
    }
}

// The box is only ever mutated through CoW `wrappedValue.set`, which
// enforces unique ownership before writing.
extension BamlIndirect: @unchecked Sendable where Value: Sendable {}
