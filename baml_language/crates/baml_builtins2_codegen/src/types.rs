/// Which pipeline a builtin belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinPipeline {
    /// `$rust_function` — synchronous, runs inline in VM.
    Vm,
    /// `$rust_io_function` — asynchronous, dispatched via engine.
    Io,
}

/// A builtin's memory behavior, declared by `//baml:effects(...)` on a
/// method of a container class. Clauses, comma-separated:
///
/// - `reads`: keeps nothing it is given.
/// - `keeps(param)`: `param`'s value becomes an element or entry of the
///   receiver.
/// - `returns(element)`: returns one of the receiver's elements or entries.
/// - `returns(fresh)` / `returns(fresh+param)`: returns a new container of
///   the receiver's elements (and `param`'s).
/// - `compares(keys)`: hashes or compares the receiver's keys, which runs
///   user code unless they are primitive.
///
/// Anything not named is only read. Escape analysis trusts these, so a
/// declaration must cover everything the Rust implementation does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryEffects {
    pub keeps: Vec<String>,
    pub returns: EffectReturn,
    pub compares_keys: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EffectReturn {
    /// Returns no object of the receiver's.
    #[default]
    Nothing,
    /// Returns an element or entry of the receiver.
    Element,
    /// Returns a new container of the receiver's elements and those of the
    /// named parameters.
    Fresh(Vec<String>),
}

impl MemoryEffects {
    /// Parse the text inside `//baml:effects(...)`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut effects = Self::default();
        let mut clauses = Vec::new();
        let mut depth = 0usize;
        let mut start = 0;
        for (i, c) in text.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    clauses.push(&text[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        clauses.push(&text[start..]);
        for clause in clauses.into_iter().map(str::trim) {
            let (name, arg) = match clause.split_once('(') {
                Some((name, rest)) => (
                    name.trim(),
                    Some(
                        rest.strip_suffix(')')
                            .ok_or_else(|| format!("unclosed `{clause}`"))?
                            .trim(),
                    ),
                ),
                None => (clause, None),
            };
            match (name, arg) {
                ("reads", None) => {}
                ("keeps", Some(param)) => effects.keeps.push(param.to_string()),
                ("returns", Some("element")) => effects.returns = EffectReturn::Element,
                ("returns", Some(fresh)) if fresh.split('+').next() == Some("fresh") => {
                    effects.returns = EffectReturn::Fresh(
                        fresh
                            .split('+')
                            .skip(1)
                            .map(|p| p.trim().to_string())
                            .collect(),
                    );
                }
                ("compares", Some("keys")) => effects.compares_keys = true,
                _ => return Err(format!("unknown effect clause `{clause}`")),
            }
        }
        Ok(effects)
    }

    /// Every parameter the declaration names.
    pub fn named_params(&self) -> impl Iterator<Item = &str> {
        let fresh: &[String] = match &self.returns {
            EffectReturn::Fresh(params) => params,
            EffectReturn::Nothing | EffectReturn::Element => &[],
        };
        self.keeps.iter().chain(fresh).map(String::as_str)
    }
}

/// A single extracted `$rust_function` or `$rust_io_function` builtin.
pub struct NativeBuiltin {
    /// Dotted path: e.g. `"baml.Array.length"`, `"baml.deep_copy"`, `"baml.sys.argv"`
    pub path: String,
    /// The declaring namespace (no package, no class): `"fs"` for
    /// `baml.fs.File.read`, `""` for a top-level class or item.
    /// Carried STRUCTURALLY from extraction — the `path` alone cannot be
    /// split, because an impl-block class segment embeds the interface
    /// target as the native key spells it — a written `root.` as the
    /// package's fixed name — which may itself be dotted
    /// (`baml.io.Read$for$File`).
    pub namespace: String,
    /// The full class segment (`Class` or `{iface}$for${Class}`) for a
    /// method; `None` for a free item. Same structural-carry rationale as
    /// [`Self::namespace`].
    pub class_segment: Option<String>,
    /// Rust function name derived from path (dots → underscores, lowercased):
    /// e.g. `"baml_array_length"`, `"baml_deep_copy"`, `"baml_sys_argv"`
    pub fn_name: String,
    /// Non-self/receiver parameters only.
    pub params: Vec<Param>,
    pub return_type: BamlType,
    /// Generic type parameters declared on the function or class (e.g. `["T"]`).
    pub generics: Vec<String>,
    /// None for free functions; Some for methods with a `self` receiver.
    pub receiver: Option<Receiver>,
    /// How the method uses the VM parameter, determined by `//baml:vm` or `//baml:mut_vm`
    /// directives. Mutually exclusive with `ReceiverType::MutSelf` (enforced at extraction time).
    pub vm_usage: VmUsage,
    /// When true, the trait method returns `NativeCallResult` directly instead of
    /// `Result<T, VmRustFnError>`. Set by `//baml:may_yield` directive.
    pub may_yield: bool,
    /// When true, the builtin is treated as fallible (the trait method returns
    /// `Result<T, VmRustFnError>`) even if it declares `throws never`. Set by the
    /// `//baml:fallible` directive — for builtins that can raise a *panic*
    /// (e.g. `AllocFailure`) rather than a catchable `throws` error.
    pub fallible: bool,
    /// What the builtin does with the objects it receives, from
    /// `//baml:effects(...)`; `None` when undeclared, which callers must
    /// treat as doing anything.
    pub effects: Option<MemoryEffects>,
    /// Which pipeline this builtin belongs to.
    pub pipeline: BuiltinPipeline,
    /// The `throws` clause:
    /// - `None` — no clause declared (a compiler error for builtins).
    /// - `Some([])` — `throws never`: declares the builtin throws nothing.
    /// - `Some([cats])` — error categories, e.g. `["Io", "Timeout"]`; the
    ///   builtin is fallible.
    pub throws: Option<Vec<String>>,
    /// Virtual path of the `.baml` source file (e.g. `"<builtin>/baml/ns_fs/fs.baml"`).
    pub source_file: String,
}

impl NativeBuiltin {
    /// Derive the `SysOp` enum variant name from the path.
    /// `"baml.fs.open"` → `"BamlFsOpen"`, `"baml.fs.File.read"` → `"BamlFsFileRead"`,
    /// `"baml.env.get"` → `"BamlEnvGet"`, `"ai.Prompt.text"` → `"AiPromptText"`.
    /// An impl-block method path (`baml.random.Rng$for$SystemRandom.random`)
    /// treats `$` as a word boundary → `"BamlRandomRngForSystemRandomRandom"`.
    pub fn sys_op_variant_name(&self) -> String {
        self.path
            .split(['.', '$'])
            .flat_map(|segment| {
                segment.split('_').map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    }
                })
            })
            .collect()
    }
}

/// How a native method accesses the VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmUsage {
    /// No `vm` parameter.
    None,
    /// Immutable borrow: `vm: &BexVm` (from `//baml:vm`).
    Ref,
    /// Mutable borrow: `vm: &mut BexVm` (from `//baml:mut_vm`).
    MutRef,
}

/// A single non-receiver parameter.
pub struct Param {
    pub name: String,
    pub ty: BamlType,
    /// A native default resolved by glue without introducing a bytecode frame.
    pub empty_map_default: bool,
}

/// Receiver (the `self` parameter) of a method.
pub struct Receiver {
    /// The class name (e.g. `"Array"`, `"Map"`, `"String"`, `"Pdf"`).
    pub class_name: String,
    /// Dotted namespace of the class, relative to the `baml` package
    /// (e.g. `"time"`, `"media"`, `""` for a root-level class). Used to build
    /// the `view::<ns>::<Class>` path when the receiver is passed as a view.
    pub namespace: String,
    /// Whether the class is backed by `Object::Instance` with declared fields
    /// (i.e. it has a generated `view::` struct). True for classes like
    /// `time.Instant` / media classes; false for dedicated-variant types
    /// (`Array`/`Map`/`String`/`Uint8Array`), opaque classes (`Future`), and
    /// primitives — those keep a type-erased `&Value` receiver.
    pub instance_backed: bool,
    /// Generic type parameters of the class (e.g. `["T"]` for `Array<T>`).
    pub class_generics: Vec<String>,
    pub receiver_type: ReceiverType,
    /// Whether the receiver may be one of the builtins the glue special-cases
    /// by class name (the containers, `String`, the numeric companions, the
    /// media classes). Those all live in the `baml` package, so a same-named
    /// class of another package (`reflect.Array`, `reflect.Map`) is not one.
    pub builtin: bool,
}

impl Receiver {
    /// The class name builtin-receiver handling matches on: the class name
    /// when the receiver may be a builtin ([`Self::builtin`]), else `""`,
    /// which no builtin is called.
    pub fn builtin_name(&self) -> &str {
        if self.builtin { &self.class_name } else { "" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReceiverType {
    /// Static method: no `self` parameters.
    /// The receiver is set for dispatch routing but the generated trait method and
    /// glue should not include a receiver parameter.
    Static,
    /// Instance method: has `&self` parameter.
    RefSelf,
    /// Instance method with `//baml:mut_self` annotation.
    /// Rust binding gets `&mut self` parameter.
    MutSelf,
}
impl ReceiverType {
    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static)
    }
    pub fn is_mut(&self) -> bool {
        matches!(self, Self::MutSelf)
    }
}

/// BAML type extracted from a type expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BamlType {
    String,
    Int,
    Bigint,
    Float,
    Bool,
    Null,
    List(Box<BamlType>),
    Map(Box<BamlType>, Box<BamlType>),
    Optional(Box<BamlType>),
    /// A generic type parameter like `T`, `K`, `V`.
    Generic(String),
    /// `Uint8Array` (binary data) type.
    Uint8Array,
    /// A named media class: `"Pdf"`, `"Audio"`, `"Image"`, `"Video"`.
    Media(String),
    /// Some other named type (class reference, path type).
    Named(String),
    /// A `$rust_type` field — opaque Rust-managed state.
    RustType,
}

/// A single field of a builtin class.
#[derive(Debug, Clone)]
pub struct NativeClassField {
    /// Field name as it appears in the `.baml` source (e.g. `"_data"`, `"message"`).
    pub name: String,
    /// BAML type of the field.
    pub field_type: BamlType,
    /// Positional index in the `Instance.fields` Vec (0-based, matches emission order).
    pub index: usize,
}

/// A builtin class definition extracted from a `.baml` stdlib file.
#[derive(Debug, Clone)]
pub struct NativeClassDef {
    /// Simple class name (e.g. `"Pdf"`, `"InvalidArgument"`).
    pub name: String,
    /// Dotted namespace prefix (e.g. `"baml.media"`, `"baml.errors"`, `"baml"`).
    pub namespace_prefix: String,
    /// Generic type parameters (e.g. `["T"]` for `Array<T>`).
    pub generic_params: Vec<String>,
    /// Fields in declaration order.
    pub fields: Vec<NativeClassField>,
    /// Virtual path of the `.baml` source file (e.g. `"<builtin>/baml/ns_fs/fs.baml"`).
    pub source_file: String,
}

#[cfg(test)]
mod tests {
    /// The variant scheme treats `.` and `$` alike as word boundaries, so an
    /// impl-block path camel-cases through its `$for$` segment — the enum
    /// arms the generated dispatch tables and hand-written glue key on.
    #[test]
    fn sys_op_variant_name_splits_on_dots_and_dollars() {
        let variant = |path: &str| {
            super::NativeBuiltin {
                path: path.to_string(),
                namespace: String::new(),
                class_segment: None,
                fn_name: String::new(),
                params: Vec::new(),
                return_type: super::BamlType::Null,
                generics: Vec::new(),
                receiver: None,
                vm_usage: super::VmUsage::None,
                may_yield: false,
                fallible: false,
                effects: None,
                pipeline: super::BuiltinPipeline::Vm,
                throws: None,
                source_file: String::new(),
            }
            .sys_op_variant_name()
        };
        assert_eq!(variant("baml.fs.open"), "BamlFsOpen");
        assert_eq!(variant("baml.env.get"), "BamlEnvGet");
        assert_eq!(
            variant("baml.random.Rng$for$SystemRandom.random"),
            "BamlRandomRngForSystemRandomRandom"
        );
        assert_eq!(
            variant("baml.fs.baml.io.Read$for$File.read"),
            "BamlFsBamlIoReadForFileRead"
        );
    }
}

#[cfg(test)]
mod memory_effects_tests {
    use super::{EffectReturn, MemoryEffects};

    #[test]
    fn parses_every_clause() {
        assert_eq!(MemoryEffects::parse("reads"), Ok(MemoryEffects::default()));
        assert_eq!(
            MemoryEffects::parse("keeps(key), keeps(value), returns(element), compares(keys)"),
            Ok(MemoryEffects {
                keeps: vec!["key".into(), "value".into()],
                returns: EffectReturn::Element,
                compares_keys: true,
            })
        );
        assert_eq!(
            MemoryEffects::parse("returns(fresh+other)"),
            Ok(MemoryEffects {
                returns: EffectReturn::Fresh(vec!["other".into()]),
                ..MemoryEffects::default()
            })
        );
    }

    #[test]
    fn rejects_unknown_clauses() {
        assert!(MemoryEffects::parse("leaks(item)").is_err());
        assert!(MemoryEffects::parse("returns(anything)").is_err());
        assert!(MemoryEffects::parse("keeps(item").is_err());
    }
}
