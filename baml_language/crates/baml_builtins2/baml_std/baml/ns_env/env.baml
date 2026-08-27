/// Returns the value of the environment variable `key`, or `null` if not set.
///
/// # Throws
/// - `Io` if environment variable access is not available.
/// - `ParseError` if the variable is set but its value is not valid UTF-8.
function get(key: string) -> string? throws root.errors.Io | root.errors.ParseError {
    $rust_io_function
}

/// Returns the value of the environment variable `key`. Panics if not set.
function get_or_panic(key: string) -> string {
    get(key) ?? root.sys.panic("env var not found: " + key)
}

/// A lazy reference to an environment variable.
///
/// Holds the variable's name, not its value. The value is not read until `get()` is called.
///
/// `env.SOME_VAR` desugars to `baml.env.ref("SOME_VAR")`, which constructs one
/// of these. Nothing is read at construction time, so a `client Foo = ...`
/// declaration — evaluated during `$init`, which cannot run io sysops — can
/// name a variable without dying on an opaque `InitFailed`, and a host that
/// loads its secrets AFTER the runtime initializes still gets the fresh value:
/// the read happens at USE time, inside `get()`.
///
/// A `Ref` is also what keeps secrets out of constructed values. The name is
/// the only thing captured, so a client holding `api_key: env.OPENAI_API_KEY`
/// never has the key inside it — printing, serializing, or journaling that
/// client cannot leak the credential.
class Ref {
    /// The environment variable's name. Never its value.
    name: string,

    /// Read the variable now, or `null` when it is not set.
    function get(self) -> string? {
        root.env.get(self.name)
    }

    /// Read the variable now. Panics, naming the variable, when it is not set.
    function get_or_panic(self) -> string {
        root.env.get_or_panic(self.name)
    }

    /// Read the variable now, falling back to `fallback` when it is not set.
    function or(self, fallback: string) -> string {
        root.env.get(self.name) ?? fallback
    }
}

/// Construct a late-bound reference to the environment variable `name`.
///
/// This is what `env.NAME` sugar desugars to. It performs no io: the variable
/// is read when the returned `Ref` is used, not here.
function ref(name: string) -> Ref throws never {
    Ref { name: name }
}

/// Like `get_or_panic`, but tolerant of a missing variable when `lenient` is
/// true: a missing variable yields the empty string instead of panicking.
///
/// Used by generated client constructors so that offline prompt rendering
/// (`<Fn>$render_prompt`) can build a client for its provider/role metadata
/// without requiring credentials. The network path (`<Fn>$call`) constructs
/// with `lenient = false`, so a missing `api_key` env var still panics for it,
/// exactly as before.
function get_or_panic_lenient(key: string, lenient: bool) -> string {
    if (lenient) {
        get(key) ?? ""
    } else {
        get_or_panic(key)
    }
}
