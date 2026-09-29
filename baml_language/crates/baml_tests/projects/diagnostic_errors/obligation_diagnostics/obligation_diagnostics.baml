// WHICH report an interface goal produces when it fails or never decides.
//
// The `!error` below is deliberate and correct: it is how a type inference
// could not determine renders, and saying so is a true finding the user
// should see. It is not a leak to be spelled away -- `unknown` is a real
// type (the top type, which a user can write and every value inhabits), so
// naming it here would claim inference SUCCEEDED and landed there. Nor is it
// a reason to drop a report: a cascade is suppressed where the derived goal
// would be REGISTERED, because its cause already reported, never by
// inspecting rendered text for a marker.

// -- Undecided is not ambiguous ------------------------------------------
// Exactly ONE `implements Shown` exists in this file, so no goal here can
// have found more than one way to be proven. This bound goal never decides
// because its subject is a union that never closes -- the unsolved element
// reports at the `[]` -- and it must NOT additionally claim that several
// implementations apply, which was true of nothing.

interface Shown {
    function s(self) -> string throws never
}

class Only { v: int }

implements Shown for Only {
    function s(self) -> string throws never { "only" }
}

function needs_shown<T extends Shown>(t: T) -> int throws never { 0 }

function open_union_is_not_ambiguity(flag: bool) -> int throws never {
    let u = if (flag) { Only { v: 1 } } else { [] };
    needs_shown(u)
}

// Genuine ambiguity, for contrast: two impls each prove `Conv<A>` for `W`
// and nothing in the program picks an `A`. This one IS "type annotations
// needed" (rustc's E0283), and it is the only shape here that says so.

interface Conv<A> {
    function to(self) -> A throws never
}

class W { v: int }

implements Conv<int> for W {
    function to(self) -> int throws never { self.v }
}

implements Conv<string> for W {
    function to(self) -> string throws never { "w" }
}

function needs_conv<A, T extends Conv<A>>(t: T) -> int throws never { 0 }

function genuine_ambiguity(w: W) -> int throws never {
    needs_conv(w)
}

// -- Two faults at one span are two reports ------------------------------
// The upcast fails (`C1` does not implement `A1`) AND its result is the
// wrong interface for the parameter. Both anchor at the same expression, so
// a duplicate filter keyed on the anchor alone drops the second: the user
// fixes the upcast, rebuilds, and only then learns the argument was never a
// `B1` either. The filter is keyed by the interface that failed, so the
// verdict on `A1` silences a restatement about `A1` and nothing else.

interface A1 {
    function a(self) -> int throws never
}

interface B1 {
    function b(self) -> int throws never
}

class C1 { v: int }

function takes_b(b: B1) -> int throws never { 0 }

function two_faults_one_span(c: C1) -> int throws never {
    takes_b(c.as<A1>)
}
