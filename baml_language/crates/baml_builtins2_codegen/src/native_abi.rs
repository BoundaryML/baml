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
//! directory, one `<path> <kind>` per line. A builtin not listed there is in
//! the compiler's table all the same, marked as not ported.

use std::{collections::BTreeMap, fmt::Write as _, path::Path};

use crate::types::{BamlType, NativeBuiltin};

/// How a native may be called, which later selects the stub it is reached
/// through.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NativeKind {
    /// Reads its arguments and returns: no allocation, no throw.
    Leaf,
    /// May allocate, and so collect, and may throw.
    Reentrant,
}

impl NativeKind {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "leaf" => Some(Self::Leaf),
            "reentrant" => Some(Self::Reentrant),
            _ => None,
        }
    }

    fn variant(self) -> &'static str {
        match self {
            Self::Leaf => "Leaf",
            Self::Reentrant => "Reentrant",
        }
    }
}

/// One line of the native runtime's lists: a builtin it implements.
#[derive(Clone, Debug)]
pub struct NativeListEntry {
    /// The builtin's dotted path, as its declaration has it.
    pub path: String,
    pub kind: NativeKind,
}

/// Read every `*.txt` file of `dir`: a `<path> <kind>` per line, with blank
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
            let entry = match (words.next(), words.next().and_then(NativeKind::parse)) {
                (Some(path), Some(kind)) if words.next().is_none() => NativeListEntry {
                    path: path.to_owned(),
                    kind,
                },
                _ => {
                    return Err(format!(
                        "{}:{}: expected `<path> leaf` or `<path> reentrant`",
                        file.display(),
                        number + 1
                    ));
                }
            };
            entries.push(entry);
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
    Array(Box<AbiTy>),
    /// A type or `null`: a reference that may be zero, or for an `int`,
    /// `float` or `bool` a pair of words, the value's bits and whether
    /// there is a value.
    Optional(Box<AbiTy>),
    /// The builtin's type parameter at this position, class parameters
    /// first: a value of whatever type a call names, as the one word an
    /// array or a field holds it in. Whether that word is a reference is
    /// known from the call's type id for the parameter.
    Elem(usize),
}

impl AbiTy {
    fn of(ty: &BamlType, generics: &[&str]) -> Option<Self> {
        match ty {
            BamlType::Generic(name) => generics
                .iter()
                .position(|generic| generic == name)
                .map(Self::Elem),
            BamlType::Int => Some(Self::Int),
            BamlType::Float => Some(Self::Float),
            BamlType::Bool => Some(Self::Bool),
            BamlType::Null => Some(Self::Null),
            BamlType::String => Some(Self::String),
            BamlType::List(element) => match Self::of(element, generics)? {
                Self::Null | Self::Optional(_) => None,
                element => Some(Self::Array(Box::new(element))),
            },
            BamlType::Optional(inner) => match Self::of(inner, generics)? {
                Self::Null | Self::Optional(_) => None,
                inner => Some(Self::Optional(Box::new(inner))),
            },
            _ => None,
        }
    }

    fn is_ref(&self) -> bool {
        match self {
            Self::String | Self::Array(_) => true,
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
            Self::Array(element) => format!("Ty::Array(&{})", element.table()),
            Self::Optional(inner) => format!("Ty::Optional(&{})", inner.table()),
            Self::Elem(index) => format!("Ty::Elem({index})"),
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
            Self::String | Self::Array(_) | Self::Optional(_) | Self::Elem(_) => "usize",
        }
    }

    /// The symbol's parameters for a parameter `name` of this type: one,
    /// or for an optional scalar its bits and then whether it is present.
    fn c_parameters(&self, name: &str) -> Vec<String> {
        if self.is_pair() {
            vec![format!("r#{name}: u64"), format!("{name}_present: u64")]
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
            Self::Array(element) => format!("Arr<{}>", element.marker()),
            Self::Optional(inner) => inner.marker(),
            Self::Elem(_) => "Slot".to_owned(),
        }
    }

    /// The type as a trait method takes it.
    fn parameter(&self, kind: NativeKind) -> String {
        match (self, kind) {
            (Self::Optional(inner), _) if matches!(**inner, Self::Elem(_)) => {
                "Option<usize>".to_owned()
            }
            (Self::Optional(inner), _) => format!("Option<{}>", inner.parameter(kind)),
            (Self::Elem(_), NativeKind::Leaf) => "usize".to_owned(),
            (Self::Elem(_), NativeKind::Reentrant) => "Elem<'s>".to_owned(),
            (Self::Int, _) => "i64".to_owned(),
            (Self::Float, _) => "f64".to_owned(),
            (Self::Bool, _) => "bool".to_owned(),
            (Self::Null, _) => "()".to_owned(),
            (Self::String, NativeKind::Leaf) => "StrRef<'_>".to_owned(),
            (Self::Array(element), NativeKind::Leaf) => {
                format!("ArrRef<'_, {}>", element.marker())
            }
            (_, NativeKind::Reentrant) => format!("Root<'s, {}>", self.marker()),
        }
    }

    /// The expression making a trait method's argument out of the symbol's
    /// parameter `name`.
    fn argument(&self, name: &str, kind: NativeKind) -> String {
        match (self, kind) {
            (Self::Int | Self::Float, _) | (Self::Elem(_), NativeKind::Leaf) => name.to_owned(),
            (Self::Elem(index), NativeKind::Reentrant) => {
                format!("unsafe {{ scope.elem_raw({name}, types[{index}]) }}")
            }
            (Self::Bool, _) => format!("{name} != 0"),
            (Self::Null, _) => "()".to_owned(),
            (Self::Optional(inner), _) if inner.is_ref() => {
                format!("({name} != 0).then(|| {})", inner.argument(name, kind))
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
            (_, NativeKind::Leaf) => format!("unsafe {{ View::from_raw({name}) }}"),
            (_, NativeKind::Reentrant) => format!("unsafe {{ scope.root_raw({name}) }}"),
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
            Self::String | Self::Array(_) => format!("{value}.raw() as u64"),
            Self::Elem(_) => format!("{value} as u64"),
            // An optional scalar has no fallible form: see `listed`.
            Self::Optional(_) => format!("{value}.map_or(0, |value| value.raw() as u64)"),
        }
    }

    /// The expression turning a trait method's result `value` into what an
    /// infallible symbol returns.
    fn returned(&self, value: &str) -> String {
        match self {
            Self::Int | Self::Float | Self::Null | Self::Elem(_) => value.to_owned(),
            Self::Bool => format!("u8::from({value})"),
            Self::String | Self::Array(_) => format!("{value}.raw()"),
            Self::Optional(inner) if inner.is_ref() => {
                format!("{value}.map_or(0, |value| value.raw())")
            }
            Self::Optional(_) => format!("OptionalScalar::from({value})"),
        }
    }
}

/// The ABI of a builtin, when every type in its signature has one.
struct Abi {
    params: Vec<(String, AbiTy)>,
    result: AbiTy,
}

/// The builtin's type parameters, in the order a call passes their types:
/// its class's, then its own. The record lists both together.
fn generics(builtin: &NativeBuiltin) -> Vec<&str> {
    builtin.generics.iter().map(String::as_str).collect()
}

fn abi_of(builtin: &NativeBuiltin) -> Option<Abi> {
    let generics = generics(builtin);
    let mut params = Vec::new();
    if let Some(receiver) = &builtin.receiver {
        if !receiver.receiver_type.is_static() {
            let ty = match receiver.builtin_name() {
                "String" => AbiTy::String,
                "Array" => AbiTy::Array(Box::new(AbiTy::Elem(0))),
                "Float" => AbiTy::Float,
                "Int" => AbiTy::Int,
                "Bool" => AbiTy::Bool,
                _ => return None,
            };
            params.push((receiver.class_name.to_lowercase(), ty));
        }
    }
    for param in &builtin.params {
        match AbiTy::of(&param.ty, &generics)? {
            AbiTy::Null => return None,
            // The two words of an optional value of a type parameter are
            // only ever a result.
            AbiTy::Optional(inner) if matches!(*inner, AbiTy::Elem(_)) => return None,
            ty => params.push((param.name.clone(), ty)),
        }
    }
    Some(Abi {
        params,
        result: AbiTy::of(&builtin.return_type, &generics)?,
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
/// leaf throws nothing, whatever the declaration allows: listing a builtin
/// as one says its native implementation cannot fail.
fn throws(builtin: &NativeBuiltin, kind: Option<NativeKind>) -> bool {
    kind != Some(NativeKind::Leaf) && is_fallible(builtin)
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
    implemented: &[NativeListEntry],
) -> Result<BTreeMap<&'a str, (&'a NativeBuiltin, Option<NativeKind>)>, String> {
    let mut builtins: BTreeMap<&str, (&NativeBuiltin, Option<NativeKind>)> = vm
        .iter()
        .chain(io)
        .map(|builtin| (builtin.path.as_str(), (builtin, None)))
        .collect();
    for entry in implemented {
        let Some((builtin, kind)) = builtins.get_mut(entry.path.as_str()) else {
            return Err(format!(
                "`{}` is listed as a native but the standard library declares no such builtin",
                entry.path
            ));
        };
        if kind.replace(entry.kind).is_some() {
            return Err(format!("`{}` is listed as a native twice", entry.path));
        }
        if abi_of(builtin).is_none() {
            return Err(format!(
                "`{}` is listed as a native but its signature has a type natives cannot take yet",
                entry.path
            ));
        }
        if throws(builtin, Some(entry.kind))
            && abi_of(builtin).is_some_and(|abi| abi.result.is_pair())
        {
            return Err(format!(
                "`{}` is listed as a native but it throws and returns an optional scalar, which \
                 takes three registers",
                entry.path
            ));
        }
        if entry.kind == NativeKind::Leaf && abi_of(builtin).is_some_and(|abi| abi.result.is_ref())
        {
            return Err(format!(
                "`{}` is listed as a leaf, but it returns an object: list it as reentrant",
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
    implemented: &[NativeListEntry],
) -> Result<String, String> {
    let mut out = String::from("pub(crate) static NATIVES: &[Native] = &[\n");
    for (path, (builtin, kind)) in listed(vm, io, implemented)? {
        let abi = abi_of(builtin);
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
             kind: {}, type_args: {} }},",
            symbol(builtin),
            throws(builtin, kind),
            kind.map_or_else(
                || "None".to_owned(),
                |kind| format!("Some(Kind::{})", kind.variant())
            ),
            type_args(builtin),
        );
    }
    out.push_str("];\n");
    Ok(out)
}

/// The native runtime's side: the traits its implementations fill in, and
/// the symbols compiled code calls.
pub fn generate_native_shims(
    vm: &[NativeBuiltin],
    io: &[NativeBuiltin],
    implemented: &[NativeListEntry],
) -> Result<String, String> {
    let mut traits: BTreeMap<String, String> = BTreeMap::new();
    let mut shims = String::new();
    for (path, (builtin, kind)) in listed(vm, io, implemented)? {
        let Some(kind) = kind else { continue };
        let abi = abi_of(builtin).expect("checked when listed");
        let fallible = throws(builtin, Some(kind));
        let type_args = type_args(builtin);
        let trait_name = trait_name(builtin);
        let method = method_name(builtin);
        let reentrant = kind == NativeKind::Reentrant;

        // The trait method.
        let mut parameters: Vec<String> = Vec::new();
        if reentrant {
            parameters.push("scope: &mut Scope<'s>".to_owned());
        }
        // The type id of each type argument of the call.
        if type_args != 0 {
            parameters.push(format!("types: [TypeId; {type_args}]"));
        }
        for (name, ty) in &abi.params {
            parameters.push(format!("r#{name}: {}", ty.parameter(kind)));
        }
        let value = match (&abi.result, reentrant) {
            (ty, true) if ty.is_ref() => ty.parameter(NativeKind::Reentrant),
            (ty, _) => ty.parameter(NativeKind::Leaf),
        };
        let returned = if fallible {
            format!("Result<{value}, Thrown<'s>>")
        } else {
            value
        };
        let lifetime = if reentrant { "<'s>" } else { "" };
        let _ = writeln!(
            traits.entry(trait_name.clone()).or_default(),
            "    /// `{path}`\n    fn r#{method}{lifetime}({}) -> {returned};",
            parameters.join(", "),
        );

        // The symbol.
        let mut c_parameters: Vec<String> = Vec::new();
        if reentrant {
            c_parameters.push("mutator: &mut Mutator".to_owned());
        }
        for index in 0..type_args {
            c_parameters.push(format!("type_{index}: u32"));
        }
        for (name, ty) in &abi.params {
            c_parameters.extend(ty.c_parameters(name));
        }
        let mut arguments: Vec<String> = Vec::new();
        if reentrant {
            arguments.push("&mut scope".to_owned());
        }
        if type_args != 0 {
            arguments.push("types".to_owned());
        }
        let mut body = String::new();
        if reentrant {
            body.push_str(
                "    mutator.enter(current_frame());\n    let slots = RootSlots::new();\n    \
                 // SAFETY: `slots` outlives the scope, and the arguments are live objects of \
                 the declared types.\n    let mut scope = unsafe { Scope::new(mutator, &slots) \
                 };\n",
            );
        }
        if type_args != 0 {
            let ids: Vec<_> = (0..type_args)
                .map(|index| format!("TypeId(type_{index})"))
                .collect();
            let _ = writeln!(body, "    let types = [{}];", ids.join(", "));
        }
        for (name, ty) in &abi.params {
            if ty.is_ref() || (reentrant && matches!(ty, AbiTy::Elem(_))) {
                let _ = writeln!(
                    body,
                    "    // SAFETY: compiled code passes a live object of the declared type.\n    \
                     let r#{name} = {};",
                    ty.argument(&format!("r#{name}"), kind)
                );
                arguments.push(format!("r#{name}"));
            } else {
                arguments.push(ty.argument(&format!("r#{name}"), kind));
            }
        }
        let call = format!(
            "<Natives as {trait_name}>::r#{method}({})",
            arguments.join(", ")
        );
        let c_result = if fallible {
            let _ = writeln!(
                body,
                "    match {call} {{\n        Ok(value) => RtResult::ok({}),\n        \
                 Err(thrown) => RtResult::thrown(&thrown),\n    }}",
                abi.result.bits("value")
            );
            "RtResult"
        } else {
            let _ = writeln!(
                body,
                "    let value = {call};\n    {}",
                abi.result.returned("value")
            );
            abi.result.c()
        };
        let _ = writeln!(
            shims,
            "/// `{path}`\n///\n/// # Safety\n///\n/// Called from compiled code, with live \
             objects of the declared types.\n#[unsafe(no_mangle)]\npub unsafe extern \"C\" fn \
             {}({}) -> {c_result} {{\n{body}}}\n",
            symbol(builtin),
            c_parameters.join(", "),
        );
    }

    let mut out = String::new();
    for (name, methods) in traits {
        let _ = writeln!(out, "pub trait {name} {{\n{methods}}}\n");
    }
    out.push_str(&shims);
    Ok(out)
}
