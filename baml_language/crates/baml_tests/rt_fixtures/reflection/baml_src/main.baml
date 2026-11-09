class ReflectRecord { name: string }
class ReflectBox<T> { value: T }
class ReflectTypeFields {
    ty: reflect.Type,
    types: reflect.Type[],
    maybe: reflect.Type?,
    nested: ReflectBox<reflect.Type>,
}
enum ReflectColor { Red, Blue }
interface ReflectMarker { function mark(self) -> int throws never }

function ReflectParameterType<T>(value: T) -> reflect.Type {
    reflect.Type.of<T>()
}

function ReflectDefault(ty: reflect.Type = reflect.Type.of<int>()) -> reflect.Type {
    ty
}

/// baml:$trace=trace.span
function ReflectForward(ty: reflect.Type) -> reflect.Type {
    log.info(ty);
    ty
}

function RtNativeReflection() -> int {
    defer { log.info("leaving reflected types"); }
    let ty = reflect.Type.of<ReflectRecord>();
    let fields = ReflectTypeFields {
        ty: ReflectForward(ty),
        types: [reflect.Type.of<int>(), ty],
        maybe: ty,
        nested: ReflectBox<reflect.Type> { value: ty },
    };
    let capture = () -> reflect.Type {
        log.warn("returning captured type");
        ty
    };
    let erased: unknown = capture();
    let by_name: map<string, reflect.Type> = { "record": ty, "integer": reflect.Type.of<int>() };
    let absent: reflect.Type? = null;
    baml.sys.collect_garbage();
    let array = reflect.Type.of<int[]>().as_array();
    let map = reflect.Type.of<map<string, ReflectRecord>>().as_map();
    let primitive = reflect.Type.of<int>().as_primitive();
    let class_view = ty.as_class();
    let enum_view = reflect.Type.of<ReflectColor>().as_enum();
    let union_view = reflect.Type.of<int | string>().as_union();
    let literal_view = reflect.Type.of<"hello">().as_literal();
    let interface_view = reflect.Type.of<ReflectMarker>().as_interface();
    let callable = (value: int) -> string { `${value}` };
    let callable_type = ReflectParameterType(callable);
    let function_view = callable_type.as_function();
    if (array == null || map == null || primitive == null || class_view == null
        || enum_view == null || union_view == null || literal_view == null
        || interface_view == null || function_view == null) {
        throw "missing reflection kind";
    }
    let primitive_kind = reflect.Type.of<int>().kind();
    let class_kind = ty.kind();
    let checks = [
        fields.ty == ty,
        fields.types[0] == reflect.Type.of<int>(),
        fields.types[1] == ty,
        fields.maybe == ty,
        fields.nested.value == ty,
        erased == ty,
        capture() == ty,
        ty != reflect.Type.of<int>(),
        by_name["record"] == ty,
        by_name["integer"] == reflect.Type.of<int>(),
        absent == null,
        fields.maybe != null,
        ReflectDefault() == reflect.Type.of<int>(),
        ReflectDefault(ty = ty) == ty,
        ty.to_string() == "ReflectRecord",
        reflect.Type.of<ReflectBox<int>>().to_string() == "ReflectBox<int>",
        reflect.Type.of<ReflectBox<int>>() != reflect.Type.of<ReflectBox<string>>(),
        reflect.Type.of<int | string>() == reflect.Type.of<string | int>(),
        reflect.Type.of<string?>() == reflect.Type.of<string | null>(),
        array.element_type() == reflect.Type.of<int>(),
        array.as_type() == reflect.Type.of<int[]>(),
        map.key_type() == reflect.Type.of<string>(),
        map.value_type() == ty,
        map.as_type() == reflect.Type.of<map<string, ReflectRecord>>(),
        primitive.as_type() == reflect.Type.of<int>(),
        class_view.as_type() == ty,
        enum_view.as_type() == reflect.Type.of<ReflectColor>(),
        union_view.as_type() == reflect.Type.of<string | int>(),
        literal_view.as_type() == reflect.Type.of<"hello">(),
        interface_view.as_type() == reflect.Type.of<ReflectMarker>(),
        function_view.as_type() == callable_type,
        function_view.return_type() == reflect.Type.of<string>(),
        ty.as_map() == null,
        primitive_kind is reflect.Primitive,
        class_kind is reflect.Class,
        reflect.Type.of<int[]>().kind() is reflect.Array,
        reflect.Type.of<map<string, int>>().kind() is reflect.Map,
        reflect.Type.of<ReflectColor>().kind() is reflect.Enum,
        reflect.Type.of<int | string>().kind() is reflect.Union,
        reflect.Type.of<"hello">().kind() is reflect.Literal,
        reflect.Type.of<ReflectMarker>().kind() is reflect.Interface,
        callable_type.kind() is reflect.Function,
    ];
    let index = 0;
    let failed = 0;
    for (let check in checks) {
        if (!check) {
            log.warn(`reflection check ${index} failed`);
            failed = failed + 1;
        }
        index = index + 1;
    }
    if (failed != 0) { throw "reflection check failed"; }
    7
}

class EqPoint { x: int, y: float, label: string?, tags: string[] }
class EqNode { value: int, next: EqNode? }
enum EqColor { Red, Green }

function EqSame(a: unknown, b: unknown) -> bool {
    a == b
}

function EqAssertFails() -> int {
    assert.equal([1], [2]);
    0
}

function RtNativeEquality() -> int {
    let p1 = EqPoint { x: 1, y: 2.5, label: "p", tags: ["a", "b"] };
    let p2 = EqPoint { x: 1, y: 2.5, label: "p", tags: ["a", "b"] };
    let p3 = EqPoint { x: 1, y: 2.5, label: null, tags: ["a"] };
    let nan = float.nan();
    let n1 = EqNode { value: 1, next: EqNode { value: 2, next: null } };
    let n2 = EqNode { value: 1, next: EqNode { value: 2, next: null } };
    let m1: map<string, int[]> = { "a": [1, 2], "b": [] };
    let m2: map<string, int[]> = { "b": [], "a": [1, 2] };
    let m3: map<string, int[]> = { "a": [1, 3], "b": [] };
    let results = [
        p1 == p2, p1 == p3, p1 != p3, n1 == n2,
        EqSame(p1, p2), EqSame(p1, p3), EqSame(1, 1.0), EqSame(1, 1), EqSame("x", "x"),
        EqSame(nan, nan), EqSame([nan], [nan]), EqSame(null, null), EqSame(null, p1),
        EqSame(EqColor.Red, EqColor.Red), EqSame(EqColor.Red, EqColor.Green),
        EqSame([1, 2], [1, 2]), EqSame([1, 2], [2, 1]), EqSame(m1, m2), EqSame(m1, m3),
        m1 == m2, m1 == m3, EqSame(p1, n1), EqSame(true, 1),
    ];
    let count = 0;
    for (let r in results) {
        log.info(r);
        if (r) { count = count + 1; }
    }
    assert.equal(p1, p2);
    assert.equal([p1], [p2]);
    let caught = EqAssertFails() catch (e) {
        baml.panics.AssertionFailed => {
            log.info(e.message);
            100
        }
    };
    count + caught
}

class EqTag { id: int, note: string }
implement baml.ops.Equals for EqTag {
    function eq(self, other: Self) -> bool throws never {
        log.info(`eq ${self.note} ${other.note}`);
        self.id == other.id
    }
}
class EqWrap { tag: EqTag, items: EqTag[], label: string }
enum EqLoose { A, B }
implement baml.ops.Equals for EqLoose {
    function eq(self, other: Self) -> bool throws never { true }
}

class EqBox<T> { v: T, note: string }
implement<T> baml.ops.Equals for EqBox<T> {
    function eq(self, other: Self) -> bool throws never {
        baml.ops.equals_equals(self.v, other.v)
    }
}

function RtNativeEqualityOverrides() -> int {
    let a = EqTag { id: 1, note: "a" };
    let b = EqTag { id: 1, note: "b" };
    let c = EqTag { id: 2, note: "c" };
    let w1 = EqWrap { tag: a, items: [a, c], label: "w" };
    let w2 = EqWrap { tag: b, items: [b, c], label: "w" };
    let w3 = EqWrap { tag: c, items: [a], label: "w" };
    let results = [
        a == b, a == c, a != c, a == a,
        EqSame(a, b), EqSame(w1, w2), EqSame(w1, w3), w1 == w2,
        EqSame(EqLoose.A, EqLoose.B), EqLoose.A == EqLoose.B,
        EqSame([a], [b]), EqSame({ "k": a }, { "k": c }),
        EqSame(EqBox<int> { v: 1, note: "x" }, EqBox<int> { v: 1, note: "y" }),
        EqSame(EqBox<int> { v: 1, note: "x" }, EqBox<int> { v: 2, note: "x" }),
        EqSame(EqBox<string> { v: "s", note: "x" }, EqBox<string> { v: "s", note: "z" }),
        EqSame(EqBox<int> { v: 1, note: "x" }, EqBox<string> { v: "1", note: "x" }),
    ];
    let count = 0;
    for (let r in results) {
        log.info(r);
        if (r) { count = count + 1; }
    }
    count
}

class Marked { n: int }
implement ReflectMarker for Marked {
    function mark(self) -> int throws never { self.n }
}

function RtNativeReflectionRuntime() -> int {
    let ints = reflect.Type.of<int>().array();
    let opt = reflect.Type.of<int | string>().optional();
    let already = reflect.Type.of<string?>().optional();
    let nested = ints.as_type().array().as_type().optional().as_type();
    let members = reflect.Type.of<int | string | null>().as_union();
    let callable = (value: int, label: string) -> string { label };
    let fn_type = ReflectParameterType(callable).as_function();
    let literal = reflect.Type.of<"hello">().as_literal();
    let number = reflect.Type.of<42>().as_literal();
    let made = reflect.Literal.new("hi");
    if (members == null || fn_type == null || literal == null || number == null) {
        throw "missing view";
    }
    let params = fn_type.params();
    let types = [
        ints.as_type(), ints.element_type(), opt.as_type(), already.as_type(), nested,
        reflect.Type.of_value(1), reflect.Type.of_value("s"), reflect.Type.of_value(2.5),
        reflect.Type.of_value(ReflectRecord { name: "r" }), reflect.Type.of_value([1, 2]),
        reflect.Type.of_value(ReflectColor.Red), reflect.Type.of_value({ "a": 1 }),
        reflect.Type.of_value(null), reflect.Type.of_value(true), made.as_type(),
        params[0].type, params[1].type,
    ];
    for (let t in types) {
        log.info(t.to_string());
    }
    let member_types = members.member_types();
    let checks = [
        ints.as_type() == reflect.Type.of<int[]>(),
        ints.element_type() == reflect.Type.of<int>(),
        opt.as_type() == reflect.Type.of<int | string | null>(),
        opt.as_type() == reflect.Type.of<string | int | null>(),
        already.as_type() == reflect.Type.of<string?>(),
        nested == reflect.Type.of<int[][]?>(),
        member_types.length() == 3,
        member_types[1] == reflect.Type.of<string>(),
        reflect.Type.of_value(1) == reflect.Type.of<int>(),
        reflect.Type.of_value(ReflectRecord { name: "r" }) == reflect.Type.of<ReflectRecord>(),
        reflect.Type.of_value([1, 2]) == reflect.Type.of<int[]>(),
        reflect.Type.of_value(ReflectColor.Red) == reflect.Type.of<ReflectColor>(),
        params.length() == 2,
        params[0].name == "value",
        params[1].type == reflect.Type.of<string>(),
        !params[0].optional,
        made.as_type() == reflect.Type.of<"hi">(),
        literal.value() == "hello",
        number.value() == 42,
        made.value() == "hi",
        reflect.Type.of<Marked>().implements(reflect.Type.of<ReflectMarker>()),
        !reflect.Type.of<ReflectRecord>().implements(reflect.Type.of<ReflectMarker>()),
        reflect.Type.of_value(Marked { n: 1 }).implements(reflect.Type.of<ReflectMarker>()),
        !reflect.Type.of<ReflectMarker>().implements(reflect.Type.of<ReflectMarker>()),
    ];
    log.info(reflect.Type.of<ReflectRecord>().to_baml());
    log.info(reflect.Type.of<ReflectBox<int>[]>().to_baml());
    log.info(reflect.Type.of<int | string>().to_baml());
    let index = 0;
    let failed = 0;
    for (let check in checks) {
        if (!check) {
            log.warn(`runtime reflection check ${index} failed`);
            failed = failed + 1;
        }
        index = index + 1;
    }
    if (failed != 0) { throw "runtime reflection check failed"; }
    11
}

/// A described record.
class MetaRecord {
    /// The label.
    name: string @alias("label") @description("display name"),
    count: int,
    tags: string[]?,
    hidden: bool @skip,
}
class MetaBox<T> { item: T, items: T[] }
enum MetaLevel {
    Low @alias("lo"),
    High @description("urgent"),
}

function RtNativeReflectionMeta() -> int {
    let record = reflect.Type.of<MetaRecord>().as_class();
    let boxed = reflect.Type.of<MetaBox<MetaRecord>>().as_class();
    let level = reflect.Type.of<MetaLevel>().as_enum();
    if (record == null || boxed == null || level == null) { throw "missing view"; }
    let total = 0;
    for (let f in record.fields()) {
        log.info(`${f.name}: ${f.type} alias=${f.meta.alias ?? "-"} desc=${f.meta.description ?? "-"} doc=${f.meta.docstring ?? "-"} skip=${f.meta.skip} other=${f.meta.other.length()}`);
        total = total + 1;
    }
    for (let f in boxed.fields()) {
        log.info(`${f.name}: ${f.type}`);
        total = total + 1;
    }
    for (let v in level.values()) {
        log.info(`${v.name} alias=${v.meta.alias ?? "-"} desc=${v.meta.description ?? "-"}`);
        total = total + 1;
    }
    let m = record.metadata();
    log.info(`class doc=${m.docstring ?? "-"} alias=${m.alias ?? "-"} skip=${m.skip}`);
    log.info(`enum doc=${level.metadata().docstring ?? "-"}`);
    let fields = record.fields();
    if (fields[0].type != reflect.Type.of<string>() || fields[2].type != reflect.Type.of<string[]?>()) {
        throw "field types";
    }
    total
}

const GLOBAL_INT_TYPE = reflect.Type.of<int>();
const GLOBAL_PACKAGE = reflect.Package.current();
const GLOBAL_TYPES = [reflect.Type.of<string>(), reflect.Type.of<ReflectRecord>()];

function RtNativeReflectionGlobals() -> int {
    let package = GLOBAL_PACKAGE;
    log.info(GLOBAL_INT_TYPE);
    log.info(GLOBAL_TYPES[1].to_string());
    if (GLOBAL_INT_TYPE != reflect.Type.of<int>() || GLOBAL_TYPES[0] != reflect.Type.of<string>()) {
        throw "global type";
    }
    3
}
