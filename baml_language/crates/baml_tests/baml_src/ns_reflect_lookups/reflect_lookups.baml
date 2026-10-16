// Lookups by name: `get_*` throws `reflect.errors.NotFound` on a miss,
// `find_*` returns `null`.

function rl_package() -> reflect.Package {
    reflect.Package.compile(
        {
            "main.baml": `
                class Foo { value int }
                enum Color { Red }
                interface Shape {}
                function double(n: int) -> int { n * 2 }
            `,
        },
    )
}

class RlRecord {
    count: int,
    note: string?,
}

function rl_record() -> reflect.AnyClass {
    let value: reflect.AnyClass = RlRecord { count: 3, note: null } else {
        throw "RlRecord is a class"
    };
    value
}

test "get_class hit returns the class" {
    assert.equal(rl_package().get_class("Foo").as_type().to_string(), "Foo")
}

test "get_class miss throws NotFound" {
    let message = rl_package().get_class("Missing") catch (e) {
        reflect.errors.NotFound { message } => message,
    };
    assert.equal(message, "no class named Missing in this package")
}

test "find_class returns the class or null" {
    let pkg = rl_package();
    assert.not_null(pkg.find_class("Foo"));
    assert.equal(pkg.find_class("Missing"), null)
}

test "get_enum hit returns the enum" {
    assert.equal(rl_package().get_enum("Color").values()[0].name, "Red")
}

test "get_enum miss throws NotFound" {
    let message = rl_package().get_enum("Missing") catch (e) {
        reflect.errors.NotFound { message } => message,
    };
    assert.equal(message, "no enum named Missing in this package")
}

test "find_enum returns the enum or null" {
    let pkg = rl_package();
    assert.not_null(pkg.find_enum("Color"));
    assert.equal(pkg.find_enum("Missing"), null)
}

test "get_interface hit returns the interface" {
    assert.equal(rl_package().get_interface("Shape").as_type().to_string(), "Shape")
}

test "get_interface miss throws NotFound" {
    let message = rl_package().get_interface("Missing") catch (e) {
        reflect.errors.NotFound { message } => message,
    };
    assert.equal(message, "no interface named Missing in this package")
}

test "find_interface returns the interface or null" {
    let pkg = rl_package();
    assert.not_null(pkg.find_interface("Shape"));
    assert.equal(pkg.find_interface("Missing"), null)
}

test "get_function hit returns the function" {
    let double = rl_package().get_function<(int) -> int>("double");
    assert.equal(double(21), 42)
}

test "get_function miss throws NotFound" {
    let message = rl_package().get_function<(int) -> int>("missing") catch (e) {
        reflect.errors.NotFound { message } => message,
        reflect.errors.CompilationError => "CompilationError",
    };
    assert.equal(message, "no function named missing in this package")
}

test "find_function returns the function or null" {
    let pkg = rl_package();
    let double = pkg.find_function<(int) -> int>("double") ?? throw "double is exported";
    assert.equal(double(4), 8);
    assert.equal(pkg.find_function<(int) -> int>("missing"), null)
}

test "get_function signature mismatch throws CompilationError" {
    let outcome = rl_package().get_function<(string) -> int>("double") catch (e) {
        reflect.errors.CompilationError => "CompilationError",
        reflect.errors.NotFound => "NotFound",
    };
    assert.equal(outcome, "CompilationError")
}

test "find_function signature mismatch throws CompilationError" {
    let outcome = rl_package().find_function<(string) -> int>("double") catch (e) {
        reflect.errors.CompilationError => "CompilationError",
    };
    assert.equal(outcome, "CompilationError")
}

test "a name declared as another kind is a miss" {
    let pkg = rl_package();
    assert.equal(pkg.find_class("Color"), null);
    assert.equal(pkg.find_enum("Foo"), null);
    assert.equal(pkg.find_interface("Foo"), null);
    assert.equal(pkg.find_function<() -> int>("Foo"), null);
    let message = pkg.get_class("Color") catch (e) {
        reflect.errors.NotFound { message } => message,
    };
    assert.equal(message, "no class named Color in this package")
}

/// Names what `lookup` threw. An uninitialized receiver throws
/// `baml.errors.InvalidArgument`, which the `get_*` lookups do not declare, so
/// the lookup is widened to `throws unknown` to make that arm reachable.
function rl_thrown(lookup: () -> unknown throws unknown) -> string {
    lookup() catch (e) {
        reflect.errors.NotFound => {
            return "NotFound";
        }
        baml.errors.InvalidArgument { message } => {
            return message;
        }
    };
    "nothing thrown"
}

test "get on an uninitialized package reports the receiver, not a miss" {
    let forged = reflect.Package { _inner: 1 };
    let fault = "reflect.Package is not initialized";
    assert.equal(
        rl_thrown(() -> unknown {
            forged.get_class("Foo")
        }),
        fault,
    );
    assert.equal(
        rl_thrown(() -> unknown {
            forged.get_enum("Color")
        }),
        fault,
    );
    assert.equal(
        rl_thrown(() -> unknown {
            forged.get_interface("Shape")
        }),
        fault,
    );
    assert.equal(forged.find_class("Foo"), null)
}

test "value get reads a field" {
    assert.equal(rl_record().get<int>("count"), 3)
}

test "value get on a missing field throws NotFound" {
    let message = rl_record().get<int>("missing") catch (e) {
        reflect.errors.NotFound { message } => message,
        reflect.errors.TypeMismatch => "TypeMismatch",
    };
    assert.equal(message, "no field named missing on reflect_lookups.RlRecord")
}

test "value get reads a null-valued nullable field as null" {
    assert.equal(rl_record().get<string?>("note"), null)
}

test "value get on a null-valued field with a non-null type throws TypeMismatch" {
    let outcome = rl_record().get<string>("note") catch (e) {
        reflect.errors.TypeMismatch => "TypeMismatch",
        reflect.errors.NotFound => "NotFound",
    };
    assert.equal(outcome, "TypeMismatch")
}

test "value find returns the field value or null" {
    let record = rl_record();
    assert.equal(record.find<int>("count"), 3);
    assert.equal(record.find<int>("missing"), null)
}

test "value get_field returns a bound field" {
    let field = rl_record().get_field("count");
    assert.equal(field.name, "count");
    assert.equal(field.value<int>(), 3)
}

test "value get_field on a missing field throws NotFound" {
    let message = rl_record().get_field("missing") catch (e) {
        reflect.errors.NotFound { message } => message,
    };
    assert.equal(message, "no field named missing on reflect_lookups.RlRecord")
}

test "value find_field returns the field or null" {
    let record = rl_record();
    assert.not_null(record.find_field("count"));
    assert.equal(record.find_field("missing"), null)
}
