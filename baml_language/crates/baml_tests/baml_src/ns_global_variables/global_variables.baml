// Global variables: top-level `const` bindings, evaluated once at startup.
//
// Every test in the corpus runs in one engine, so the globals here are shared
// by all of them; the one test that mutates a global's contents asserts the
// change it made, not an absolute value.

/// How many times to retry.
const MAX_RETRIES = 3;
const GREETING: string = "hello";
const NAMES: string[] = ["ada", "grace"];
const SCORES = { "ada": 1, "grace": 2 };
const MESSAGE = `${GREETING}, ${NAMES[0]}`;
const OPTIONAL_LIMIT: int? = null;
const EITHER: int | string = "text";
const RATIO: float = 1.5;

// Initialized in dependency order, not source order: `FORWARD` reads `LATER`,
// declared below it, and `THROUGH_HELPER` reads `HELPER_INPUT` through a
// function's body.
const FORWARD = LATER + 1;
const LATER = 41;
const THROUGH_HELPER = helper_reads_global();
const HELPER_INPUT = 5;
const THROUGH_DEFAULT = default_reads_global();
const DEFAULT_INPUT = 7;

function helper_reads_global() -> int {
    HELPER_INPUT * 2
}

function default_reads_global(value: int = DEFAULT_INPUT) -> int {
    value
}

function double(x: int) -> int {
    x * 2
}

const DOUBLED = double(MAX_RETRIES);

// A function value: the lambda body reads a global when it is called.
const ADD_RETRIES = (x: int) -> int {
    x + MAX_RETRIES
};

class Counter {
    n: int,
}

enum Shade {
    Light,
    Dark,
}

const COUNTER = Counter { n: 0 };
const FAVORITE_SHADE = Shade.Dark;

// A handled throw: the initializer's value is the catch arm's.
function fails() -> int throws string {
    throw "nope"
}

const RECOVERED = fails() catch (e) {
    string => -1
};

// A global named like the synthetic test collector stays reachable from tests.
const registry = "user registry";

// A global named like a type does not capture that type's statics.
const float = 2;

// Reflection handles are ordinary values.
const INT_TYPE = reflect.Type.of<int>();
const CURRENT_PACKAGE = reflect.Package.current();

function bump_counter() -> int {
    COUNTER.n += 1;
    COUNTER.n
}

function push_name_and_count(name: string) -> int {
    NAMES.push(name);
    NAMES.length()
}

function shadowed_by_local() -> int {
    let MAX_RETRIES = 100;
    MAX_RETRIES
}

function shadowed_by_parameter(MAX_RETRIES: int) -> int {
    MAX_RETRIES
}

function shadowed_in_lambda() -> int {
    let read = (MAX_RETRIES: int) -> int {
        MAX_RETRIES
    };
    read(9)
}

function local_copy_is_independent() -> int {
    let copy = MAX_RETRIES;
    copy += 10;
    copy
}

function generic_method_on_global() -> int[] {
    NAMES.map((name: string) -> int {
        name.length()
    })
}

function reflected_function_through_global() -> int {
    let f = CURRENT_PACKAGE.get_function<(x: int) -> int>("global_variables.double")
        ?? throw "missing double";
    f(21)
}

function spawned_task_reads_global() -> int {
    let task = spawn { MAX_RETRIES * 10 };
    await task
}

// `float.nan` names the static, not a member of the global `float`.
function type_static_through_global_name() -> bool {
    let make_nan = float.nan;
    make_nan().is_nan()
}

function qualified_child_global() -> int {
    root.global_variables.settings.TIMEOUT_SECONDS
}

function qualified_function_value_call() -> int {
    root.global_variables.settings.SCALE(21)
}

function qualified_field_mutation() -> int {
    root.global_variables.settings.STATE.n += 1;
    root.global_variables.settings.STATE.n
}

function optional_chain_on_qualified_global() -> int {
    root.global_variables.settings.MAYBE_STATE?.n ?? -1
}

function root_qualified_global() -> int {
    root.global_variables.MAX_RETRIES
}

function session_reads_package_global() -> int {
    let session = reflect.Session.new(packages = { "app": reflect.Package.current() });
    session.eval<int>(
        `app.global_variables.MAX_RETRIES + app.global_variables.settings.TIMEOUT_SECONDS`,
    )
}

function compiled_package_globals() -> int {
    let package = reflect.Package.compile(
        {
            "main.baml": "const BASE = 40;\nconst DERIVED = BASE + 2;\nfunction get() -> int { DERIVED }",
        },
    );
    let get = package.get_function<() -> int>("get") ?? throw "missing get";
    get()
}

function compiled_package_initializer_throw_reaches_the_caller() -> string {
    let source = "function boom() -> int throws string { throw \"init failed\" }\nconst X = boom();";
    let outcome = reflect.Package.compile({ "main.baml": source }) catch (e) {
        string => {
            return e;
        }
    };
    "compiled"
}

testset "global_variables" {
    test "scalar and annotated globals" {
        assert.equal(MAX_RETRIES, 3);
        assert.equal(GREETING, "hello");
        assert.equal(RATIO, 1.5);
        assert.equal(OPTIONAL_LIMIT, null);
        assert.equal(EITHER, "text")
    }

    test "container globals" {
        assert.equal(SCORES["grace"], 2);
        assert.equal(MESSAGE, "hello, ada")
    }

    test "initializers run in dependency order" {
        assert.equal(FORWARD, 42);
        assert.equal(THROUGH_HELPER, 10);
        assert.equal(THROUGH_DEFAULT, 7);
        assert.equal(DOUBLED, 6)
    }

    test "function value global" {
        assert.equal(ADD_RETRIES(1), 4)
    }

    test "class and enum globals" {
        assert.equal(FAVORITE_SHADE, Shade.Dark)
    }

    test "a handled throw initializes from the catch arm" {
        assert.equal(RECOVERED, -1)
    }

    test "locals, parameters, and lambda parameters shadow globals" {
        assert.equal(shadowed_by_local(), 100);
        assert.equal(shadowed_by_parameter(7), 7);
        assert.equal(shadowed_in_lambda(), 9);
        assert.equal(MAX_RETRIES, 3)
    }

    test "a local copy of a global is a separate binding" {
        assert.equal(local_copy_is_independent(), 13);
        assert.equal(MAX_RETRIES, 3)
    }

    test "a global's contents are shared and mutable" {
        let before = COUNTER.n;
        let after = bump_counter();
        assert.equal(after, before + 1);
        assert.equal(COUNTER.n, after)
    }

    test "a global array can grow" {
        let before = NAMES.length();
        assert.equal(push_name_and_count("katherine"), before + 1)
    }

    test "generic methods on a global instantiate at the call" {
        assert.equal(generic_method_on_global().at(0), 3)
    }

    test "a global named registry is visible in tests" {
        assert.equal(registry, "user registry")
    }

    test "a global named like a type does not capture its statics" {
        assert.equal(float, 2);
        assert.is_true(type_static_through_global_name())
    }

    test "reflection values in globals" {
        assert.equal(INT_TYPE, reflect.Type.of<int>());
        assert.equal(reflected_function_through_global(), 42)
    }

    test "a spawned task reads a global" {
        assert.equal(spawned_task_reads_global(), 30)
    }

    test "qualified globals" {
        assert.equal(qualified_child_global(), 30);
        assert.equal(root_qualified_global(), 3);
        assert.equal(root.global_variables.settings.TIMEOUT_SECONDS, 30);
        assert.equal(root.global_variables.settings.RETRY_BUDGET, 90);
        assert.equal(qualified_function_value_call(), 42);
        assert.equal(optional_chain_on_qualified_global(), 7);
        assert.equal(root.global_variables.ADD_RETRIES(1), 4)
    }

    test "a qualified global's contents are shared and mutable" {
        let before = root.global_variables.settings.STATE.n;
        assert.equal(qualified_field_mutation(), before + 1)
    }

    test "a session reads the package's globals" {
        assert.equal(session_reads_package_global(), 33)
    }

    test "a compiled package initializes its own globals" {
        assert.equal(compiled_package_globals(), 42)
    }

    test "a compiled package's initializer throw reaches the caller" {
        assert.equal(compiled_package_initializer_throw_reaches_the_caller(), "init failed")
    }
}
