pub use baml_codegen_types::Object;
use baml_codegen_types::{Name, Namespace, Ty};

/// Parse a type string into a `Ty`.
///
/// # Supported formats
///
/// - Primitives: `"int"`, `"float"`, `"string"`, `"bool"`, `"null"`
/// - Literals: `"42"`, `"true"`, `"false"`, `"'hello'"`, `"'draft'"`
/// - Optional: `"string?"`, `"User?"`
/// - List: `"string[]"`, `"User[]"`
/// - Nested: `"string[]?"`, `"User?[]"`
/// - Parentheses: `"(string | int)[]"`, `"(A | B)?"`
/// - Class (default): `"User"`, `"MyClass"`
/// - Class with namespace: `"types.User"`, `"stream_types.PartialUser"`
/// - Enum: `"enum.Status"`, `"types.enum.Status"`
///
/// # Examples
///
/// ```
/// use baml_codegen_tests::ty;
///
/// let int_ty = ty("int");
/// let int_lit = ty("42");
/// let str_lit = ty("'draft'");
/// let optional_str = ty("string?");
/// let user_list = ty("User[]");
/// ```
pub fn ty(s: &str) -> Ty {
    parse_ty(s.trim())
}

fn parse_ty(s: &str) -> Ty {
    // Handle optional suffix
    if let Some(inner) = s.strip_suffix('?') {
        return Ty::Optional(Box::new(parse_ty(inner)));
    }

    // Handle list suffix
    if let Some(inner) = s.strip_suffix("[]") {
        return Ty::List(Box::new(parse_ty(inner)));
    }

    // Handle balanced outer parentheses: (A | B) -> A | B
    if let Some(inner) = strip_balanced_parens(s) {
        return parse_ty(inner);
    }

    // Handle stream_state<T>
    if let Some(inner) = s
        .strip_prefix("stream_state<")
        .and_then(|s| s.strip_suffix('>'))
    {
        return Ty::StreamState(Box::new(parse_ty(inner.trim())));
    }

    // Handle map<K, V>
    if let Some(inner) = s.strip_prefix("map<").and_then(|s| s.strip_suffix('>')) {
        if let Some((key, value)) = split_at_depth(inner, ',') {
            return Ty::Map {
                key: Box::new(parse_ty(key.trim())),
                value: Box::new(parse_ty(value.trim())),
            };
        }
    }

    // Handle union (A | B | C) - must respect parentheses/angle bracket depth
    if let Some(types) = split_union_types(s) {
        return Ty::Union(types.into_iter().map(|t| parse_ty(t.trim())).collect());
    }

    // Handle literal values before primitives
    // - Boolean literals: true, false
    match s {
        "true" => return Ty::Literal(baml_base::Literal::Bool(true)),
        "false" => return Ty::Literal(baml_base::Literal::Bool(false)),
        _ => {}
    }

    // - Integer literals: 42, -10
    if let Ok(v) = s.parse::<i64>() {
        return Ty::Literal(baml_base::Literal::Int(v));
    }

    // - Float literals: 3.14, -2.5
    if s.parse::<f64>().is_ok() {
        return Ty::Literal(baml_base::Literal::Float(s.to_string()));
    }

    // - String literals: 'hello', 'draft'
    if let Some(string_val) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ty::Literal(baml_base::Literal::String(string_val.to_string()));
    }

    // Handle primitives
    match s {
        "int" => Ty::Int,
        "float" => Ty::Float,
        "string" => Ty::String,
        "bool" => Ty::Bool,
        "null" => Ty::Null,
        "void" | "unit" => Ty::Unit,
        "image" => Ty::Media(baml_base::MediaKind::Image),
        "audio" => Ty::Media(baml_base::MediaKind::Audio),
        "video" => Ty::Media(baml_base::MediaKind::Video),
        "pdf" => Ty::Media(baml_base::MediaKind::Pdf),
        _ => {
            // Check for enum prefix
            if let Some(rest) = s.strip_prefix("enum.") {
                Ty::Enum(name(rest))
            } else {
                // Default to class
                Ty::Class(name(s))
            }
        }
    }
}

/// Helper to strip outer balanced parentheses if present
fn strip_balanced_parens(s: &str) -> Option<&str> {
    if !s.starts_with('(') || !s.ends_with(')') {
        return None;
    }

    let inner = &s[1..s.len() - 1];
    // Must verify parens are balanced for the WHOLE string, not just (A) | B
    // e.g. "(A) | (B)" starts with ( and ends with ), but stripping them gives "A) | (B" which is invalid
    let mut depth = 0;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    // Closed a parenthesis before the end - so the outer ones aren't a single pair
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    if depth == 0 { Some(inner) } else { None }
}

/// Helper to split union types by `|` respecting nesting depth
fn split_union_types(s: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut last_split = 0;
    let mut found_pipe = false;

    for (i, c) in s.char_indices() {
        match c {
            '(' | '<' => depth += 1,
            ')' | '>' => depth -= 1,
            '|' => {
                if depth == 0 {
                    parts.push(&s[last_split..i]);
                    last_split = i + 1;
                    found_pipe = true;
                }
            }
            _ => {}
        }
    }

    if found_pipe {
        parts.push(&s[last_split..]);
        Some(parts)
    } else {
        None
    }
}

/// Helper to split string at a delimiter respecting nesting depth
fn split_at_depth(s: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '<' => depth += 1,
            ')' | '>' => depth -= 1,
            c if c == delimiter => {
                if depth == 0 {
                    return Some((&s[..i], &s[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a name string into a `Name`.
///
/// Handles explicit namespaces like `"types.User"` or `"stream_types.PartialUser"`.
/// Defaults to `Namespace::Types`.
pub fn name(s: &str) -> Name {
    if let Some(rest) = s.strip_prefix("types.") {
        Name {
            name: rest.into(),
            namespace: Namespace::Types,
        }
    } else if let Some(rest) = s.strip_prefix("stream_types.") {
        Name {
            name: rest.into(),
            namespace: Namespace::StreamTypes,
        }
    } else {
        // Default to Types namespace
        Name {
            name: s.into(),
            namespace: Namespace::Types,
        }
    }
}

/// Macro to construct a `Class` for testing.
///
/// # Examples
///
/// ```
/// use baml_codegen_tests::class;
///
/// // Simple class
/// let user = class!("User" {
///     name: "string",
///     age: "int",
/// });
///
/// // With docstrings
/// let person = class!("Person" @ "A person" {
///     name: "string" @ "The person's name",
///     age: "int",
/// });
/// ```
#[macro_export]
macro_rules! class {
    // String literal name
    (
        $name:literal $(@ $doc:literal)? {
            $($prop_name:ident: $prop_ty:literal $(@ $prop_doc:literal)?),* $(,)?
        }
    ) => {
        $crate::Class {
            name: $crate::name($name),
            docstring: $crate::class!(@opt $($doc)?),
            properties: vec![
                $(
                    $crate::ClassProperty {
                        name: stringify!($prop_name).into(),
                        docstring: $crate::class!(@opt $($prop_doc)?),
                        ty: $crate::ty($prop_ty),
                    },
                )*
            ],
        }
    };
    // Identifier name (for use in other macros)
    (
        $name:ident $(@ $doc:literal)? {
            $($prop_name:ident: $prop_ty:literal $(@ $prop_doc:literal)?),* $(,)?
        }
    ) => {
        $crate::Class {
            name: $crate::name(stringify!($name)),
            docstring: $crate::class!(@opt $($doc)?),
            properties: vec![
                $(
                    $crate::ClassProperty {
                        name: stringify!($prop_name).into(),
                        docstring: $crate::class!(@opt $($prop_doc)?),
                        ty: $crate::ty($prop_ty),
                    },
                )*
            ],
        }
    };
    (@opt $doc:literal) => { Some($doc.into()) };
    (@opt) => { None };
}

/// Macro to construct a `Function` for testing.
///
/// # Examples
///
/// ```
/// use baml_codegen_tests::function;
///
/// // Simple function (no streaming)
/// let greet = function!(fn greet(name: "string") -> "string");
///
/// // With docstrings
/// let process = function!(
///     fn process(input: "string" @ "The input") @ "Process input" -> "string"
/// );
///
/// // Streaming function
/// let get_user = function!(fn get_user(id: "int") -> "User" streams "PartialUser");
/// ```
#[macro_export]
macro_rules! function {
    // Non-streaming function
    (
        fn $name:ident($($arg_name:ident: $arg_ty:literal $(@ $arg_doc:literal)?),* $(,)?)
            $(@ $doc:literal)?
            -> $ret_ty:literal
    ) => {
        $crate::Function {
            name: stringify!($name).into(),
            docstring: $crate::function!(@opt $($doc)?),
            arguments: vec![
                $(
                    $crate::FunctionArgument {
                        name: stringify!($arg_name).into(),
                        docstring: $crate::function!(@opt $($arg_doc)?),
                        ty: $crate::ty($arg_ty),
                    },
                )*
            ],
            return_type: $crate::ty($ret_ty),
            stream_return_type: None,
            watchers: vec![],
        }
    };
    // Streaming function
    (
        fn $name:ident($($arg_name:ident: $arg_ty:literal $(@ $arg_doc:literal)?),* $(,)?)
            $(@ $doc:literal)?
            -> $ret_ty:literal streams $stream_ty:literal
    ) => {
        $crate::Function {
            name: stringify!($name).into(),
            docstring: $crate::function!(@opt $($doc)?),
            arguments: vec![
                $(
                    $crate::FunctionArgument {
                        name: stringify!($arg_name).into(),
                        docstring: $crate::function!(@opt $($arg_doc)?),
                        ty: $crate::ty($arg_ty),
                    },
                )*
            ],
            return_type: $crate::ty($ret_ty),
            stream_return_type: Some($crate::ty($stream_ty)),
            watchers: vec![],
        }
    };
    (@opt $doc:literal) => { Some($doc.into()) };
    (@opt) => { None };
}

/// Macro to construct an `Enum` for testing.
///
/// # Examples
///
/// ```
/// use baml_codegen_tests::r#enum;
///
/// // Simple enum (variant values default to variant names)
/// let status = r#enum!("Status" {
///     Active,
///     Inactive,
/// });
///
/// // With custom values and docstrings
/// let color = r#enum!("Color" @ "A color enum" {
///     Red = "RED" @ "The color red",
///     Blue = "BLUE",
/// });
/// ```
#[macro_export]
macro_rules! r#enum {
    // String literal name
    (
        $name:literal $(@ $doc:literal)? {
            $($variant_name:ident $(= $variant_value:literal)? $(@ $variant_doc:literal)?),* $(,)?
        }
    ) => {
        $crate::Enum {
            name: $crate::name($name),
            docstring: $crate::r#enum!(@opt $($doc)?),
            variants: vec![
                $(
                    $crate::EnumVariant {
                        name: stringify!($variant_name).into(),
                        docstring: $crate::r#enum!(@opt $($variant_doc)?),
                        value: $crate::r#enum!(@value $variant_name $(, $variant_value)?),
                    },
                )*
            ],
        }
    };
    // Identifier name (for use in other macros)
    (
        $name:ident $(@ $doc:literal)? {
            $($variant_name:ident $(= $variant_value:literal)? $(@ $variant_doc:literal)?),* $(,)?
        }
    ) => {
        $crate::Enum {
            name: $crate::name(stringify!($name)),
            docstring: $crate::r#enum!(@opt $($doc)?),
            variants: vec![
                $(
                    $crate::EnumVariant {
                        name: stringify!($variant_name).into(),
                        docstring: $crate::r#enum!(@opt $($variant_doc)?),
                        value: $crate::r#enum!(@value $variant_name $(, $variant_value)?),
                    },
                )*
            ],
        }
    };
    (@opt $doc:literal) => { Some($doc.into()) };
    (@opt) => { None };
    (@value $variant_name:ident, $variant_value:literal) => { $variant_value.into() };
    (@value $variant_name:ident) => { stringify!($variant_name).into() };
}

/// Macro to construct a `TypeAlias` for testing.
///
/// # Examples
///
/// ```
/// use baml_codegen_tests::type_alias;
///
/// let user_id = type_alias!("UserId" = "int");
/// let users = type_alias!("Users" = "User[]");
/// ```
#[macro_export]
macro_rules! type_alias {
    // String literal name
    ($name:literal = $ty:literal) => {
        $crate::TypeAlias {
            name: $crate::name($name),
            resolves_to: $crate::ty($ty),
        }
    };
    // Identifier name (for use in other macros)
    ($name:ident = $ty:literal) => {
        $crate::TypeAlias {
            name: $crate::name(stringify!($name)),
            resolves_to: $crate::ty($ty),
        }
    };
}

/// Macro to construct an `ObjectPool` for testing.
///
/// # Examples
///
/// ```
/// use baml_codegen_tests::object_pool;
///
/// let pool = object_pool! {
///     class User {
///         name: "string",
///         age: "int",
///     },
///     enum Status {
///         Active,
///         Inactive,
///     },
///     type UserId = "int",
///     fn greet(name: "string") -> "string",
/// };
///
/// assert_eq!(pool.len(), 4);
/// ```
#[macro_export]
macro_rules! object_pool {
    // Empty pool
    () => {{
        let pool: $crate::ObjectPool = std::collections::HashMap::new();
        pool
    }};

    // Entry point - use internal muncher
    ($($items:tt)+) => {{
        let mut pool: $crate::ObjectPool = std::collections::HashMap::new();
        $crate::object_pool_insert!(pool; $($items)+);
        pool
    }};
}

/// Internal helper macro for object_pool - inserts items one by one.
#[macro_export]
#[doc(hidden)]
macro_rules! object_pool_insert {
    // Done - no more items
    ($pool:ident;) => {};

    // Class item with string literal name (for specifying namespace, e.g., "stream_types.PartialUser")
    ($pool:ident; class $name:literal $(@ $doc:literal)? { $($body:tt)* } $(, $($rest:tt)*)?) => {
        let obj = $crate::class!($name $(@ $doc)? { $($body)* });
        $pool.insert(obj.name.clone(), $crate::Object::Class(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Class item with identifier name
    ($pool:ident; class $name:ident $(@ $doc:literal)? { $($body:tt)* } $(, $($rest:tt)*)?) => {
        let obj = $crate::class!($name $(@ $doc)? { $($body)* });
        $pool.insert(obj.name.clone(), $crate::Object::Class(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Enum item with string literal name (for specifying namespace)
    ($pool:ident; enum $name:literal $(@ $doc:literal)? { $($body:tt)* } $(, $($rest:tt)*)?) => {
        let obj = $crate::r#enum!($name $(@ $doc)? { $($body)* });
        $pool.insert(obj.name.clone(), $crate::Object::Enum(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Enum item with identifier name
    ($pool:ident; enum $name:ident $(@ $doc:literal)? { $($body:tt)* } $(, $($rest:tt)*)?) => {
        let obj = $crate::r#enum!($name $(@ $doc)? { $($body)* });
        $pool.insert(obj.name.clone(), $crate::Object::Enum(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Type alias item with string literal name (for specifying namespace)
    ($pool:ident; type $name:literal = $ty:literal $(, $($rest:tt)*)?) => {
        let obj = $crate::type_alias!($name = $ty);
        $pool.insert(obj.name.clone(), $crate::Object::TypeAlias(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Type alias item with identifier name
    ($pool:ident; type $name:ident = $ty:literal $(, $($rest:tt)*)?) => {
        let obj = $crate::type_alias!($name = $ty);
        $pool.insert(obj.name.clone(), $crate::Object::TypeAlias(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Streaming function item (must come before non-streaming to match first)
    // Inserts under both Types (for sync) and StreamTypes (for async) namespaces
    // Constructs the function twice to avoid requiring Clone
    ($pool:ident; fn $name:ident($($args:tt)*) $(@ $doc:literal)? -> $ret:literal streams $stream:literal $(, $($rest:tt)*)?) => {
        // Insert for sync client (Types namespace)
        let sync_key = $crate::Name {
            name: stringify!($name).into(),
            namespace: $crate::Namespace::Types,
        };
        $pool.insert(sync_key, $crate::Object::Function(
            $crate::function!(fn $name($($args)*) $(@ $doc)? -> $ret streams $stream)
        ));
        // Insert for async/streaming client (StreamTypes namespace)
        let stream_key = $crate::Name {
            name: stringify!($name).into(),
            namespace: $crate::Namespace::StreamTypes,
        };
        $pool.insert(stream_key, $crate::Object::Function(
            $crate::function!(fn $name($($args)*) $(@ $doc)? -> $ret streams $stream)
        ));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };

    // Non-streaming function item
    ($pool:ident; fn $name:ident($($args:tt)*) $(@ $doc:literal)? -> $ret:literal $(, $($rest:tt)*)?) => {
        let obj = $crate::function!(fn $name($($args)*) $(@ $doc)? -> $ret);
        let key = $crate::Name {
            name: obj.name.clone(),
            namespace: $crate::Namespace::Types,
        };
        $pool.insert(key, $crate::Object::Function(obj));
        $($crate::object_pool_insert!($pool; $($rest)*);)?
    };
}

// Macro to define fixtures and generate all necessary boilerplate
#[macro_export]
macro_rules! define_fixtures {
    (
        @dollar $d:tt;
        $(
            $(#[$meta:meta])*
            $fixture_name:ident => $body:block
        ),* $(,)?
    ) => {
        /// Fixture functions that return `ObjectPool` for testing.
        pub mod fixtures {

            $(
                $(#[$meta])*
                pub fn $fixture_name() -> $crate::ObjectPool $body
            )*
        }

        /// Array of all fixture names (for tooling that needs to iterate over all fixtures).
        pub const FIXTURE_NAMES: &[&str] = &[
            $(
                stringify!($fixture_name),
            )*
        ];

        paste::paste! {
            /// Trait that codegen crates must implement to test all fixtures.
            pub trait FixtureTests {
                /// The output type for this codegen (e.g., a wrapper around generated files).
                type Output;

                /// Create an output from an `ObjectPool`.
                fn create_output(pool: &$crate::ObjectPool) -> Self::Output;

                /// Convert output to a snapshot string (e.g., concatenate all files).
                fn snapshot_output(output: &Self::Output) -> String;
            }

            /// Macro to generate test functions from a `FixtureTests` implementation.
            #[macro_export]
            macro_rules! fixture_tests {
                // Define a unit struct and generate tests
                (struct $d name:ident) => {
                    struct $d name;
                    $crate::fixture_tests!($d name);
                };

                // Generate tests for an existing type
                ($d impl_type:ty) => {
                    $(
                        #[test]
                        fn [<test_ $fixture_name>]() {
                            let pool = $crate::fixtures::$fixture_name();
                            let output = <$d impl_type as $crate::FixtureTests>::create_output(&pool);
                            let snapshot = <$d impl_type as $crate::FixtureTests>::snapshot_output(&output);
                            insta::assert_snapshot!(snapshot);
                        }
                    )*
                };
            }
        }
    };
}

/// Zero-boilerplate macro for setting up codegen tests.
///
/// Just provide your codegen function and the files to snapshot.
///
/// # Example
/// ```rust,ignore
/// baml_codegen_tests::codegen_tests! {
///     codegen: to_source_code,
///     files: ["types.py", "stream_types.py"],
/// }
/// ```
#[macro_export]
macro_rules! codegen_tests {
    // With trailing comma
    (
        codegen: $codegen_fn:path,
        files: [$($file:literal,)+]
    ) => {
        $crate::codegen_tests! {
            codegen: $codegen_fn,
            files: [$($file),+]
        }
    };
    // Without trailing comma
    (
        codegen: $codegen_fn:path,
        files: [$($file:literal),+]
    ) => {
        use std::path::PathBuf;
        use std::collections::HashMap;
        use $crate::{FixtureTests, ObjectPool};

        struct Output(HashMap<PathBuf, String>);

        impl Output {
            fn new(pool: &ObjectPool) -> Self {
                Self($codegen_fn(pool, &PathBuf::from(".")))
            }

            fn all_files(&self) -> String {
                let files = [$($file),+];

                format!("# All files\n\n{}", files
                    .iter()
                    .map(|name| {
                        let content = self.0
                            .get(&PathBuf::from(name))
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        let lang = if name.ends_with(".py") || name.ends_with(".pyi") {
                            "python"
                        } else if name.ends_with(".ts") || name.ends_with(".tsx") {
                            "typescript"
                        } else if name.ends_with(".js") || name.ends_with(".jsx") {
                            "javascript"
                        } else if name.ends_with(".rs") {
                            "rust"
                        } else if name.ends_with(".java") {
                            "java"
                        } else if name.ends_with(".go") {
                            "go"
                        } else if name.ends_with(".rb") {
                            "ruby"
                        } else {
                            ""
                        };
                        format!("## {name}\n\n```{lang}\n{content}\n```")
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"))
            }
        }

        $crate::fixture_tests!(struct CodegenImpl);

        impl FixtureTests for CodegenImpl {
            type Output = Output;

            fn create_output(pool: &ObjectPool) -> Self::Output {
                Output::new(pool)
            }

            fn snapshot_output(output: &Self::Output) -> String {
                output.all_files()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ty_primitives() {
        assert_eq!(ty("int"), Ty::Int);
        assert_eq!(ty("float"), Ty::Float);
        assert_eq!(ty("string"), Ty::String);
        assert_eq!(ty("bool"), Ty::Bool);
        assert_eq!(ty("null"), Ty::Null);
    }

    #[test]
    fn test_ty_literals() {
        // Integer literals
        assert_eq!(ty("42"), Ty::Literal(baml_base::Literal::Int(42)));
        assert_eq!(ty("-10"), Ty::Literal(baml_base::Literal::Int(-10)));

        // Boolean literals
        assert_eq!(ty("true"), Ty::Literal(baml_base::Literal::Bool(true)));
        assert_eq!(ty("false"), Ty::Literal(baml_base::Literal::Bool(false)));

        // String literals
        assert_eq!(
            ty("'draft'"),
            Ty::Literal(baml_base::Literal::String("draft".to_string()))
        );
        assert_eq!(
            ty("'hello world'"),
            Ty::Literal(baml_base::Literal::String("hello world".to_string()))
        );
    }

    #[test]
    fn test_ty_optional() {
        assert_eq!(ty("string?"), Ty::Optional(Box::new(Ty::String)));
        assert_eq!(ty("int?"), Ty::Optional(Box::new(Ty::Int)));
    }

    #[test]
    fn test_ty_list() {
        assert_eq!(ty("string[]"), Ty::List(Box::new(Ty::String)));
        assert_eq!(ty("int[]"), Ty::List(Box::new(Ty::Int)));
    }

    #[test]
    fn test_ty_nested() {
        // Optional list
        assert_eq!(
            ty("string[]?"),
            Ty::Optional(Box::new(Ty::List(Box::new(Ty::String))))
        );
        // List of optionals
        assert_eq!(
            ty("string?[]"),
            Ty::List(Box::new(Ty::Optional(Box::new(Ty::String))))
        );
    }

    #[test]
    fn test_ty_class() {
        assert_eq!(
            ty("User"),
            Ty::Class(Name {
                name: "User".into(),
                namespace: Namespace::Types,
            })
        );
        assert_eq!(
            ty("stream_types.PartialUser"),
            Ty::Class(Name {
                name: "PartialUser".into(),
                namespace: Namespace::StreamTypes,
            })
        );
    }

    #[test]
    fn test_ty_enum() {
        assert_eq!(
            ty("enum.Status"),
            Ty::Enum(Name {
                name: "Status".into(),
                namespace: Namespace::Types,
            })
        );
    }

    #[test]
    fn test_ty_map() {
        assert_eq!(
            ty("map<string, int>"),
            Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::Int),
            }
        );
        assert_eq!(
            ty("map<string, User[]>"),
            Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::List(Box::new(Ty::Class(Name {
                    name: "User".into(),
                    namespace: Namespace::Types,
                })))),
            }
        );
    }

    #[test]
    fn test_ty_union() {
        assert_eq!(ty("string | int"), Ty::Union(vec![Ty::String, Ty::Int]));
        assert_eq!(
            ty("string | int | bool"),
            Ty::Union(vec![Ty::String, Ty::Int, Ty::Bool])
        );
    }

    #[test]
    fn test_ty_parentheses() {
        // Simple parentheses just unwrap
        assert_eq!(ty("(string)"), Ty::String);

        // Parentheses for grouping union as list element: (A | B)[]
        assert_eq!(
            ty("(string | int)[]"),
            Ty::List(Box::new(Ty::Union(vec![Ty::String, Ty::Int])))
        );

        // Parentheses for grouping union as optional: (A | B)?
        assert_eq!(
            ty("(string | int)?"),
            Ty::Optional(Box::new(Ty::Union(vec![Ty::String, Ty::Int])))
        );

        // Union of parenthesized expressions: (A) | (B) shouldn't strip outer parens incorrectly
        assert_eq!(ty("(string) | (int)"), Ty::Union(vec![Ty::String, Ty::Int]));

        // Complex nested: ((A | B)[] | C)?
        assert_eq!(
            ty("((string | int)[] | bool)?"),
            Ty::Optional(Box::new(Ty::Union(vec![
                Ty::List(Box::new(Ty::Union(vec![Ty::String, Ty::Int]))),
                Ty::Bool,
            ])))
        );
    }

    #[test]
    fn test_name() {
        assert_eq!(
            name("User"),
            Name {
                name: "User".into(),
                namespace: Namespace::Types,
            }
        );
        assert_eq!(
            name("types.User"),
            Name {
                name: "User".into(),
                namespace: Namespace::Types,
            }
        );
        assert_eq!(
            name("stream_types.Partial"),
            Name {
                name: "Partial".into(),
                namespace: Namespace::StreamTypes,
            }
        );
    }

    #[test]
    fn test_class_macro() {
        let c = class!("User" {
            name: "string",
            age: "int",
        });
        assert_eq!(c.name.name.to_string(), "User");
        assert_eq!(c.properties.len(), 2);
        assert_eq!(c.properties[0].name.to_string(), "name");
        assert_eq!(c.properties[0].ty, Ty::String);
    }

    #[test]
    fn test_class_macro_with_docs() {
        let c = class!("Person" @ "A person" {
            name: "string" @ "The name",
            age: "int",
        });
        assert_eq!(c.docstring, Some("A person".into()));
        assert_eq!(c.properties[0].docstring, Some("The name".into()));
        assert_eq!(c.properties[1].docstring, None);
    }

    #[test]
    fn test_function_macro() {
        let f = function!(fn greet(name: "string") -> "string");
        assert_eq!(f.name.to_string(), "greet");
        assert_eq!(f.arguments.len(), 1);
        assert_eq!(f.arguments[0].name.to_string(), "name");
        assert_eq!(f.return_type, Ty::String);
    }

    #[test]
    fn test_function_macro_with_docs() {
        let f = function!(
            fn process(input: "string" @ "The input") @ "Process" -> "int"
        );
        assert_eq!(f.docstring, Some("Process".into()));
        assert_eq!(f.arguments[0].docstring, Some("The input".into()));
    }

    #[test]
    fn test_enum_macro() {
        let e = r#enum!("Status" {
            Active,
            Inactive,
        });
        assert_eq!(e.name.name.to_string(), "Status");
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name.to_string(), "Active");
        assert_eq!(e.variants[0].value, "Active");
    }

    #[test]
    fn test_enum_macro_with_values() {
        let e = r#enum!("Color" @ "Colors" {
            Red = "RED" @ "Red color",
            Blue = "BLUE",
        });
        assert_eq!(e.docstring, Some("Colors".into()));
        assert_eq!(e.variants[0].value, "RED");
        assert_eq!(e.variants[0].docstring, Some("Red color".into()));
        assert_eq!(e.variants[1].value, "BLUE");
        assert_eq!(e.variants[1].docstring, None);
    }

    #[test]
    fn test_type_alias_macro() {
        let ta = type_alias!("UserId" = "int");
        assert_eq!(ta.name.name.to_string(), "UserId");
        assert_eq!(ta.resolves_to, Ty::Int);

        let ta2 = type_alias!("Users" = "User[]");
        assert_eq!(
            ta2.resolves_to,
            Ty::List(Box::new(Ty::Class(Name {
                name: "User".into(),
                namespace: Namespace::Types,
            })))
        );
    }

    #[test]
    fn test_object_pool_macro() {
        let pool = object_pool! {
            class User {
                name: "string",
                age: "int",
            },
            enum Status {
                Active,
                Inactive,
            },
            type UserId = "int",
            fn greet(name: "string") -> "string",
        };

        assert_eq!(pool.len(), 4);

        // Check class
        let user_key = name("User");
        assert!(matches!(pool.get(&user_key), Some(Object::Class(_))));

        // Check enum
        let status_key = name("Status");
        assert!(matches!(pool.get(&status_key), Some(Object::Enum(_))));

        // Check type alias
        let user_id_key = name("UserId");
        assert!(matches!(pool.get(&user_id_key), Some(Object::TypeAlias(_))));

        // Check function
        let greet_key = name("greet");
        assert!(matches!(pool.get(&greet_key), Some(Object::Function(_))));
    }

    #[test]
    fn test_object_pool_empty() {
        let pool = object_pool! {};
        assert!(pool.is_empty());
    }

    #[test]
    fn test_object_pool_single_item() {
        let pool = object_pool! {
            class Person {
                name: "string",
                age: "int",
            },
        };

        assert_eq!(pool.len(), 1);
        assert!(matches!(pool.get(&name("Person")), Some(Object::Class(_))));
    }
}
