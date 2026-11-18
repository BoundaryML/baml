//! Code generation for the ahead-of-time native backend.
//!
//! From the same builtin records the interpreter's glue is generated from,
//! this writes the two sides of a native call: for the compiler, a table of
//! every Rust-declared builtin with the symbol and calling convention of its
//! native implementation; for the native runtime, one trait per class with a
//! method per implemented builtin, and the `extern "C"` function behind each
//! symbol.
//!
//! Which builtins the native runtime implements is listed in its `natives`
//! directory, one `<path> [flags]` per line. A builtin not listed there is in
//! the compiler's table all the same, marked as not ported.

use std::{collections::BTreeMap, fmt::Write as _, path::Path};

use crate::types::{BamlType, NativeBuiltin, NativeClassDef};

/// What the native runtime's lists promise of a native. The promises are
/// explained in the runtime's `natives/README.md`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct NativeFlags {
    /// May allocate, and so collect. It is passed roots and a scope, where
    /// a native without the flag is passed borrowed views.
    pub allocates: bool,
    /// May stop and go on later: it is an `async fn`. Implies `allocates`.
    pub is_async: bool,
}

impl NativeFlags {
    fn parse<'a>(words: impl Iterator<Item = &'a str>) -> Option<Self> {
        let mut flags = Self::default();
        for word in words {
            match word {
                "allocates" if !flags.allocates => flags.allocates = true,
                "async" if !flags.is_async => flags.is_async = true,
                _ => return None,
            }
        }
        flags.allocates |= flags.is_async;
        Some(flags)
    }
}

/// One line of the native runtime's lists: a builtin it implements.
#[derive(Clone, Debug)]
pub struct NativeListEntry {
    /// The builtin's dotted path, as its declaration has it.
    pub path: String,
    pub flags: NativeFlags,
}

/// Read every `*.txt` file of `dir`: a `<path> [flags]` per line, with blank
/// lines and `#` comments skipped.
pub fn read_native_lists(dir: &Path) -> Result<Vec<NativeListEntry>, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    files.sort();
    let mut entries = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("cannot read `{}`: {error}", file.display()))?;
        for (number, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let path = words.next().unwrap_or_default();
            let Some(flags) = NativeFlags::parse(words) else {
                return Err(format!(
                    "{}:{}: expected `<path>` and then any of `allocates` and `async`",
                    file.display(),
                    number + 1
                ));
            };
            entries.push(NativeListEntry {
                path: path.to_owned(),
                flags,
            });
        }
    }
    Ok(entries)
}

/// A type a native call can pass or return.
#[derive(Clone, PartialEq, Eq, Debug)]
enum AbiTy {
    Int,
    Float,
    Bool,
    Null,
    String,
    Bigint,
    Array(Box<AbiTy>),
    /// A map from the builtin's first type parameter to its second: the
    /// receiver of a method of `Map`.
    Map,
    /// A type or `null`: a reference that may be zero, or for an `int`,
    /// `float` or `bool` a pair of words, the value's bits and whether
    /// there is a value.
    Optional(Box<AbiTy>),
    /// The builtin's type parameter at this position, class parameters
    /// first: a value of whatever type a call names, as the one word an
    /// array or a field holds it in. Whether that word is a reference is
    /// known from the call's type id for the parameter.
    Elem(usize),
    /// A value of any type as a union holds it: a reference whose object's
    /// header says what it is, zero for `null`. What `baml.json` takes and
    /// returns a `json` as.
    Any,
    /// An `unknown` parameter of `baml.json`: the id of the description of
    /// the argument's static type, zero when it has none, and then the
    /// value as [`Self::Any`].
    Typed,
    /// The builtin's type parameter at this position, as [`Self::Any`]:
    /// what `baml.json` returns a value it decoded as.
    BoxedElem(usize),
    /// An instance of a class of the standard library that takes no type
    /// arguments: its namespace under `baml`, dotted, and its name.
    Class(String, String),
    /// A `$rust_type` value: a reference to the object that holds the Rust
    /// state.
    Rust,
}

/// The classes of the `baml` package a signature can name: those without
/// type parameters, each as its dotted namespace and its name.
struct Classes<'a> {
    known: Vec<(&'a str, &'a str)>,
}

impl<'a> Classes<'a> {
    fn new(classes: &'a [NativeClassDef]) -> Self {
        let known = classes
            .iter()
            .filter(|class| class.generic_params.is_empty())
            .filter_map(|class| {
                let namespace = match class.namespace_prefix.split_once('.') {
                    Some(("baml", namespace)) => namespace,
                    None if class.namespace_prefix == "baml" => "",
                    _ => return None,
                };
                Some((namespace, class.name.as_str()))
            })
            .collect();
        Self { known }
    }

    /// The class a declaration in `namespace` names by `written`: a name
    /// of its own namespace, or a path from the package's root.
    fn resolve(&self, namespace: &str, written: &str) -> Option<AbiTy> {
        let (namespace, name) = match written
            .strip_prefix("root.")
            .or_else(|| written.strip_prefix("baml."))
        {
            Some(path) => path.rsplit_once('.').unwrap_or(("", path)),
            None if written.contains('.') => return None,
            None => (namespace, written),
        };
        self.known
            .contains(&(namespace, name))
            .then(|| AbiTy::Class(namespace.to_owned(), name.to_owned()))
    }
}

/// What a type of a builtin's declaration is read against.
struct Scope<'a> {
    generics: Vec<&'a str>,
    /// The namespace the builtin is declared in.
    namespace: &'a str,
    classes: &'a Classes<'a>,
}

impl AbiTy {
    /// The type as a builtin of `baml.json` takes or returns it.
    fn of_json(ty: &BamlType, scope: &Scope<'_>) -> Option<Self> {
        let generics = &scope.generics;
        match ty {
            BamlType::Named(name) if name == "unknown" => Some(Self::Typed),
            BamlType::Named(name)
                if matches!(
                    name.as_str(),
                    "json" | "baml.json.json" | "root.json.json" | "union"
                ) =>
            {
                Some(Self::Any)
            }
            BamlType::Generic(name) => generics
                .iter()
                .position(|generic| generic == name)
                .map(Self::BoxedElem),
            other => Self::of(other, scope),
        }
    }

    fn of(ty: &BamlType, scope: &Scope<'_>) -> Option<Self> {
        let generics = &scope.generics;
        match ty {
            BamlType::Named(name) => scope.classes.resolve(scope.namespace, name),
            BamlType::RustType => Some(Self::Rust),
            BamlType::Generic(name) => generics
                .iter()
                .position(|generic| generic == name)
                .map(Self::Elem),
            BamlType::Int => Some(Self::Int),
            BamlType::Float => Some(Self::Float),
            BamlType::Bool => Some(Self::Bool),
            BamlType::Null => Some(Self::Null),
            BamlType::String => Some(Self::String),
            BamlType::Bigint => Some(Self::Bigint),
            BamlType::List(element) => match Self::of(element, scope)? {
                Self::Null | Self::Optional(_) => None,
                element => Some(Self::Array(Box::new(element))),
            },
            BamlType::Optional(inner) => match Self::of(inner, scope)? {
                Self::Null | Self::Optional(_) => None,
                inner => Some(Self::Optional(Box::new(inner))),
            },
            _ => None,
        }
    }

    fn is_ref(&self) -> bool {
        match self {
            Self::String
            | Self::Bigint
            | Self::Array(_)
            | Self::Map
            | Self::Class(..)
            | Self::Rust => true,
            Self::Optional(inner) => inner.is_ref(),
            _ => false,
        }
    }

    /// Whether a value is an optional `int`, `float` or `bool`, or an
    /// optional value of a type parameter: two words.
    fn is_pair(&self) -> bool {
        matches!(self, Self::Optional(inner) if !inner.is_ref())
    }

    /// The compiler's description of the type.
    fn table(&self) -> String {
        match self {
            Self::Int => "Ty::Int".to_owned(),
            Self::Float => "Ty::Float".to_owned(),
            Self::Bool => "Ty::Bool".to_owned(),
            Self::Null => "Ty::Null".to_owned(),
            Self::String => "Ty::String".to_owned(),
            Self::Bigint => "Ty::Bigint".to_owned(),
            Self::Array(element) => format!("Ty::Array(&{})", element.table()),
            Self::Map => "Ty::Map".to_owned(),
            Self::Optional(inner) => format!("Ty::Optional(&{})", inner.table()),
            Self::Elem(index) => format!("Ty::Elem({index})"),
            Self::Any => "Ty::Any".to_owned(),
            Self::Typed => "Ty::Typed".to_owned(),
            Self::BoxedElem(index) => format!("Ty::BoxedElem({index})"),
            Self::Class(namespace, name) => format!("Ty::Class({})", class_path(namespace, name)),
            Self::Rust => "Ty::Rust".to_owned(),
        }
    }

    /// The classes the type names.
    fn classes(&self, into: &mut Vec<String>) {
        match self {
            Self::Class(namespace, name) => {
                let path = class_path(namespace, name);
                if !into.contains(&path) {
                    into.push(path);
                }
            }
            Self::Array(inner) | Self::Optional(inner) => inner.classes(into),
            _ => {}
        }
    }

    /// The type as the symbol returns it.
    fn c(&self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Float => "f64",
            Self::Bool => "u8",
            Self::Null => "()",
            Self::Optional(inner) if !inner.is_ref() => "OptionalScalar",
            Self::String
            | Self::Bigint
            | Self::Array(_)
            | Self::Map
            | Self::Optional(_)
            | Self::Elem(_)
            | Self::Any
            | Self::Typed
            | Self::BoxedElem(_)
            | Self::Class(..)
            | Self::Rust => "usize",
        }
    }

    /// The symbol's parameters for a parameter `name` of this type: one,
    /// or for an optional scalar its bits and then whether it is present.
    fn c_parameters(&self, name: &str) -> Vec<String> {
        if self.is_pair() {
            vec![format!("r#{name}: u64"), format!("{name}_present: u64")]
        } else if *self == Self::Typed {
            vec![format!("{name}_type: u32"), format!("r#{name}: usize")]
        } else {
            vec![format!("r#{name}: {}", self.c())]
        }
    }

    /// The marker naming what a reference refers to, or an array holds.
    fn marker(&self) -> String {
        match self {
            Self::Int => "Int".to_owned(),
            Self::Float => "Float".to_owned(),
            Self::Bool => "Bool".to_owned(),
            Self::Null => "()".to_owned(),
            Self::String => "Str".to_owned(),
            Self::Bigint => "Big".to_owned(),
            Self::Array(element) => format!("Arr<{}>", element.marker()),
            Self::Map => "Map".to_owned(),
            Self::Class(..) => "Obj".to_owned(),
            Self::Rust => "Opaque".to_owned(),
            Self::Optional(inner) => inner.marker(),
            Self::Elem(_) | Self::Any | Self::Typed | Self::BoxedElem(_) => "Slot".to_owned(),
        }
    }

    /// The type as a trait method takes it: a root when the native
    /// `allocates`, a view otherwise.
    fn parameter(&self, allocates: bool) -> String {
        match (self, allocates) {
            (Self::Optional(inner), _) if matches!(**inner, Self::Elem(_)) => {
                "Option<usize>".to_owned()
            }
            (Self::Optional(inner), _) => format!("Option<{}>", inner.parameter(allocates)),
            (Self::Elem(_) | Self::Any | Self::Typed | Self::BoxedElem(_), false) => {
                "usize".to_owned()
            }
            (Self::Elem(_) | Self::Any | Self::Typed | Self::BoxedElem(_), true) => {
                "Elem<'s>".to_owned()
            }
            (Self::Int, _) => "i64".to_owned(),
            (Self::Float, _) => "f64".to_owned(),
            (Self::Bool, _) => "bool".to_owned(),
            (Self::Null, _) => "()".to_owned(),
            (Self::String, false) => "StrRef<'_>".to_owned(),
            (Self::Bigint, false) => "BigRef<'_>".to_owned(),
            (Self::Array(element), false) => {
                format!("ArrRef<'_, {}>", element.marker())
            }
            (Self::Map, false) => "MapRef<'_>".to_owned(),
            (Self::Class(..), false) => "ObjRef<'_>".to_owned(),
            (Self::Rust, false) => "OpaqueRef<'_>".to_owned(),
            (_, true) => format!("Root<'s, {}>", self.marker()),
        }
    }

    /// The expression making a trait method's argument out of the symbol's
    /// parameter `name`.
    fn argument(&self, name: &str, allocates: bool) -> String {
        match (self, allocates) {
            (Self::Int | Self::Float, _)
            | (Self::Elem(_) | Self::Any | Self::Typed | Self::BoxedElem(_), false) => {
                name.to_owned()
            }
            // Any value at all: a reference, whatever it refers to.
            (Self::Any | Self::Typed | Self::BoxedElem(_), true) => {
                format!("unsafe {{ scope.elem_raw({name}, TypeId(0)) }}")
            }
            (Self::Elem(index), true) => {
                format!("unsafe {{ scope.elem_raw({name}, types[{index}]) }}")
            }
            (Self::Bool, _) => format!("{name} != 0"),
            (Self::Null, _) => "()".to_owned(),
            (Self::Optional(inner), _) if inner.is_ref() => {
                format!("({name} != 0).then(|| {})", inner.argument(name, allocates))
            }
            (Self::Optional(inner), _) => {
                let present = format!("{}_present", name.trim_start_matches("r#"));
                let value = match **inner {
                    Self::Float => format!("f64::from_bits({name})"),
                    Self::Bool => format!("{name} != 0"),
                    _ => format!("{name}.cast_signed()"),
                };
                format!("({present} != 0).then_some({value})")
            }
            (_, false) => format!("unsafe {{ View::from_raw({name}) }}"),
            (_, true) => format!("unsafe {{ scope.root_raw({name}) }}"),
        }
    }

    /// The expression turning a trait method's result `value` into the bits
    /// of a fallible symbol's value word.
    fn bits(&self, value: &str) -> String {
        match self {
            Self::Int => format!("{value}.cast_unsigned()"),
            Self::Float => format!("{value}.to_bits()"),
            Self::Bool => format!("u64::from({value})"),
            Self::Null => format!("{{ let () = {value}; 0 }}"),
            Self::String
            | Self::Bigint
            | Self::Array(_)
            | Self::Map
            | Self::Class(..)
            | Self::Rust => {
                format!("{value}.raw(&scope) as u64")
            }
            Self::Elem(_) | Self::Any | Self::Typed | Self::BoxedElem(_) => {
                format!("{value} as u64")
            }
            // An optional scalar has no fallible form: see `listed`.
            Self::Optional(_) => format!("{value}.map_or(0, |value| value.raw(&scope) as u64)"),
        }
    }

    /// The expression turning a trait method's result `value` into what an
    /// infallible symbol returns.
    fn returned(&self, value: &str) -> String {
        match self {
            Self::Int
            | Self::Float
            | Self::Null
            | Self::Elem(_)
            | Self::Any
            | Self::Typed
            | Self::BoxedElem(_) => value.to_owned(),
            Self::Bool => format!("u8::from({value})"),
            Self::String
            | Self::Bigint
            | Self::Array(_)
            | Self::Map
            | Self::Class(..)
            | Self::Rust => format!("{value}.raw(&scope)"),
            Self::Optional(inner) if inner.is_ref() => {
                format!("{value}.map_or(0, |value| value.raw(&scope))")
            }
            Self::Optional(_) => format!("OptionalScalar::from({value})"),
        }
    }
}

/// A class as the compiler's table names it: the parts of its namespace
/// under `baml`, and its name.
fn class_path(namespace: &str, name: &str) -> String {
    let parts: Vec<String> = namespace
        .split('.')
        .filter(|part| !part.is_empty())
        .map(|part| format!("{part:?}"))
        .collect();
    format!("(&[{}], {name:?})", parts.join(", "))
}

/// The ABI of a builtin, when every type in its signature has one.
struct Abi {
    params: Vec<(String, AbiTy)>,
    result: AbiTy,
    /// The classes its declaration names, in what it takes, returns and
    /// throws: what the native can be passed or make an instance of.
    classes: Vec<String>,
}

/// The builtin's type parameters, in the order a call passes their types:
/// its class's, then its own. The record lists both together.
fn generics(builtin: &NativeBuiltin) -> Vec<&str> {
    builtin.generics.iter().map(String::as_str).collect()
}

fn abi_of(builtin: &NativeBuiltin, classes: &Classes<'_>) -> Option<Abi> {
    // Only a builtin of `baml` names the package's classes by their bare
    // names.
    let none = Classes { known: Vec::new() };
    let scope = Scope {
        generics: generics(builtin),
        namespace: &builtin.namespace,
        classes: if builtin.path.starts_with("baml.") {
            classes
        } else {
            &none
        },
    };
    let classes = scope.classes;
    let mut params = Vec::new();
    if let Some(receiver) = &builtin.receiver {
        if !receiver.receiver_type.is_static() {
            let ty = match receiver.builtin_name() {
                "String" => AbiTy::String,
                "Bigint" => AbiTy::Bigint,
                "Array" => AbiTy::Array(Box::new(AbiTy::Elem(0))),
                "Map" => AbiTy::Map,
                "Float" => AbiTy::Float,
                "Int" => AbiTy::Int,
                "Bool" => AbiTy::Bool,
                _ if receiver.instance_backed && receiver.class_generics.is_empty() => {
                    classes.resolve(&receiver.namespace, &receiver.class_name)?
                }
                _ => return None,
            };
            params.push((receiver.class_name.to_lowercase(), ty));
        }
    }
    // `baml.json` moves values of any type, as a union holds them, and so
    // do the two defaults a `to_json` and a `from_json` fall back on.
    let of = if builtin.path.starts_with("baml.json.")
        || matches!(
            builtin.path.as_str(),
            "baml._to_json_default" | "baml._from_json_structural_default"
        ) {
        AbiTy::of_json
    } else {
        AbiTy::of
    };
    for param in &builtin.params {
        match of(&param.ty, &scope)? {
            AbiTy::Null => return None,
            // The two words of an optional value of a type parameter are
            // only ever a result.
            AbiTy::Optional(inner) if matches!(*inner, AbiTy::Elem(_)) => return None,
            ty => params.push((param.name.clone(), ty)),
        }
    }
    let result = of(&builtin.return_type, &scope)?;
    let mut named = Vec::new();
    for ty in params.iter().map(|(_, ty)| ty).chain([&result]) {
        ty.classes(&mut named);
    }
    // A category of `baml.errors` is written without its namespace.
    for thrown in builtin.throws.iter().flatten() {
        let class = classes
            .resolve(scope.namespace, thrown)
            .or_else(|| classes.resolve("errors", thrown));
        if let Some(class) = class {
            class.classes(&mut named);
        }
    }
    Some(Abi {
        params,
        result,
        classes: named,
    })
}

fn is_fallible(builtin: &NativeBuiltin) -> bool {
    builtin.fallible
        || builtin
            .throws
            .as_ref()
            .is_none_or(|throws| !throws.is_empty())
}

/// Whether the native's symbol returns a thrown object beside its value. A
/// native that does not allocate throws nothing, whatever the declaration
/// allows: an error is an object. One that is `async` can always throw,
/// since the BAML it calls can.
fn throws(builtin: &NativeBuiltin, flags: Option<NativeFlags>) -> bool {
    match flags {
        Some(flags) => flags.is_async || (flags.allocates && is_fallible(builtin)),
        None => is_fallible(builtin),
    }
}

/// How many type arguments a call of the builtin passes.
fn type_args(builtin: &NativeBuiltin) -> usize {
    builtin.generics.len()
}

fn symbol(builtin: &NativeBuiltin) -> String {
    format!("baml_rt_{}", builtin.fn_name)
}

fn camel(text: &str) -> String {
    text.split(['.', '_', '$'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

/// The trait holding a builtin's method: one per class, and one per
/// namespace for its free functions.
fn trait_name(builtin: &NativeBuiltin) -> String {
    let package = builtin.path.split('.').next().unwrap_or_default();
    format!(
        "Native{}{}{}",
        camel(package),
        camel(&builtin.namespace),
        camel(builtin.class_segment.as_deref().unwrap_or_default())
    )
}

fn method_name(builtin: &NativeBuiltin) -> &str {
    builtin.path.rsplit('.').next().unwrap_or(&builtin.path)
}

/// Every builtin with what the native runtime's lists say of it, by path.
fn listed<'a>(
    vm: &'a [NativeBuiltin],
    io: &'a [NativeBuiltin],
    classes: &Classes<'_>,
    implemented: &[NativeListEntry],
) -> Result<BTreeMap<&'a str, (&'a NativeBuiltin, Option<NativeFlags>)>, String> {
    let mut builtins: BTreeMap<&str, (&NativeBuiltin, Option<NativeFlags>)> = vm
        .iter()
        .chain(io)
        .map(|builtin| (builtin.path.as_str(), (builtin, None)))
        .collect();
    for entry in implemented {
        let Some((builtin, flags)) = builtins.get_mut(entry.path.as_str()) else {
            return Err(format!(
                "`{}` is listed as a native but the standard library declares no such builtin",
                entry.path
            ));
        };
        if flags.replace(entry.flags).is_some() {
            return Err(format!("`{}` is listed as a native twice", entry.path));
        }
        if abi_of(builtin, classes).is_none() {
            return Err(format!(
                "`{}` is listed as a native but its signature has a type natives cannot take \
                 yet: {:?} -> {:?}",
                entry.path,
                builtin
                    .params
                    .iter()
                    .map(|param| &param.ty)
                    .collect::<Vec<_>>(),
                builtin.return_type
            ));
        }
        if throws(builtin, Some(entry.flags))
            && abi_of(builtin, classes).is_some_and(|abi| abi.result.is_pair())
        {
            return Err(format!(
                "`{}` is listed as a native but it throws and returns an optional scalar, which \
                 takes three registers",
                entry.path
            ));
        }
        if !entry.flags.allocates && abi_of(builtin, classes).is_some_and(|abi| abi.result.is_ref())
        {
            return Err(format!(
                "`{}` returns an object, so it allocates: list it with `allocates`",
                entry.path
            ));
        }
    }
    Ok(builtins)
}

/// The compiler's table: a `Native` for every Rust-declared builtin, sorted
/// by path.
pub fn generate_native_table(
    vm: &[NativeBuiltin],
    io: &[NativeBuiltin],
    classes: &[NativeClassDef],
    implemented: &[NativeListEntry],
) -> Result<String, String> {
    let classes = &Classes::new(classes);
    let mut out = String::from("pub(crate) static NATIVES: &[Native] = &[\n");
    for (path, (builtin, flags)) in listed(vm, io, classes, implemented)? {
        let abi = abi_of(builtin, classes);
        let named = abi
            .as_ref()
            .map_or_else(String::new, |abi| abi.classes.join(", "));
        let signature = abi.as_ref().map_or_else(
            || "None".to_owned(),
            |abi| {
                let params: Vec<_> = abi.params.iter().map(|(_, ty)| ty.table()).collect();
                format!(
                    "Some(Signature {{ params: &[{}], result: {} }})",
                    params.join(", "),
                    abi.result.table()
                )
            },
        );
        let _ = writeln!(
            out,
            "    Native {{ path: {path:?}, symbol: {:?}, signature: {signature}, throws: {}, \
             flags: {}, type_args: {}, classes: &[{named}] }},",
            symbol(builtin),
            throws(builtin, flags),
            flags.map_or_else(
                || "None".to_owned(),
                |flags| format!(
                    "Some(Flags {{ allocates: {}, is_async: {} }})",
                    flags.allocates, flags.is_async
                )
            ),
            type_args(builtin),
        );
    }
    out.push_str("];\n");
    Ok(out)
}

/// The native runtime's side: the traits its implementations fill in, and
/// the symbols compiled code calls.
///
/// A native that `allocates` is called with a scope whose roots are in the
/// symbol's own frame. One that is `async` is an `async fn`: its symbol
/// makes the future in a block of its own and returns the block, for the
/// stub that called it to poll until the native is done.
pub fn generate_native_shims(
    vm: &[NativeBuiltin],
    io: &[NativeBuiltin],
    classes: &[NativeClassDef],
    implemented: &[NativeListEntry],
) -> Result<String, String> {
    let classes = &Classes::new(classes);
    let mut traits: BTreeMap<String, String> = BTreeMap::new();
    let mut shims = String::new();
    for (path, (builtin, flags)) in listed(vm, io, classes, implemented)? {
        let Some(flags) = flags else { continue };
        let abi = abi_of(builtin, classes).expect("checked when listed");
        let fallible = throws(builtin, Some(flags));
        let type_args = type_args(builtin);
        let trait_name = trait_name(builtin);
        let method = method_name(builtin);
        let allocates = flags.allocates;

        // The trait method.
        let mut parameters: Vec<String> = Vec::new();
        if allocates {
            parameters.push("scope: &mut Scope<'s>".to_owned());
        }
        // The type id of each type argument of the call.
        if type_args != 0 {
            parameters.push(format!("types: [TypeId; {type_args}]"));
        }
        for (name, ty) in &abi.params {
            if *ty == AbiTy::Typed {
                parameters.push(format!("{name}_type: TypeId"));
            }
            parameters.push(format!("r#{name}: {}", ty.parameter(allocates)));
        }
        let value = abi.result.parameter(allocates && abi.result.is_ref());
        let returned = if fallible {
            format!("Result<{value}, Thrown<'s>>")
        } else {
            value
        };
        let lifetime = if allocates { "<'s>" } else { "" };
        let keyword = if flags.is_async { "async fn" } else { "fn" };
        let _ = writeln!(
            traits.entry(trait_name.clone()).or_default(),
            "    /// `{path}`\n    {keyword} r#{method}{lifetime}({}) -> {returned};",
            parameters.join(", "),
        );

        // The symbol.
        let mut c_parameters: Vec<String> = Vec::new();
        if allocates {
            c_parameters.push("mutator: &mut Mutator".to_owned());
        }
        for index in 0..type_args {
            c_parameters.push(format!("type_{index}: u32"));
        }
        for (name, ty) in &abi.params {
            c_parameters.extend(ty.c_parameters(name));
        }
        let mut arguments: Vec<String> = Vec::new();
        if flags.is_async {
            arguments.push("scope".to_owned());
        } else if allocates {
            arguments.push("&mut scope".to_owned());
        }
        if type_args != 0 {
            arguments.push("types".to_owned());
        }
        let mut body = String::new();
        if allocates && !flags.is_async {
            body.push_str(
                "    let slots = RootSlots::new();\n    // SAFETY: `slots` outlives the scope, and \
                 the arguments are live objects of the declared types.\n    let mut scope = \
                 unsafe { Scope::new(mutator, &slots) };\n",
            );
        }
        if type_args != 0 {
            let ids: Vec<_> = (0..type_args)
                .map(|index| format!("TypeId(type_{index})"))
                .collect();
            let _ = writeln!(body, "    let types = [{}];", ids.join(", "));
        }
        // What roots the arguments: in an `async` native, the first lines
        // of the closure that makes the future, which is handed the scope.
        let mut rooting = String::new();
        for (name, ty) in &abi.params {
            if *ty == AbiTy::Typed {
                arguments.push(format!("TypeId({name}_type)"));
            }
            let held = matches!(
                ty,
                AbiTy::Elem(_) | AbiTy::Any | AbiTy::Typed | AbiTy::BoxedElem(_)
            );
            if ty.is_ref() || (allocates && held) {
                let _ = writeln!(
                    rooting,
                    "    // SAFETY: compiled code passes a live object of the declared type.\n    \
                     let r#{name} = {};",
                    ty.argument(&format!("r#{name}"), allocates)
                );
                arguments.push(format!("r#{name}"));
            } else {
                arguments.push(ty.argument(&format!("r#{name}"), allocates));
            }
        }
        let call = format!(
            "<Natives as {trait_name}>::r#{method}({})",
            arguments.join(", ")
        );
        let c_result = if flags.is_async {
            let _ = writeln!(
                body,
                "    let make = move |scope: &'static mut Scope<'static>| {{\n{rooting}    async \
                 move {{\n        match {call}.await {{\n            Ok(value) => \
                 RtResult::ok({}),\n            Err(thrown) => scope.throw_again(&thrown),\n        \
                 }}\n    }}\n    }};\n    // SAFETY: called on a task, by the stub compiled code \
                 calls the native through.\n    unsafe {{ start(mutator, {path:?}, make) }}",
                abi.result.bits("value")
            );
            "*mut Head"
        } else if fallible {
            body.push_str(&rooting);
            let _ = writeln!(
                body,
                "    match {call} {{\n        Ok(value) => RtResult::ok({}),\n        \
                 Err(thrown) => RtResult::thrown(&thrown),\n    }}",
                abi.result.bits("value")
            );
            "RtResult"
        } else {
            body.push_str(&rooting);
            let _ = writeln!(
                body,
                "    let value = {call};\n    {}",
                abi.result.returned("value")
            );
            abi.result.c()
        };
        // A native that can collect is reached through a stub that
        // records where compiled code called it from: the symbol is the
        // stub, and the function is what the stub goes on in.
        let symbol = symbol(builtin);
        if allocates {
            let _ = writeln!(
                shims,
                "anchored! {{\n    /// The symbol of `{path}`.\n    {symbol} => {name}\n}}\n\n/// \
                 `{path}`\n///\n/// # Safety\n///\n/// Called from compiled code through its \
                 symbol, with live objects of the declared types.\nunsafe extern \"C\" fn \
                 {name}({}) -> {c_result} {{\n{body}}}\n",
                c_parameters.join(", "),
                name = builtin.fn_name,
            );
        } else {
            let _ = writeln!(
                shims,
                "/// `{path}`\n///\n/// # Safety\n///\n/// Called from compiled code, with live \
                 objects of the declared types.\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn \
                 {symbol}({}) -> {c_result} {{\n{body}}}\n",
                c_parameters.join(", "),
            );
        }
    }

    let mut out = String::new();
    for (name, methods) in traits {
        let _ = writeln!(out, "pub trait {name} {{\n{methods}}}\n");
    }
    out.push_str(&shims);
    Ok(out)
}
