// Associated-type projection patterns are real runtime tests:
// `Self.<member>` in an interface default body collapses to the member's
// frame slot, and a bounded-var projection (`T.Item`) lowers to a structured
// projection template the VM reduces through the impl registry at test time.
// Reduction is total for every declared member: the baked impl rules carry a
// binding for each one — the impl's pin, or the interface's declared default
// realized at that impl. A defaulted member is symbolic under a bound exactly
// like a non-defaulted one (a bare `T extends I` pins nothing), so its tests
// dispatch per-receiver: an impl that leaves it to the default matches the
// default, and an impl that pins it differently matches its pin.

interface Holder {
    type Item

    function get(self) -> Self.Item throws never

    function is_item(self, v: unknown) -> bool throws never {
        v is Self.Item
    }

    function classify(self, v: unknown) -> string throws never {
        match (v) {
            let x: Self.Item => "item",
            _ => "other",
        }
    }
}

class IntHolder {
    implements Holder {
        type Item = int
        function get(self) -> int throws never {
            7
        }
    }
}

class StrHolder {
    implements Holder {
        type Item = string
        function get(self) -> string throws never {
            "s"
        }
    }
}

function is_item_of<T extends Holder>(h: T, v: unknown) -> bool {
    v is T.Item
}

function classify_for<T extends Holder>(h: T, v: unknown) -> string {
    match (v) {
        let x: T.Item => "item",
        _ => "other",
    }
}

// ── Self.<member> (interface default body: the assoc frame slot) ────────────

function self_item_accepts_own_fn() -> bool {
    IntHolder {  }.is_item(5)
}

test "projection_self_item_accepts_own" {
    assert.is_true(self_item_accepts_own_fn())
}

function self_item_rejects_other_fn() -> bool {
    IntHolder {  }.is_item("x")
}

test "projection_self_item_rejects_other" {
    assert.is_true(!self_item_rejects_other_fn())
}

function self_item_per_receiver_fn() -> bool {
    StrHolder {  }.is_item("x")
}

test "projection_self_item_per_receiver" {
    assert.is_true(self_item_per_receiver_fn())
}

function self_item_match_arm_fn() -> string {
    IntHolder {  }.classify(5)
}

test "projection_self_item_match_arm" {
    assert.equal(self_item_match_arm_fn(), "item")
}

function self_item_match_falls_through_fn() -> string {
    IntHolder {  }.classify("x")
}

test "projection_self_item_match_falls_through" {
    assert.equal(self_item_match_falls_through_fn(), "other")
}

// ── T.<member> (structured projection, reduced through the registry) ────────

function bound_projection_accepts_fn() -> bool {
    is_item_of(IntHolder {  }, 5)
}

test "projection_bound_var_accepts" {
    assert.is_true(bound_projection_accepts_fn())
}

function bound_projection_rejects_fn() -> bool {
    is_item_of(IntHolder {  }, "x")
}

test "projection_bound_var_rejects" {
    assert.is_true(!bound_projection_rejects_fn())
}

function bound_projection_per_receiver_fn() -> bool {
    is_item_of(StrHolder {  }, "x")
}

test "projection_bound_var_per_receiver" {
    assert.is_true(bound_projection_per_receiver_fn())
}

function bound_projection_match_arm_fn() -> string {
    classify_for(StrHolder {  }, "x")
}

test "projection_bound_var_match_arm" {
    assert.equal(bound_projection_match_arm_fn(), "item")
}

function bound_projection_match_falls_through_fn() -> string {
    classify_for(StrHolder {  }, 5)
}

test "projection_bound_var_match_falls_through" {
    assert.equal(bound_projection_match_falls_through_fn(), "other")
}

// ── Defaulted members: symbolic under bounds, per-receiver at runtime ───────

interface Labeled {
    type Label = string

    function id(self) -> int throws never

    function is_label(self, v: unknown) -> bool throws never {
        v is Self.Label
    }

    function label_name(self) -> string throws never {
        reflect.type_of<Self.Label>().to_string()
    }
}

// Leaves Label to its declared default (string).
class Plain {
    implements Labeled {
        function id(self) -> int throws never {
            1
        }
    }
}

// Pins Label = int, overriding the default.
class Pinned {
    implements Labeled {
        type Label = int
        function id(self) -> int throws never {
            2
        }
    }
}

function is_label_of<T extends Labeled>(t: T, v: unknown) -> bool {
    v is T.Label
}

function default_label_accepts_fn() -> bool {
    Plain {  }.is_label("x")
}

test "projection_defaulted_member_default_accepts" {
    assert.is_true(default_label_accepts_fn())
}

function default_label_rejects_fn() -> bool {
    Plain {  }.is_label(5)
}

test "projection_defaulted_member_default_rejects" {
    assert.is_true(!default_label_rejects_fn())
}

function pinned_label_accepts_fn() -> bool {
    Pinned {  }.is_label(5)
}

test "projection_defaulted_member_pin_overrides" {
    assert.is_true(pinned_label_accepts_fn())
}

function pinned_label_rejects_default_fn() -> bool {
    Pinned {  }.is_label("x")
}

test "projection_defaulted_member_pin_rejects_default" {
    assert.is_true(!pinned_label_rejects_default_fn())
}

function default_label_reflects_fn() -> string {
    Plain {  }.label_name()
}

test "projection_defaulted_member_reflects_default" {
    assert.equal(default_label_reflects_fn(), "string")
}

function pinned_label_reflects_fn() -> string {
    Pinned {  }.label_name()
}

test "projection_defaulted_member_reflects_pin" {
    assert.equal(pinned_label_reflects_fn(), "int")
}

function bound_defaulted_default_fn() -> bool {
    is_label_of(Plain {  }, "x") && !is_label_of(Plain {  }, 5)
}

test "projection_defaulted_member_bound_var_default" {
    assert.is_true(bound_defaulted_default_fn())
}

function bound_defaulted_pin_fn() -> bool {
    is_label_of(Pinned {  }, 5) && !is_label_of(Pinned {  }, "x")
}

test "projection_defaulted_member_bound_var_pin" {
    assert.is_true(bound_defaulted_pin_fn())
}
