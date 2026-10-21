// Reflection over a package's globals: `globals()` lists every global
// variable and `client` declaration with its current value, and
// `get_global<T>` / `find_global<T>` look one up by package-local name,
// checked against `T`. The corpus is one package, so these globals are
// `root.global_variables.reflection.NAME` to it.

class Endpoint {
    url string
}

const RETRIES = 3;
const ENDPOINT = Endpoint { url: "https://example.test" };
const NOTHING: int? = null;
// Changed through reflection by one test only.
const SHARED = Endpoint { url: "before" };

client Model = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function own(name: string) -> string {
    "global_variables.reflection." + name
}

function lists(name: string) -> bool {
    reflect.Package.current().globals().has("root." + own(name))
}

/// The clients this namespace declares, found among the globals by value.
function declared_clients() -> string[] {
    let prefix = "root." + own("");
    let globals = reflect.Package.current().globals();
    let clients: string[] = [];
    for (let name in globals.keys()) {
        if (name.starts_with(prefix) && globals[name] is ai.Client) {
            clients.push(name);
        }
    }
    clients
}

function missing_global_message() -> string {
    reflect.Package.current().get_global<int>(own("MISSING")) catch (e) {
        reflect.errors.NotFound { message } => {
            return message;
        }
    };
    "found"
}

function mismatched_global_message() -> string {
    reflect.Package.current().get_global<string>(own("RETRIES")) catch (e) {
        reflect.errors.TypeMismatch { message } => {
            return message;
        }
    };
    "matched"
}

function compiled_package_globals() -> int {
    let package = reflect.Package.compile(
        {
            "main.baml": "const BASE = 40;\nclient Local = openai.ResponsesClient.new(model = \"m\", api_key = \"k\", base_url = \"http://localhost:1\");",
        },
    );
    let globals = package.globals();
    if (!globals.has("root.BASE") || !(globals["root.Local"] is ai.Client)) {
        return -1;
    }
    package.get_global<int>("BASE") + 2
}

testset "global reflection" {
    test "globals lists global variables and clients" {
        assert.is_true(lists("RETRIES"));
        assert.is_true(lists("ENDPOINT"));
        assert.is_true(lists("NOTHING"));
        assert.is_true(lists("Model"));
        assert.equal(reflect.Package.current().globals()["root." + own("RETRIES")], 3)
    }

    test "a client is found among the globals by its value" {
        assert.equal(declared_clients(), ["root." + own("Model")])
    }

    test "get_global reads a global as T" {
        let package = reflect.Package.current();
        assert.equal(package.get_global<int>(own("RETRIES")), 3);
        assert.equal(package.get_global<int>("root." + own("RETRIES")), 3);
        assert.equal(package.get_global<Endpoint>(own("ENDPOINT")).url, "https://example.test");
        assert.equal(package.get_global<int?>(own("NOTHING")), null);
        assert.is_true(package.get_global<ai.Client>(own("Model")) is ai.Client)
    }

    test "find_global returns null for a missing name" {
        assert.equal(reflect.Package.current().find_global<int>(own("MISSING")), null);
        assert.equal(reflect.Package.current().find_global<int>(own("RETRIES")), 3)
    }

    test "get_global throws NotFound for a missing name" {
        assert.equal(missing_global_message(), "no global named " + own("MISSING") + " in this package")
    }

    test "a global of another type is a TypeMismatch" {
        assert.equal(mismatched_global_message(), "global `" + own("RETRIES") + "` has type `int`, expected `string`")
    }

    test "a value read through reflection is the global itself" {
        reflect.Package.current().get_global<Endpoint>(own("SHARED")).url = "after";
        assert.equal(SHARED.url, "after")
    }

    test "a compiled package lists and reads its own globals" {
        assert.equal(compiled_package_globals(), 42)
    }
}
