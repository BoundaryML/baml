// A bound list is a CONJUNCTION: an argument for `T extends A & B` must satisfy
// EVERY conjunct. CST lowering used to keep only the first, so a type satisfying
// `A` alone was silently accepted; each case below must now be rejected, and
// each must name the conjunct that is missing.
//
// Every declaration site that accepts bounds is covered, plus the two
// order-sensitivity cases (leading conjunct missing, trailing conjunct missing)
// that prove this is a conjunction rather than "the last bound wins".
// See `compiles/generic_intersection_bounds` for the accepted shapes.

interface Named {
    name: string
}

interface Sized {
    function size(self) -> int throws never
}

class OnlyNamed {
    name: string

    implements Named {}
}

class OnlySized {
    implements Sized {
        function size(self) -> int { 1 }
    }
}

function describe<T extends Named & Sized>(item: T) -> string {
    item.name
}

// The trailing conjunct is unsatisfied — the case a dropped `& Sized` hid.
function missing_trailing_conjunct() -> string {
    describe<OnlyNamed>(OnlyNamed { name: "n" })
}

// The leading conjunct is unsatisfied, so `Named` is enforced too.
function missing_leading_conjunct() -> string {
    describe<OnlySized>(OnlySized {})
}

// Referencing the generic as a *value* enforces the conjunction as well.
function missing_conjunct_in_instantiation_expr() -> string {
    let f = describe<OnlyNamed>
    "ok"
}

// Declaration-site bounds on a type constructor are checked at every use.
class Holder<T extends Named & Sized> {
    item: T
}

function missing_conjunct_in_class_type_arg() -> string {
    let h = Holder<OnlyNamed> { item: OnlyNamed { name: "n" } }
    h.item.name
}

// An out-of-body `implements` block applies only to types satisfying every
// conjunct of its bound list, so `OnlyNamed` gains no `Printable`.
interface Printable {
    function display(self) -> string throws never
}

implements<T extends Named & Sized> Printable for T {
    function display(self) -> string { self.name }
}

function blanket_impl_needs_every_conjunct() -> string {
    let p: Printable = OnlyNamed { name: "n" }
    p.display()
}

// An implementation may not require more than the interface declares, so
// widening `Named` to `Named & Sized` on an override is a conformance error.
interface Reader {
    function read<T extends Named>(self, value: T) -> string throws never
}

class WideningReader {
    implements Reader {
        function read<T extends Named & Sized>(self, value: T) -> string {
            value.name
        }
    }
}

// Two conjuncts declaring the same member are siblings — neither shadows the
// other — so the access is ambiguous and must be qualified with `.as<I>`.
// Resolving conjunct-by-conjunct and taking the first hit would silently pick
// `Sized` here, and which one it picked would depend on the order written.
interface OtherSized {
    function size(self) -> int throws never
}

function ambiguous_across_conjuncts<T extends Sized & OtherSized>(item: T) -> int {
    item.size()
}

// A conjunct overlapping through `requires` is NOT ambiguous: both reach the
// same realized interface, so it dedupes to one declarer.
interface NamedAndSized requires Named {
    function extra(self) -> int throws never
}

function overlap_through_requires<T extends Named & NamedAndSized>(item: T) -> string {
    item.name
}
