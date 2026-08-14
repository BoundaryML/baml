// Classes share one file and get per-test prefixes (Drc/Iou/Iwd/Ovr/Dmc);
// structural rendering includes class names, so expected strings use the
// prefixed names.

// ─── default (structural) rendering ─────────────────────────────────────────

test "default_renders_primitives" {
    assert.equal(string.from(42), "42")
    assert.equal(string.from(true), "true")
    assert.equal(string.from(3.5), "3.5")
}

test "default_top_level_string_is_bare" {
    // A bare `string` does not implement ToString, so it hits the `_` arm; the
    // top-level rendering is unquoted.
    assert.equal(string.from("hi"), "hi")
}

test "default_renders_array_with_quoted_nested_strings" {
    assert.equal(string.from([1, 2, 3]), "[1, 2, 3]")
    assert.equal(string.from(["a", "b"]), "[\"a\", \"b\"]")
}

class DrcPoint { x int  y int }

function drc_render() -> string {
    string.from(DrcPoint { x: 1, y: 2 })
}

test "default_renders_class_instance_structurally" {
    // A class that does NOT implement ToString renders structurally via the
    // `_` arm.
    assert.equal(drc_render(), "DrcPoint { x: 1, y: 2 }")
}

// ─── override dispatch ───────────────────────────────────────────────────────

class IouPoint {
    x int
    y int
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
        }
    }
}

function iou_render() -> string {
    string.from(IouPoint { x: 1, y: 2 })
}

test "implementor_override_is_used" {
    assert.equal(iou_render(), "(1, 2)")
}

class IwdPoint {
    x int
    y int
    implements baml.ToString {}
}

class IwdPlain { x int  y int }

function iwd_with_default() -> string {
    string.from(IwdPoint { x: 1, y: 2 })
}

function iwd_without() -> string {
    string.from(IwdPlain { x: 1, y: 2 })
}

test "implementor_with_default_body_matches_nonimplementor" {
    // `implements baml.ToString {}` with no override uses the interface's
    // default body — the same structural rendering as the `_` arm.
    assert.equal(iwd_with_default(), "IwdPoint { x: 1, y: 2 }")
    assert.equal(iwd_without(), "IwdPlain { x: 1, y: 2 }")
}

test "builtin_instant_dispatches_through_interface" {
    // `Instant` implements baml.ToString; both a direct call and `string.from`
    // must dispatch to it (and the formatter is infallible for an in-range
    // value).
    assert.equal(baml.time.Instant.epoch().to_string(), "1970-01-01T00:00:00Z")
    assert.equal(string.from(baml.time.Instant.epoch()), "1970-01-01T00:00:00Z")
}

// ─── overrides honored at depth (shared OvrPoint, from POINT_OVERRIDE) ──────

class OvrPoint {
    x int
    y int
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
        }
    }
}

function nov_array_render() -> string {
    string.from([OvrPoint { x: 1, y: 2 }, OvrPoint { x: 3, y: 4 }])
}

test "nested_override_in_array_is_honored" {
    // Elements whose runtime class overrides `to_string` render via the
    // override, not structurally.
    assert.equal(nov_array_render(), "[(1, 2), (3, 4)]")
}

class OvrLine { start OvrPoint  end OvrPoint }

function nov_field_render() -> string {
    string.from(OvrLine { start: OvrPoint { x: 1, y: 2 }, end: OvrPoint { x: 3, y: 4 } })
}

test "nested_override_in_class_field_is_honored" {
    // The outer class renders structurally, but override fields render via the
    // override.
    assert.equal(nov_field_render(), "OvrLine { start: (1, 2), end: (3, 4) }")
}

class OvrMixed { p OvrPoint  n int  q OvrPoint }

function nov_mixed_render() -> string {
    string.from(OvrMixed { p: OvrPoint { x: 1, y: 2 }, n: 99, q: OvrPoint { x: 3, y: 4 } })
}

test "override_and_structural_siblings_stay_aligned" {
    // Mixed override / non-override fields exercise the pre-order counter: the
    // structural `n` must not consume an override result meant for `p` / `q`.
    assert.equal(nov_mixed_render(), "OvrMixed { p: (1, 2), n: 99, q: (3, 4) }")
}

function nov_map_render() -> string {
    let m: map<string, OvrPoint> = { "a": OvrPoint { x: 1, y: 2 } }
    string.from(m)
}

test "nested_override_in_map_is_honored" {
    assert.equal(nov_map_render(), "{\"a\": (1, 2)}")
}

class OvrPath { points OvrPoint[] }

function nov_deep_render() -> string {
    string.from(OvrPath { points: [OvrPoint { x: 1, y: 2 }, OvrPoint { x: 3, y: 4 }] })
}

test "nested_override_inside_array_inside_class" {
    // Override two levels down: a non-overriding class holding an array of
    // overriding elements.
    assert.equal(nov_deep_render(), "OvrPath { points: [(1, 2), (3, 4)] }")
}

class DmcGreeter {
    name string
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "Hello, " + self.name
        }
    }
}

function dmc_render() -> string {
    let g = DmcGreeter { name: "world" }
    g.to_string()
}

test "direct_method_call_on_implementor" {
    // The interface method is also directly callable on a concrete
    // implementor.
    assert.equal(dmc_render(), "Hello, world")
}
