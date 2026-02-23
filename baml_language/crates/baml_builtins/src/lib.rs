//! Type signatures for BAML built-in functions.
//!
//! This crate provides compile-time type information for built-in functions,
//! used by the type checker (`baml_compiler_tir`). It does NOT include
//! runtime implementations - those live in `bex_vm`.
//!
//! This separation allows the type checker to avoid depending on the VM.
//!
//! # Adding a new builtin
//!
//! Add a new entry in the `define_builtins!` macro invocation below.
//! This generates both the path constant and the signature in one place.

mod adt;

pub use adt::*;

/// Type pattern for matching/constructing types with type variables.
///
/// Used for generic builtin functions like `Array<T>.push(item: T)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePattern {
    Int,
    Float,
    String,
    Bool,
    Null,
    Array(Box<TypePattern>),
    Map {
        key: Box<TypePattern>,
        value: Box<TypePattern>,
    },
    Media,
    Optional(Box<TypePattern>),
    /// Type variable - binds to actual type during pattern matching.
    /// E.g., `Var("T")` in `Array<T>.push(item: T)` binds to the element type.
    Var(&'static str),
    /// Builtin type - matches exactly by path.
    /// E.g., `Builtin("baml.fs.File")` matches only `Ty::Builtin("baml.fs.File")`.
    Builtin(&'static str),
    /// Function type - a callable with parameters and return type.
    /// E.g., `Function { params: vec![], ret: Builtin("...") }` for `fn() -> T`.
    Function {
        params: Vec<TypePattern>,
        ret: Box<TypePattern>,
    },
    /// Opaque resource handle (file, socket, HTTP response body).
    Resource,
    /// Builtin unknown type - accepts any value during type checking.
    /// Used for builtins that need to accept heterogeneous values
    /// (e.g., `build_primitive_client`'s options map).
    /// Maps to `Ty::BuiltinUnknown` in TIR.
    /// In builtin definitions, use the `Unknown` type annotation.
    BuiltinUnknown,
    /// Builtin enum type - matches exactly by path.
    /// E.g., `Enum("baml.llm.ClientType")` matches only `Ty::Enum("baml.llm.ClientType")`.
    Enum(&'static str),
    /// Meta-type — the type of type values.
    /// A value of type `Type` wraps a `baml_type::Ty` at runtime.
    Type,
}

/// How a builtin type is represented at runtime on the VM heap.
///
/// Most builtin types are stored as `Object::Instance` (same as user-defined classes).
/// Some have dedicated `Object` variants for efficiency or because they wrap
/// opaque Rust ADTs that can't be represented as field-based instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Stored as `Object::Instance` at runtime.
    /// Used by: Request, Response, File, Socket, `PrimitiveClient`.
    Instance,
    /// Stored as `Object::PromptAst` at runtime — wraps an opaque Rust ADT.
    PromptAst,
}

/// A field in a builtin type definition.
#[derive(Debug, Clone)]
pub struct BuiltinField {
    /// Field name (e.g., "_handle", "`status_code`").
    pub name: &'static str,
    /// Field type pattern. All fields have a type (including private ones).
    /// Privacy is handled separately by the `is_private` field.
    pub ty: TypePattern,
    /// Whether this field is private (not visible to BAML code).
    /// Private fields are not added to the type checking map but still have types.
    pub is_private: bool,
    /// Field index in the runtime instance layout.
    pub index: usize,
}

/// A builtin type definition (struct with fields).
#[derive(Debug, Clone)]
pub struct BuiltinTypeDefinition {
    /// Full path (e.g., "baml.http.Response").
    pub path: &'static str,
    /// All fields (public and private) in runtime order.
    pub fields: Vec<BuiltinField>,
    /// How this type is represented on the VM heap.
    pub runtime_kind: RuntimeKind,
}

/// A builtin enum definition (enum with variants).
#[derive(Debug, Clone)]
pub struct BuiltinEnumDefinition {
    /// Full path (e.g., "baml.llm.ClientType").
    pub path: &'static str,
    /// Variant names (e.g., `Primitive`, `Fallback`, `RoundRobin`).
    pub variants: Vec<&'static str>,
}

impl TypePattern {
    #[must_use]
    pub fn optional(self) -> Self {
        match self {
            Self::Optional(inner) => inner.optional(),
            _ => Self::Optional(Box::new(self)),
        }
    }
}

/// A built-in function's type signature (compile-time only).
///
/// This contains everything needed for type checking, but NOT runtime execution.
/// The VM links these signatures to native function implementations separately.
pub struct BuiltinSignature {
    /// Full path, e.g., "baml.Array.length" or "env.get".
    pub path: &'static str,

    /// If Some, this is a method callable as `receiver.method_name()`.
    /// If None, this is a free function callable as `path()`.
    pub receiver: Option<TypePattern>,

    /// Parameters (excluding self for methods).
    pub params: Vec<(&'static str, TypePattern)>,

    /// Return type.
    pub returns: TypePattern,

    /// Whether this is a `sys_op` function (runs async outside VM).
    /// `Sys_op` functions use DispatchFuture/Await instead of Call.
    pub is_sys_op: bool,
}

impl BuiltinSignature {
    /// Get the method name from the path.
    /// E.g., "baml.Array.length" -> Some("length")
    /// E.g., "env.get" -> None (it's a free function)
    pub fn method_name(&self) -> Option<&str> {
        self.receiver.as_ref()?;
        self.path.rsplit('.').next()
    }

    /// Arity including self for methods.
    pub fn arity(&self) -> usize {
        self.params.len() + usize::from(self.receiver.is_some())
    }
}

/// Macro containing all builtin definitions.
///
/// This is used by both `baml_builtins` and `bex_vm` to ensure consistency.
/// The macro takes a callback that will receive the definitions.
#[macro_export]
macro_rules! with_builtins {
    ($callback:path) => {
        $callback! {
            mod baml {
                // =====================================================================
                // Array methods
                // =====================================================================
                struct Array<T> {
                    fn length(self: Array<T>) -> i64;
                    fn push(self: mut Array<T>, item: T);
                    fn at(self: Array<T>, index: i64) -> Result<T>;
                    fn concat(self: Array<T>, other: Array<T>) -> Array<T>;
                }

                // =====================================================================
                // String methods
                // =====================================================================
                struct String {
                    fn length(self: String) -> i64;
                    fn toLowerCase(self: String) -> String;
                    fn toUpperCase(self: String) -> String;
                    fn trim(self: String) -> String;
                    fn includes(self: String, search: String) -> bool;
                    fn startsWith(self: String, prefix: String) -> bool;
                    fn endsWith(self: String, suffix: String) -> bool;
                    #[uses(vm)]
                    fn split(self: String, delimiter: String) -> Array<String>;
                    fn substring(self: String, start: i64, end: i64) -> String;
                    fn replace(self: String, search: String, replacement: String) -> String;
                }

                // =====================================================================
                // Map methods
                // =====================================================================
                struct Map<K, V> {
                    fn length(self: Map<K, V>) -> i64;
                }
                // Map.has only works on string-keyed maps, so we define it separately
                // with only V as generic (String in the signature is the concrete type)
                struct Map<V> {
                    fn has(self: Map<String, V>, key: String) -> bool;
                }

                // =====================================================================
                // Free functions
                // =====================================================================
                #[uses(vm)]
                fn deep_copy<T>(value: T) -> Result<T>;
                #[uses(vm)]
                fn deep_equals<T>(a: T, b: T) -> bool;

                mod unstable {
                    #[uses(vm)]
                    fn string<T>(value: T) -> Result<String>;
                }

                // =====================================================================
                // Math functions
                // =====================================================================
                mod math {
                    fn trunc(value: f64) -> i64;
                }

                // =====================================================================
                // Media methods
                // =====================================================================
                struct Media {
                    fn as_url(self: Media) -> Option<String>;
                    fn as_base64(self: Media) -> Option<String>;
                    fn as_file(self: Media) -> Option<String>;
                    fn mime_type(self: Media) -> Option<String>;
                }

                // =====================================================================
                // Filesystem operations
                // =====================================================================
                mod fs {
                    #[builtin]
                    struct File {
                        private _handle: ResourceHandle,
                        #[sys_op]
                        fn read(self: File) -> String;
                        #[sys_op]
                        fn close(self: File);
                    }

                    #[sys_op]
                    fn open(path: String) -> File;
                }

                // =====================================================================
                // System operations
                // =====================================================================
                mod sys {
                    /// Execute a shell command and return stdout.
                    #[sys_op]
                    fn shell(command: String) -> String;

                    /// Sleep for the given number of milliseconds.
                    #[sys_op]
                    fn sleep(delay_ms: i64);

                    /// Abort execution with an error message.
                    #[sys_op]
                    fn panic(message: String);
                }

                // =====================================================================
                // Network operations
                // =====================================================================
                mod net {
                    #[builtin]
                    struct Socket {
                        private _handle: ResourceHandle,
                        /// Read data from the socket as a string.
                        #[sys_op]
                        fn read(self: Socket) -> String;
                        /// Close the socket.
                        #[sys_op]
                        fn close(self: Socket);
                    }

                    /// Connect to a TCP address (host:port).
                    #[sys_op]
                    fn connect(addr: String) -> Socket;
                }

                // =====================================================================
                // HTTP operations
                // =====================================================================
                mod http {
                    /// An HTTP request to be sent.
                    #[builtin]
                    struct Request {
                        method: String,
                        url: String,
                        headers: Map<String, String>,
                        body: String,
                    }

                    #[builtin]
                    struct Response {
                        private _handle: ResourceHandle,
                        status_code: i64,
                        headers: Map<String, String>,
                        url: String,
                        /// Get response body as text (consumes body).
                        #[sys_op]
                        fn text(self: Response) -> String;
                        /// Check if status is 2xx.
                        #[sys_op]
                        fn ok(self: Response) -> bool;
                    }

                    /// Fetch a URL via HTTP GET.
                    #[sys_op]
                    fn fetch(url: String) -> Response;

                    /// Send an HTTP request and return the response.
                    #[sys_op]
                    fn send(request: Request) -> Response;
                }

                // =====================================================================
                // LLM operations
                // =====================================================================
                mod llm {
                    /// Prompt AST - a structured prompt for LLM calls.
                    /// Opaque: stored as a dedicated heap variant, not as Instance.
                    #[builtin]
                    #[opaque]
                    struct PromptAst {}

                    /// The type of an LLM client (primitive, fallback, or round-robin).
                    #[builtin]
                    enum ClientType {
                        Primitive,
                        Fallback,
                        RoundRobin,
                    }

                    /// A retry policy for LLM calls.
                    #[builtin]
                    struct RetryPolicy {
                        max_retries: i64,
                        initial_delay_ms: i64,
                        multiplier: f64,
                        max_delay_ms: i64,
                    }

                    /// An LLM client (primitive, fallback, or round-robin).
                    /// Built by get_client from compiler metadata.
                    /// Complex fields: accessor/owned codegen is skipped (written manually).
                    #[builtin]
                    struct Client {
                        name: String,
                        client_type: ClientType,
                        sub_clients: Array<Client>,
                        retry: Option<RetryPolicy>,
                    }

                    /// A primitive LLM client (single provider, fully resolved).
                    /// Options have been evaluated (env vars resolved, expressions computed).
                    #[builtin]
                    struct PrimitiveClient {
                        name: String,
                        provider: String,
                        default_role: String,
                        allowed_roles: Vec<String>,
                        options: Map<String, Unknown>,

                        /// Render a Jinja template with the given arguments.
                        /// Returns a structured PromptAst that can be sent to an LLM.
                        #[sys_op]
                        fn render_prompt(self: PrimitiveClient, template: String, args: Map<String, Unknown>) -> PromptAst;

                        /// Specialize a prompt for this client's provider.
                        /// Applies provider-specific transformations (message merging, system prompt
                        /// consolidation, metadata filtering).
                        #[sys_op]
                        fn specialize_prompt(self: PrimitiveClient, prompt: PromptAst) -> PromptAst;

                        /// Build an HTTP request from a specialized prompt.
                        /// Creates a provider-specific HTTP request ready to be sent.
                        #[sys_op]
                        fn build_request(self: PrimitiveClient, prompt: PromptAst) -> Request;

                        /// Parse an HTTP response into a BAML value.
                        /// Interprets the provider-specific response format and parses the output.
                        #[sys_op]
                        fn parse(self: PrimitiveClient, http_response_body: String, type_def: Type) -> Any;
                    }

                    /// Get the Jinja template for an LLM function.
                    #[sys_op]
                    #[uses(engine_ctx)]
                    fn get_jinja_template(function_name: String) -> String;

                    /// Build a PrimitiveClient from evaluated options.
                    /// Called after options have been evaluated by bytecode.
                    #[sys_op]
                    fn build_primitive_client(
                        name: String,
                        provider: String,
                        default_role: String,
                        allowed_roles: Array<String>,
                        options: Map<String, Unknown>
                    ) -> PrimitiveClient;

                    /// Get a Client tree for an LLM function.
                    /// Returns a Client with type, sub-clients, and retry policy.
                    #[sys_op]
                    #[uses(engine_ctx)]
                    fn get_client(function_name: String) -> Client;

                    /// Get the resolve function for a client by name.
                    /// Returns a function that resolves to a PrimitiveClient when called.
                    #[sys_op]
                    #[uses(engine_ctx)]
                    fn resolve_client(client_name: String) -> fn() -> PrimitiveClient;

                    /// Get the next round-robin index for a client.
                    /// Returns the current counter value and increments it atomically.
                    #[sys_op]
                    #[uses(engine_ctx)]
                    fn round_robin_next(client_name: String) -> i64;

                    /// Peek the current round-robin index for a client.
                    /// Returns the current counter value without incrementing it.
                    #[sys_op]
                    #[uses(engine_ctx)]
                    fn round_robin_peek(client_name: String) -> i64;

                    /// Get the return type for an LLM function.
                    /// Returns a Type value that can be passed to `parse()`.
                    #[sys_op]
                    #[uses(engine_ctx)]
                    fn get_return_type(function_name: String) -> Type;
                }
            }

            mod env {
                #[sys_op]
                fn get(key: String) -> Option<String>;
                #[sys_op]
                fn get_or_panic(key: String) -> String;
            }
        }
    };
}

// Define all builtins using ergonomic Rust-like syntax.
// The macro generates:
// - `pub mod paths` with constants like `BAML_ARRAY_LENGTH`, `ENV_GET`, etc.
// - `for_all_builtins!` macro for iterating over all builtin names
// - `BUILTINS` static with all `BuiltinSignature` instances
with_builtins!(baml_builtins_macros::define_builtins);

/// Get all built-in function signatures.
pub fn builtins() -> &'static [BuiltinSignature] {
    &BUILTINS
}

/// Get all built-in type definitions.
pub fn builtin_types() -> &'static [BuiltinTypeDefinition] {
    &BUILTIN_TYPES
}

/// Get all built-in enum definitions.
pub fn builtin_enums() -> &'static [BuiltinEnumDefinition] {
    &BUILTIN_ENUMS
}

/// Find a builtin type by path.
pub fn find_builtin_type(path: &str) -> Option<&'static BuiltinTypeDefinition> {
    builtin_types().iter().find(|td| td.path == path)
}

/// Find a builtin enum by path.
pub fn find_builtin_enum(path: &str) -> Option<&'static BuiltinEnumDefinition> {
    builtin_enums().iter().find(|ed| ed.path == path)
}

/// Find a field in a builtin type.
pub fn find_field(type_path: &str, field_name: &str) -> Option<&'static BuiltinField> {
    let type_def = find_builtin_type(type_path)?;
    type_def.fields.iter().find(|f| f.name == field_name)
}

/// Find methods by method name.
///
/// Returns an iterator over all builtin signatures that are methods with the given name.
pub fn find_method(method_name: &str) -> impl Iterator<Item = &'static BuiltinSignature> {
    builtins()
        .iter()
        .filter(move |def| def.method_name() == Some(method_name))
}

/// Find a free function by path (functions without a receiver).
pub fn find_function(path: &str) -> Option<&'static BuiltinSignature> {
    let normalized = normalize_baml_prefix(path);
    builtins()
        .iter()
        .find(|def| def.receiver.is_none() && def.path == normalized)
}

/// Find any builtin by path (including methods).
///
/// This is useful for direct builtin calls like `baml.Array.length(arr)`.
pub fn find_builtin_by_path(path: &str) -> Option<&'static BuiltinSignature> {
    let normalized = normalize_baml_prefix(path);
    builtins().iter().find(|def| def.path == normalized)
}

/// Normalize the `baml` prefix, allowing any number of a's.
///
/// This is an easter egg: `baml`, `baaml`, `baaaml`, etc. all resolve
/// to the `baml` namespace.
fn normalize_baml_prefix(path: &str) -> std::borrow::Cow<'_, str> {
    // Check if path starts with "ba"
    let Some(after_ba) = path.strip_prefix("ba") else {
        return std::borrow::Cow::Borrowed(path);
    };

    // Count consecutive 'a's after "ba"
    let extra_a_count = after_ba.chars().take_while(|&c| c == 'a').count();

    // Check if followed by "ml"
    let after_as = &after_ba[extra_a_count..];
    if !after_as.starts_with("ml") {
        return std::borrow::Cow::Borrowed(path);
    }

    // If there are extra a's, normalize to "baml"
    if extra_a_count > 0 {
        let rest = &after_as[2..]; // skip "ml"
        std::borrow::Cow::Owned(format!("baml{rest}"))
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_name() {
        let array_len = builtins()
            .iter()
            .find(|d| d.path == "baml.Array.length")
            .unwrap();
        assert_eq!(array_len.method_name(), Some("length"));

        let env_get = builtins().iter().find(|d| d.path == "env.get").unwrap();
        assert_eq!(env_get.method_name(), None);

        let env_get_or_panic = builtins()
            .iter()
            .find(|d| d.path == "env.get_or_panic")
            .unwrap();
        assert_eq!(env_get_or_panic.method_name(), None);
    }

    #[test]
    fn test_arity() {
        let array_len = builtins()
            .iter()
            .find(|d| d.path == "baml.Array.length")
            .unwrap();
        assert_eq!(array_len.arity(), 1); // self only

        let array_push = builtins()
            .iter()
            .find(|d| d.path == "baml.Array.push")
            .unwrap();
        assert_eq!(array_push.arity(), 2); // self + item

        let env_get = builtins().iter().find(|d| d.path == "env.get").unwrap();
        assert_eq!(env_get.arity(), 1); // key only

        let env_get_or_panic = builtins()
            .iter()
            .find(|d| d.path == "env.get_or_panic")
            .unwrap();
        assert_eq!(env_get_or_panic.arity(), 1); // key only
    }

    #[test]
    fn test_find_method() {
        let methods: Vec<_> = find_method("length").collect();
        assert!(methods.len() >= 2); // Array.length and String.length at minimum
    }

    #[test]
    fn test_env_builtins() {
        // env.get is a sys_op returning optional string
        let env_get = find_function("env.get").unwrap();
        assert!(env_get.is_sys_op, "env.get should be a sys_op");
        assert!(
            env_get.receiver.is_none(),
            "env.get should be a free function"
        );

        // env.get_or_panic is a sys_op returning string
        let env_gop = find_function("env.get_or_panic").unwrap();
        assert!(env_gop.is_sys_op, "env.get_or_panic should be a sys_op");
        assert!(
            env_gop.receiver.is_none(),
            "env.get_or_panic should be a free function"
        );
    }

    #[test]
    fn test_find_function() {
        let env_get = find_function("env.get");
        assert!(env_get.is_some());
        assert_eq!(env_get.unwrap().path, "env.get");

        let env_get_or_panic = find_function("env.get_or_panic");
        assert!(env_get_or_panic.is_some());
        assert_eq!(env_get_or_panic.unwrap().path, "env.get_or_panic");
    }

    #[test]
    fn test_find_builtin_by_path() {
        assert!(find_builtin_by_path("baml.Array.length").is_some());
        assert!(find_builtin_by_path("env.get").is_some());
        assert!(find_builtin_by_path("env.get_or_panic").is_some());
        assert!(find_builtin_by_path("nonexistent").is_none());
    }

    #[test]
    fn test_path_constants() {
        // Verify path constants are generated correctly
        assert_eq!(paths::BAML_ARRAY_LENGTH, "baml.Array.length");
        assert_eq!(paths::BAML_STRING_TO_LOWER_CASE, "baml.String.toLowerCase");
        assert_eq!(paths::ENV_GET, "env.get");
        assert_eq!(paths::ENV_GET_OR_PANIC, "env.get_or_panic");
        assert_eq!(paths::BAML_DEEP_COPY, "baml.deep_copy");
        assert_eq!(paths::BAML_UNSTABLE_STRING, "baml.unstable.string");
    }

    #[test]
    fn test_llm_module() {
        // The baml.llm module contains LLM-related builtins
        let render_prompt = find_builtin_by_path("baml.llm.PrimitiveClient.render_prompt");
        assert!(render_prompt.is_some());
        assert!(render_prompt.unwrap().is_sys_op);

        let get_client_fn = find_builtin_by_path("baml.llm.get_client");
        assert!(get_client_fn.is_some(), "get_client should be found");
        let get_client_fn = get_client_fn.unwrap();
        assert!(get_client_fn.is_sys_op, "get_client should be sys_op");
        assert_eq!(
            get_client_fn.arity(),
            1,
            "get_client should have arity 1 (function_name param)"
        );
        assert_eq!(
            get_client_fn.params.len(),
            1,
            "get_client should have 1 param"
        );
        assert!(
            get_client_fn.receiver.is_none(),
            "get_client should not have a receiver"
        );

        let get_jinja = find_builtin_by_path("baml.llm.get_jinja_template");
        assert!(get_jinja.is_some(), "get_jinja_template should be found");
        assert!(
            get_jinja.unwrap().is_sys_op,
            "get_jinja_template should be sys_op"
        );

        // build_primitive_client is also a sys_op in the llm module
        let build_client = find_builtin_by_path("baml.llm.build_primitive_client");
        assert!(
            build_client.is_some(),
            "build_primitive_client should be found"
        );
        assert!(
            build_client.unwrap().is_sys_op,
            "build_primitive_client should be sys_op"
        );

        // Other builtins in the same parent module are also visible
        assert!(find_builtin_by_path("baml.http.Response.text").is_some());
        assert!(find_builtin_by_path("baml.http.fetch").is_some());
    }

    #[test]
    fn test_baaml_easter_egg() {
        // Easter egg: any number of a's in "baml" should work
        assert!(find_builtin_by_path("baaml.Array.length").is_some());
        assert!(find_builtin_by_path("baaaml.Array.length").is_some());
        assert!(find_builtin_by_path("baaaaaaaaml.deep_copy").is_some());

        // Original still works
        assert!(find_builtin_by_path("baml.Array.length").is_some());

        // But not other variations
        assert!(find_builtin_by_path("bml.Array.length").is_none()); // no 'a'
        assert!(find_builtin_by_path("bamll.Array.length").is_none()); // extra 'l'
        assert!(find_builtin_by_path("bbaml.Array.length").is_none()); // extra 'b'
    }

    #[test]
    fn test_normalize_baml_prefix() {
        use std::borrow::Cow;

        // No change needed
        assert!(matches!(
            normalize_baml_prefix("baml.Array"),
            Cow::Borrowed(_)
        ));
        assert!(matches!(normalize_baml_prefix("env.get"), Cow::Borrowed(_)));
        assert!(matches!(normalize_baml_prefix("foo.bar"), Cow::Borrowed(_)));

        // Normalization happens
        assert_eq!(normalize_baml_prefix("baaml.Array"), "baml.Array");
        assert_eq!(normalize_baml_prefix("baaaml.deep_copy"), "baml.deep_copy");
        assert_eq!(
            normalize_baml_prefix("baaaaaaaaml.unstable.string"),
            "baml.unstable.string"
        );

        // Edge cases that should NOT normalize
        assert_eq!(normalize_baml_prefix("bml.Array"), "bml.Array"); // missing 'a'
        assert_eq!(normalize_baml_prefix("ba"), "ba"); // incomplete
        assert_eq!(normalize_baml_prefix("bam"), "bam"); // incomplete
        assert_eq!(normalize_baml_prefix("banal"), "banal"); // different word
    }

    #[test]
    fn test_builtin_types() {
        let types = builtin_types();
        // Should have at least Response, File, Socket
        assert!(
            types.len() >= 3,
            "Expected at least 3 builtin types, got {}",
            types.len()
        );

        // Find Response type
        let response = find_builtin_type("baml.http.Response");
        assert!(response.is_some(), "Response type should exist");
        let response = response.unwrap();

        // Response should have fields: _handle (private), status_code, headers, url
        assert!(
            response.fields.len() >= 4,
            "Response should have at least 4 fields"
        );

        // Check _handle is private and has Resource type
        let handle_field = response.fields.iter().find(|f| f.name == "_handle");
        assert!(handle_field.is_some(), "_handle field should exist");
        let handle_field = handle_field.unwrap();
        assert!(handle_field.is_private, "_handle should be private");
        assert!(
            matches!(handle_field.ty, TypePattern::Resource),
            "private _handle field should have Resource type"
        );

        // Check status_code is public
        let status_field = find_field("baml.http.Response", "status_code");
        assert!(status_field.is_some(), "status_code field should exist");
        let status_field = status_field.unwrap();
        assert!(!status_field.is_private, "status_code should be public");
        assert!(matches!(status_field.ty, TypePattern::Int));

        // Check headers field type is Map<String, String>
        let headers_field = find_field("baml.http.Response", "headers");
        assert!(headers_field.is_some(), "headers field should exist");
        let headers_field = headers_field.unwrap();
        assert!(matches!(headers_field.ty, TypePattern::Map { .. }));
    }
}

// ============================================================================
// Embedded BAML Builtin Files
// ============================================================================

/// Builtin BAML source files for built-in functions.
pub const BUILTIN_PATH_PREFIX: &str = "<builtin>/";
///
/// These files are compiled together with user code and provide
/// implementations for builtin namespaces like `baml.llm`.
///
/// On native targets, source is read from disk at runtime to avoid bloating
/// the binary. On WASM, source is embedded via `include_str!` since filesystem
/// access is not available.
///
/// # Structure
///
/// Files are organized by namespace:
/// - `baml/llm.baml` -> `baml.llm` namespace
pub mod baml_sources {
    /// Compile-time path to the `baml_builtins` crate directory.
    /// Used to locate .baml source files on disk (native only).
    ///
    /// TODO: This needs to be parametrizable. The stdlib will eventually live on the
    /// user's machine (not baked into the binary), so the consumer (e.g. db.rs) should
    /// be able to pass in a custom stdlib path instead of relying on `CARGO_MANIFEST_DIR`.
    #[cfg(not(target_arch = "wasm32"))]
    pub const BUILTINS_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

    /// Embedded source for WASM targets (no filesystem access).
    #[cfg(target_arch = "wasm32")]
    const LLM_EMBEDDED: &str = include_str!("../baml/llm.baml");

    /// A builtin BAML source file with its namespace.
    #[derive(Debug, Clone)]
    pub struct BuiltinSource {
        /// The namespace this file provides (e.g., "baml.llm").
        pub namespace: &'static str,
        /// The virtual file path for diagnostics (e.g., `<builtin>/baml/llm.baml`).
        pub path: &'static str,
        /// The relative path from `BUILTINS_CRATE_DIR` to the .baml file.
        pub relative_path: &'static str,
    }

    /// All builtin BAML sources.
    pub const ALL: &[BuiltinSource] = &[BuiltinSource {
        namespace: "baml.llm",
        path: "<builtin>/baml/llm.baml",
        relative_path: "baml/llm.baml",
    }];

    impl BuiltinSource {
        /// Load the BAML source code for this builtin.
        ///
        /// On native: reads from disk using `BUILTINS_CRATE_DIR`.
        /// On WASM: returns the embedded source.
        pub fn source(&self) -> String {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let fs_path = std::path::Path::new(BUILTINS_CRATE_DIR).join(self.relative_path);
                std::fs::read_to_string(&fs_path).unwrap_or_else(|e| {
                    panic!(
                        "Failed to read builtin BAML source at {}: {}",
                        fs_path.display(),
                        e
                    )
                })
            }
            #[cfg(target_arch = "wasm32")]
            {
                match self.relative_path {
                    "baml/llm.baml" => LLM_EMBEDDED.to_string(),
                    _ => panic!("Unknown builtin source: {}", self.relative_path),
                }
            }
        }
    }
}

/// Get all builtin BAML sources.
pub fn baml_sources() -> impl Iterator<Item = &'static baml_sources::BuiltinSource> {
    baml_sources::ALL.iter()
}

#[cfg(test)]
mod builtin_path_tests {
    use super::{BUILTIN_PATH_PREFIX, baml_sources};

    #[test]
    fn test_builtin_path_prefix_consistent_with_all() {
        for source in baml_sources::ALL {
            assert!(
                source.path.starts_with(BUILTIN_PATH_PREFIX),
                "BuiltinSource path {:?} does not start with BUILTIN_PATH_PREFIX ({:?})",
                source.path,
                BUILTIN_PATH_PREFIX,
            );
        }
    }
}
