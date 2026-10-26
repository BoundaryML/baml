// reflect.Class.instantiate<T>(fields) builds an instance of the class `T`
// from field values keyed by declared name, and reflect.Enum.value_of<T>(name)
// gives the variant of the enum `T` with that declared name. Both keep the
// given values exactly: no JSON round trip, no copy, no conversion.

enum Team {
    Billing,
    Technical @alias("tech"),
    Hidden @skip,
}

class Ticket {
    team: Team,
    urgent: bool,
}

class Box<T> {
    value: T,
}

class Choice {
    pick: Team | "Technical",
}

class Measure {
    value: float,
}

class Blob {
    bytes: uint8array,
}

class Inner {
    n: int,

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws never {
            { "n": 999 }
        }
    }
}

class Outer {
    inner: Inner,
}

class Loose {
    note: string?,
    data: baml.json.json,
    anything: unknown,
    required: int,
}

class Labeled {
    name: string @alias("full_name"),
    secret: string @skip,
}

interface Named {
    name: string,
}

// ─── instantiate: building instances ─────────────────────────────────────────

function ticket_from_static() -> Ticket {
    reflect.Class.instantiate<Ticket>({ "team": Team.Technical, "urgent": true })
}

test "instantiate builds a static class" {
    let ticket = ticket_from_static();
    assert.equal(ticket.team, Team.Technical);
    assert.is_true(ticket.urgent)
}

function int_box() -> Box<int> {
    reflect.Class.instantiate<Box<int>>({ "value": 3 })
}

function int_box_given(value: unknown) -> string {
    let built = reflect.Class.instantiate<Box<int>>({ "value": value }) catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    "built"
}

test "instantiate builds a generic class against its type arguments" {
    let built: unknown = int_box();
    assert.is_true(built is Box<int>);
    assert.is_true(!(built is Box<string>));
    assert.equal(int_box().value, 3);
    assert.equal(
        int_box_given("three"),
        "reflect.Class.instantiate: field `reflect_instantiate.Box<int>.value` expects `int`, got `string`",
    )
}

function runtime_point() -> reflect.Class {
    reflect.Class.new("Point", { "x": int.type(), "label": string.type() })
}

function runtime_instance(cls: reflect.Class) -> unknown {
    let type P = cls;
    reflect.Class.instantiate<P>({ "x": 1, "label": "origin" })
}

test "instantiate builds a class created at runtime" {
    let cls = runtime_point();
    let point = runtime_instance(cls);
    assert.is_true(reflect.Type.of_value(point) == cls.as_type());
    assert.equal(reflect.Class.get_field<int>(point, "x"), 1);
    assert.equal(reflect.Class.get_field<string>(point, "label"), "origin")
}

function from_type_value(ty: reflect.Type, fields: map<string, unknown>) -> unknown {
    let type T = ty;
    reflect.Class.instantiate<T>(fields)
}

test "instantiate builds a class reached through let type" {
    let built = from_type_value(reflect.Type.of<Ticket>(), { "team": Team.Billing, "urgent": false });
    assert.is_true(built is Ticket);
    let ticket: Ticket = built else {
        throw "not a Ticket"
    };
    assert.equal(ticket.team, Team.Billing);
    assert.is_true(!(ticket.urgent))
}

// ─── instantiate: values are kept as given ───────────────────────────────────

function pick_of(value: Team | "Technical") -> Team | "Technical" {
    reflect.Class.instantiate<Choice>({ "pick": value }).pick
}

test "instantiate keeps an enum value apart from an equal string literal" {
    let enum_pick: unknown = pick_of(Team.Technical);
    assert.is_true(enum_pick is Team);
    assert.equal(enum_pick, Team.Technical);
    let literal_pick: unknown = pick_of("Technical");
    assert.is_true(literal_pick is string);
    assert.is_true(!(literal_pick is Team));
    assert.equal(literal_pick, "Technical")
}

function nan_measure() -> Measure {
    reflect.Class.instantiate<Measure>({ "value": float.nan() })
}

test "instantiate keeps a NaN float" {
    assert.is_true(nan_measure().value.is_nan())
}

function blob_of(bytes: uint8array) -> Blob {
    reflect.Class.instantiate<Blob>({ "bytes": bytes })
}

test "instantiate keeps a uint8array" {
    let blob = blob_of(b"\x00\xffbytes");
    assert.equal(blob.bytes, b"\x00\xffbytes");
    assert.equal(blob.bytes.length(), 7)
}

function shared_inner() -> int {
    let inner = Inner { n: 1 };
    let outer = reflect.Class.instantiate<Outer>({ "inner": inner });
    // The outer instance holds this very object, not a copy and not a
    // value rebuilt from `to_json`.
    inner.n = 2;
    outer.inner.n
}

test "instantiate stores a nested instance as given without calling to_json" {
    assert.equal(shared_inner(), 2)
}

function int_for_float() -> string {
    let built = reflect.Class.instantiate<Measure>({ "value": 1 }) catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    "built"
}

test "instantiate does not convert an int to a float" {
    assert.equal(
        int_for_float(),
        "reflect.Class.instantiate: field `reflect_instantiate.Measure.value` expects `float`, got `int`",
    )
}

// ─── instantiate: missing fields, unknown keys, alias and skip ───────────────

function loose() -> Loose {
    reflect.Class.instantiate<Loose>({ "required": 1 })
}

test "instantiate fills a missing field that accepts null with null" {
    let built = loose();
    assert.equal(built.note, null);
    assert.equal(built.data, null);
    assert.equal(built.anything, null);
    assert.equal(built.required, 1)
}

function instantiate_ticket(fields: map<string, unknown>) -> string {
    let built = reflect.Class.instantiate<Ticket>(fields) catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    "built"
}

test "instantiate rejects a value of the wrong type and names the field" {
    assert.equal(
        instantiate_ticket({ "team": Team.Billing, "urgent": 1 }),
        "reflect.Class.instantiate: field `reflect_instantiate.Ticket.urgent` expects `bool`, got `int`",
    );
    assert.equal(
        instantiate_ticket({ "team": "Billing", "urgent": true }),
        "reflect.Class.instantiate: field `reflect_instantiate.Ticket.team` expects `reflect_instantiate.Team`, got `string`",
    )
}

test "instantiate rejects a missing required field" {
    assert.equal(
        instantiate_ticket({ "team": Team.Billing }),
        "reflect.Class.instantiate: missing field `reflect_instantiate.Ticket.urgent` of type `bool`",
    )
}

test "instantiate rejects a key that names no field" {
    assert.equal(
        instantiate_ticket({ "team": Team.Billing, "urgent": true, "colour": "red" }),
        "reflect.Class.instantiate: class `reflect_instantiate.Ticket` has no field `colour`",
    )
}

function labeled(fields: map<string, unknown>) -> string {
    let built = reflect.Class.instantiate<Labeled>(fields) catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    built.name + "/" + built.secret
}

test "instantiate keys fields by declared name and ignores alias and skip" {
    assert.equal(labeled({ "name": "Ada", "secret": "s" }), "Ada/s");
    assert.equal(
        labeled({ "name": "Ada", "full_name": "Ada", "secret": "s" }),
        "reflect.Class.instantiate: class `reflect_instantiate.Labeled` has no field `full_name`",
    );
    assert.equal(
        labeled({ "name": "Ada" }),
        "reflect.Class.instantiate: missing field `reflect_instantiate.Labeled.secret` of type `string`",
    )
}

function instantiate_non_class(ty: reflect.Type) -> string {
    let type T = ty;
    let built = reflect.Class.instantiate<T>({}) catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    "built"
}

test "instantiate rejects a type that is not a class" {
    assert.equal(
        instantiate_non_class(int.type()),
        "reflect.Class.instantiate expects a class type, got `int`",
    );
    assert.equal(
        instantiate_non_class(Team.type()),
        "reflect.Class.instantiate expects a class type, got `reflect_instantiate.Team`",
    );
    assert.equal(
        instantiate_non_class(reflect.Type.of<Named>()),
        "reflect.Class.instantiate expects a class type, got `reflect_instantiate.Named`",
    );
    assert.equal(
        instantiate_non_class(reflect.Type.of<Ticket?>()),
        "reflect.Class.instantiate expects a class type, got `reflect_instantiate.Ticket | null`",
    )
}

// ─── value_of ────────────────────────────────────────────────────────────────

test "value_of finds a variant by its declared name" {
    let technical: unknown = reflect.Enum.value_of<Team>("Technical");
    assert.is_true(technical is Team);
    assert.equal(technical, Team.Technical);
    assert.equal(reflect.Enum.value_of<Team>("Hidden"), Team.Hidden)
}

function runtime_variant_name() -> string {
    let color = reflect.Enum.new("Color", ["Red", "Green"]);
    let type C = color;
    reflect.Enum.get_value(reflect.Enum.value_of<C>("Green"))
}

test "value_of works on an enum created at runtime" {
    assert.equal(runtime_variant_name(), "Green")
}

function team_value_of(name: string) -> string {
    let team = reflect.Enum.value_of<Team>(name) catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    reflect.Enum.get_value(team)
}

test "value_of rejects an unknown name" {
    assert.equal(
        team_value_of("Sales"),
        "reflect.Enum.value_of: enum `reflect_instantiate.Team` has no variant `Sales`",
    )
}

test "value_of does not accept an alias" {
    assert.equal(
        team_value_of("tech"),
        "reflect.Enum.value_of: enum `reflect_instantiate.Team` has no variant `tech`; `tech` is the alias of `Technical`",
    )
}

function value_of_non_enum(ty: reflect.Type) -> string {
    let type T = ty;
    let value = reflect.Enum.value_of<T>("Technical") catch (e) {
        reflect.errors.InvalidArgument => return e.message,
    };
    "found"
}

test "value_of rejects a type that is not an enum" {
    assert.equal(
        value_of_non_enum(Ticket.type()),
        "reflect.Enum.value_of expects an enum type, got `reflect_instantiate.Ticket`",
    );
    assert.equal(
        value_of_non_enum(reflect.Type.of<Team.Technical>()),
        "reflect.Enum.value_of expects an enum type, got `reflect_instantiate.Team.Technical`",
    )
}
