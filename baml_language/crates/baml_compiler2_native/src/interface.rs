//! What a library exports, as plain data: its functions, and every class
//! and enum their signatures reach, numbered, with the hash a host checks
//! (see `baml_native_rt/design/bridges.md`).
//!
//! One function, [`library_interface`], makes it. The backend emits the
//! table of exports from it (`codegen/host.rs`), and `native_pack --lib
//! --describe` prints it as JSON for bindings, so the numbers and the hash
//! are made in one place.
//!
//! A type the boundary cannot carry is refused here, when the library is
//! packed, naming the function and the type: never when it runs.

use std::{collections::HashMap, fmt::Write as _};

use baml_compiler2_hir::loc::FunctionLoc;
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    layout::{
        class_fields, class_head, class_ref_of, enum_head, enum_ref_of, enum_variants,
        interface_ref_of,
    },
};
use baml_compiler2_mir::{RuntimeTy, resolved_aliases_for_package};
use baml_type::Literal;

use crate::{
    CompileError,
    program::{Entry, FnKey},
};

/// Every function a library exports and every class and enum they reach.
#[derive(Clone, Debug)]
pub struct LibraryInterface {
    /// One hash of all of it: generated bindings compare theirs with it.
    pub hash: u64,
    /// In export order: a function's number is its place here.
    pub functions: Vec<ExportedFunction>,
    /// The classes and enums, numbered by their place here: the order the
    /// signatures reach them in, depth first.
    pub types: Vec<ExportedType>,
}

#[derive(Clone, Debug)]
pub struct ExportedFunction {
    /// Its name with its namespace, without the package:
    /// `complex_models.round_trip_complex_profile`.
    pub name: String,
    pub params: Vec<ExportedParam>,
    pub result: String,
    /// What it declares or is inferred to throw; `None` when nothing.
    pub throws: Option<String>,
    /// Whether what it throws can cross as a typed value. When it cannot
    /// (a class of the standard library, say), its errors arrive described.
    pub throws_typed: bool,
    /// The signature as one line of text, which [`Self::hash`] is of.
    pub signature: String,
    pub hash: u64,
    /// The result as a tree.
    pub(crate) result_tree: HostType,
    /// The throws type as a tree, when it can cross.
    pub(crate) throws_tree: Option<HostType>,
}

#[derive(Clone, Debug)]
pub struct ExportedParam {
    pub name: String,
    pub ty: String,
    /// The host may pass `OMITTED` for it.
    pub has_default: bool,
    /// Its type as a tree.
    pub(crate) tree: HostType,
}

#[derive(Clone, Debug)]
pub struct ExportedType {
    /// Its name with its namespace, without the package.
    pub name: String,
    pub kind: TypeKind,
    /// A class's field types as trees, in slot order; empty for an enum.
    pub(crate) field_trees: Vec<HostType>,
}

/// A type as a host reads it: what the interface spells as text, as a
/// tree, each class and enum by its number. `to_json` writes it beside the
/// text, so hosts need not parse the text. It is not hashed: it says
/// nothing the text does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostType {
    Int,
    Float,
    Bool,
    Null,
    String,
    LiteralInt(i64),
    LiteralBool(bool),
    LiteralString(String),
    Class(u32),
    Enum(u32),
    EnumVariant { of: u32, index: u32 },
    List(Box<HostType>),
    Map(Box<HostType>),
    Union(Vec<HostType>),
}

impl HostType {
    /// As JSON: a scalar by its name (`"int"`), a literal as
    /// `{"literal": 42}`, and `{"class": 3}`, `{"enum": 1}`,
    /// `{"enum": 1, "variant": 0}`, `{"list": T}`, `{"map": T}` (the keys
    /// are strings), `{"union": [T, ...]}`.
    fn write_json(&self, out: &mut String) {
        match self {
            Self::Int => out.push_str("\"int\""),
            Self::Float => out.push_str("\"float\""),
            Self::Bool => out.push_str("\"bool\""),
            Self::Null => out.push_str("\"null\""),
            Self::String => out.push_str("\"string\""),
            Self::LiteralInt(value) => {
                let _ = write!(out, "{{\"literal\": {value}}}");
            }
            Self::LiteralBool(value) => {
                let _ = write!(out, "{{\"literal\": {value}}}");
            }
            Self::LiteralString(value) => {
                out.push_str("{\"literal\": ");
                json_string(out, value);
                out.push('}');
            }
            Self::Class(number) => {
                let _ = write!(out, "{{\"class\": {number}}}");
            }
            Self::Enum(number) => {
                let _ = write!(out, "{{\"enum\": {number}}}");
            }
            Self::EnumVariant { of, index } => {
                let _ = write!(out, "{{\"enum\": {of}, \"variant\": {index}}}");
            }
            Self::List(element) => {
                out.push_str("{\"list\": ");
                element.write_json(out);
                out.push('}');
            }
            Self::Map(value) => {
                out.push_str("{\"map\": ");
                value.write_json(out);
                out.push('}');
            }
            Self::Union(members) => {
                out.push_str("{\"union\": [");
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    member.write_json(out);
                }
                out.push_str("]}");
            }
        }
    }
}

/// `text` as a JSON string.
fn json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[derive(Clone, Debug)]
pub enum TypeKind {
    /// A class: its fields in slot order, each a name and a type.
    Class(Vec<(String, String)>),
    /// An enum: its variants in order.
    Enum(Vec<String>),
}

/// A class or an enum the interface reaches, with the type that names it,
/// for the backend.
#[derive(Clone)]
pub(crate) enum Reached<'db> {
    Class(ClassRef<'db>, RuntimeTy),
    Enum(EnumRef<'db>, RuntimeTy),
}

/// What the backend needs beside the plain data: for each function its
/// parameter, result and throws types, and each type reached.
pub(crate) struct Detail<'db> {
    pub(crate) functions: Vec<FunctionDetail>,
    pub(crate) types: Vec<Reached<'db>>,
}

pub(crate) struct FunctionDetail {
    pub(crate) params: Vec<RuntimeTy>,
    pub(crate) result: RuntimeTy,
    pub(crate) throws: Option<RuntimeTy>,
}

/// FNV-1a, 64 bits: a hash any host can compute with no dependency. It
/// tells a stale binding from a fresh one; it is not a defense.
pub(crate) fn fnv(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
pub(crate) const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// A name with its namespace and without its package.
fn qualified<'a>(namespace: impl IntoIterator<Item = &'a str>, name: &str) -> String {
    let mut text = String::new();
    for part in namespace {
        text.push_str(part);
        text.push('.');
    }
    text.push_str(name);
    text
}

fn class_name(db: &dyn baml_compiler2_mir::Db, class: ClassRef<'_>) -> String {
    let head = class_head(db, class);
    qualified(
        head.namespace().iter().map(baml_base::Name::as_str),
        head.name().as_str(),
    )
}

fn enum_name(db: &dyn baml_compiler2_mir::Db, enum_ref: EnumRef<'_>) -> String {
    let head = enum_head(db, enum_ref);
    qualified(
        head.namespace().iter().map(baml_base::Name::as_str),
        head.name().as_str(),
    )
}

/// The name an exported function is found by.
pub fn export_name(db: &dyn baml_compiler2_mir::Db, function: FunctionLoc<'_>) -> String {
    let key = FnKey::new(function, Entry::Direct);
    let name = match key.mir(db) {
        Ok(mir) => mir.identity.display_name(db),
        Err(_) => baml_compiler2_hir::item_data::function_data(db, function)
            .name
            .to_string(),
    };
    // The program's own package is `user`.
    name.strip_prefix("user.").unwrap_or(&name).to_owned()
}

/// A type as the interface spells it: every class and enum by its name
/// with its namespace.
fn type_text(db: &dyn baml_compiler2_mir::Db, ty: &RuntimeTy) -> String {
    let nested = |member: &RuntimeTy| {
        let text = type_text(db, member);
        if matches!(member, RuntimeTy::Union(_)) {
            format!("({text})")
        } else {
            text
        }
    };
    match ty {
        RuntimeTy::Int => "int".to_owned(),
        RuntimeTy::Float => "float".to_owned(),
        RuntimeTy::Bool => "bool".to_owned(),
        RuntimeTy::Null => "null".to_owned(),
        RuntimeTy::String => "string".to_owned(),
        RuntimeTy::Literal(Literal::Int(value), _) => value.to_string(),
        RuntimeTy::Literal(Literal::Bool(value), _) => value.to_string(),
        RuntimeTy::Literal(Literal::String(value), _) => format!("{value:?}"),
        RuntimeTy::Enum(head) => {
            enum_ref_of(db, head).map_or_else(|| "?".to_owned(), |e| enum_name(db, e))
        }
        RuntimeTy::EnumVariant(head, variant) => format!(
            "{}.{variant}",
            enum_ref_of(db, head).map_or_else(|| "?".to_owned(), |e| enum_name(db, e))
        ),
        RuntimeTy::Class(head, _) => {
            class_ref_of(db, head).map_or_else(|| "?".to_owned(), |class| class_name(db, class))
        }
        RuntimeTy::List(element) => format!("{}[]", nested(element)),
        RuntimeTy::Map { key, value, .. } => {
            format!("map<{}, {}>", type_text(db, key), type_text(db, value))
        }
        RuntimeTy::Union(members) => members
            .iter()
            .map(|member| type_text(db, member))
            .collect::<Vec<_>>()
            .join(" | "),
        RuntimeTy::Bigint => "bigint".to_owned(),
        RuntimeTy::Literal(Literal::Bigint(value), _) => format!("{value}n"),
        RuntimeTy::Uint8Array => "uint8array".to_owned(),
        RuntimeTy::Unknown => "unknown".to_owned(),
        RuntimeTy::Tuple(elements) => format!(
            "({})",
            elements
                .iter()
                .map(|element| type_text(db, element))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RuntimeTy::TypeAlias(head) => head.name().to_string(),
        RuntimeTy::TypeVar(param) => param.name().to_string(),
        RuntimeTy::Function { .. } => "a function".to_owned(),
        other => format!("{other:?}"),
    }
}

#[derive(Clone)]
struct Walk<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    types: Vec<ExportedType>,
    reached: Vec<Reached<'db>>,
    classes: HashMap<ClassRef<'db>, usize>,
    enums: HashMap<EnumRef<'db>, usize>,
}

impl Walk<'_> {
    /// Check that a value of `ty` can cross, and number every class and
    /// enum it reaches. The error says which type cannot.
    fn reach(&mut self, ty: &RuntimeTy) -> Result<(), String> {
        let db = self.db;
        let refuse = |what: &str| Err(format!("{what} `{}`", type_text(db, ty)));
        match ty {
            RuntimeTy::Int
            | RuntimeTy::Float
            | RuntimeTy::Bool
            | RuntimeTy::Null
            | RuntimeTy::String
            | RuntimeTy::Literal(Literal::Int(_) | Literal::Bool(_) | Literal::String(_), _) => {
                Ok(())
            }
            RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => {
                let Some(enum_ref) = enum_ref_of(db, head) else {
                    return refuse("an enum with no declaration");
                };
                if !self.enums.contains_key(&enum_ref) {
                    let name = enum_name(db, enum_ref);
                    let variants = enum_variants(db, enum_ref)
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    self.enums.insert(enum_ref, self.types.len());
                    self.types.push(ExportedType {
                        name,
                        kind: TypeKind::Enum(variants),
                        field_trees: Vec::new(),
                    });
                    self.reached.push(Reached::Enum(
                        enum_ref,
                        RuntimeTy::Enum(head_of(ty).clone()),
                    ));
                }
                Ok(())
            }
            RuntimeTy::Class(head, arguments) => {
                if interface_ref_of(db, head).is_some() {
                    return refuse("a value of an interface type");
                }
                if !arguments.is_empty() {
                    return refuse("a generic class");
                }
                let Some(class) = class_ref_of(db, head) else {
                    return refuse("a class with no declaration");
                };
                let lang = baml_compiler2_hir::package::lang_roots(db);
                if lang.is(baml_base::LangPackage::Baml, class_head(db, class).root()) {
                    return refuse("a class of the standard library");
                }
                if self.classes.contains_key(&class) {
                    return Ok(());
                }
                let head_info = class_head(db, class);
                let name = class_name(db, class);
                let place = self.types.len();
                self.classes.insert(class, place);
                self.types.push(ExportedType {
                    name,
                    kind: TypeKind::Class(Vec::new()),
                    field_trees: Vec::new(),
                });
                self.reached.push(Reached::Class(class, ty.clone()));
                let aliases = resolved_aliases_for_package(db, head_info.root());
                let mut fields = Vec::new();
                let mut trees = Vec::new();
                for (field, field_ty) in class_fields(db, class) {
                    let field_ty = aliases.0.convert(&field_ty);
                    self.reach(&field_ty)
                        .map_err(|why| format!("{why} (field `{field}`)"))?;
                    trees.push(self.tree(&field_ty));
                    fields.push((field.to_string(), type_text(db, &field_ty)));
                }
                self.types[place].kind = TypeKind::Class(fields);
                self.types[place].field_trees = trees;
                Ok(())
            }
            RuntimeTy::List(element) => self.reach(element),
            RuntimeTy::Map { key, value, .. } => {
                if !matches!(**key, RuntimeTy::String) {
                    return refuse("a map whose keys are not strings");
                }
                self.reach(value)
            }
            RuntimeTy::Union(members) => members.iter().try_for_each(|member| self.reach(member)),
            RuntimeTy::Bigint | RuntimeTy::Literal(Literal::Bigint(_), _) => refuse("a bigint"),
            RuntimeTy::Uint8Array => refuse("a uint8array"),
            RuntimeTy::Tuple(_) => refuse("a tuple"),
            RuntimeTy::Function { .. } => refuse("a function"),
            RuntimeTy::TypeVar(_) => refuse("a type parameter"),
            RuntimeTy::Unknown => refuse("an unknown"),
            RuntimeTy::TypeAlias(_) => refuse("a recursive alias or json"),
            _ => refuse("a type the boundary cannot carry"),
        }
    }
}

impl Walk<'_> {
    /// `ty` as a tree. Called after [`Self::reach`] accepted it, so every
    /// class and enum in it has its number.
    fn tree(&self, ty: &RuntimeTy) -> HostType {
        let db = self.db;
        let number = |place: usize| u32::try_from(place).expect("fewer than 2^32 types");
        match ty {
            RuntimeTy::Int => HostType::Int,
            RuntimeTy::Float => HostType::Float,
            RuntimeTy::Bool => HostType::Bool,
            RuntimeTy::String => HostType::String,
            RuntimeTy::Literal(Literal::Int(value), _) => HostType::LiteralInt(*value),
            RuntimeTy::Literal(Literal::Bool(value), _) => HostType::LiteralBool(*value),
            RuntimeTy::Literal(Literal::String(value), _) => HostType::LiteralString(value.clone()),
            RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => {
                let enum_ref = enum_ref_of(db, head).expect("reached, so declared");
                let of = number(self.enums[&enum_ref]);
                match ty {
                    RuntimeTy::EnumVariant(_, variant) => {
                        let index = enum_variants(db, enum_ref)
                            .iter()
                            .position(|candidate| candidate == variant)
                            .expect("a variant of its enum");
                        HostType::EnumVariant {
                            of,
                            index: number(index),
                        }
                    }
                    _ => HostType::Enum(of),
                }
            }
            RuntimeTy::Class(head, _) => {
                let class = class_ref_of(db, head).expect("reached, so declared");
                HostType::Class(number(self.classes[&class]))
            }
            RuntimeTy::List(element) => HostType::List(Box::new(self.tree(element))),
            RuntimeTy::Map { value, .. } => HostType::Map(Box::new(self.tree(value))),
            RuntimeTy::Union(members) => {
                HostType::Union(members.iter().map(|member| self.tree(member)).collect())
            }
            // `Null`, and nothing else once `reach` accepted the type.
            _ => HostType::Null,
        }
    }
}

/// Whether the runtime can be told what a value of `ty` is: the library
/// checks what a host passes against that description. A class with
/// `@json.flatten` or `@json.fallback` fields, or with a `to_json` or
/// `from_json` of its own, has none a library can use yet.
fn describable(db: &dyn baml_compiler2_mir::Db, ty: &RuntimeTy) -> Result<(), String> {
    crate::json::JsonTypes::new()
        .describe(db, ty)
        .map(drop)
        .map_err(|blocker| {
            format!(
                "a type the library cannot describe `{}`: {blocker}",
                type_text(db, ty)
            )
        })
}

fn head_of(ty: &RuntimeTy) -> &baml_type::DeclName {
    match ty {
        RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) | RuntimeTy::Class(head, _) => head,
        _ => unreachable!("a class or an enum"),
    }
}

/// Why `function` cannot be exported, if it cannot: what the native
/// compiler does not compile, or a type the boundary cannot carry.
pub fn export_refusal(
    db: &dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'_>,
) -> Option<String> {
    let coverage = crate::coverage(db, function);
    if let Some(blocked) = coverage.blocked.first() {
        return Some(format!(
            "does not compile natively: {}: {}",
            blocked.function, blocked.blockers[0]
        ));
    }
    if !baml_compiler2_hir_ty::callable::callable_signature(
        db,
        baml_compiler2_hir::loc::DeclRef::Source(function),
    )
    .generic_params
    .is_empty()
    {
        return Some("it has type parameters".to_owned());
    }
    let mut walk = Walk {
        db,
        types: Vec::new(),
        reached: Vec::new(),
        classes: HashMap::new(),
        enums: HashMap::new(),
    };
    signature(db, function)
        .and_then(|detail| {
            detail
                .params
                .iter()
                .chain([&detail.result])
                .try_for_each(|ty| walk.reach(ty).and_then(|()| describable(db, ty)))
        })
        .err()
}

/// The parameter, result and throws types of `function`, as MIR has them.
fn signature(
    db: &dyn baml_compiler2_mir::Db,
    function: FunctionLoc<'_>,
) -> Result<FunctionDetail, String> {
    let key = FnKey::new(function, Entry::Direct);
    let mir = key.mir(db).map_err(|blocker| blocker.to_string())?;
    let body = crate::check::body_of(mir).map_err(|blocker| blocker.to_string())?;
    let local = |index: usize| body.locals[index].ty.clone();
    // What it declares, or what its body is inferred to throw.
    let throws = baml_compiler2_hir_ty::callable::callable_throws_of(
        db,
        baml_compiler2_hir::loc::DeclRef::Source(function),
    );
    let throws = (!matches!(throws, baml_type::Ty::Never)).then(|| {
        let root = baml_compiler2_hir::file_package::file_package(db, function.file(db)).root;
        resolved_aliases_for_package(db, root).0.convert(throws)
    });
    Ok(FunctionDetail {
        params: (1..=mir.arity).map(local).collect(),
        result: local(0),
        throws,
    })
}

/// The interface of a library exporting `exports`: refused when one of
/// them cannot be exported.
pub fn library_interface(
    db: &dyn baml_compiler2_mir::Db,
    exports: &[FunctionLoc<'_>],
) -> Result<LibraryInterface, CompileError> {
    library_detail(db, exports).map(|(interface, _)| interface)
}

pub(crate) fn library_detail<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    exports: &[FunctionLoc<'db>],
) -> Result<(LibraryInterface, Detail<'db>), CompileError> {
    let mut walk = Walk {
        db,
        types: Vec::new(),
        reached: Vec::new(),
        classes: HashMap::new(),
        enums: HashMap::new(),
    };
    let mut functions = Vec::new();
    let mut details = Vec::new();
    for function in exports {
        let name = export_name(db, *function);
        let refuse = |why: String| CompileError::NotExportable(name.clone(), why);
        let mut detail = signature(db, *function).map_err(refuse)?;
        if !baml_compiler2_hir_ty::callable::callable_signature(
            db,
            baml_compiler2_hir::loc::DeclRef::Source(*function),
        )
        .generic_params
        .is_empty()
        {
            return Err(refuse("it has type parameters".to_owned()));
        }
        for ty in detail.params.iter().chain([&detail.result]) {
            walk.reach(ty)
                .and_then(|()| describable(db, ty))
                .map_err(refuse)?;
        }
        // A thrown value is only read, never built: a throws type that
        // cannot cross does not keep the function from being exported. Its
        // errors arrive described, without a type.
        let throws_text = detail.throws.as_ref().map(|ty| type_text(db, ty));
        if let Some(ty) = &detail.throws {
            let mut trial = walk.clone();
            if trial.reach(ty).is_ok() && describable(db, ty).is_ok() {
                walk = trial;
            } else {
                detail.throws = None;
            }
        }
        let declared = baml_compiler2_hir_ty::callable::callable_signature(
            db,
            baml_compiler2_hir::loc::DeclRef::Source(*function),
        );
        let names: Vec<String> = declared
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                param
                    .name
                    .as_ref()
                    .map_or_else(|| format!("_{}", index + 1), ToString::to_string)
            })
            .collect();
        let defaults: Vec<bool> = declared
            .params
            .iter()
            .map(|param| param.mode == baml_type::FunctionParamMode::Optional)
            .collect();
        let params: Vec<ExportedParam> = detail
            .params
            .iter()
            .zip(names)
            .zip(defaults)
            .map(|((ty, name), has_default)| ExportedParam {
                name,
                ty: type_text(db, ty),
                has_default,
                tree: walk.tree(ty),
            })
            .collect();
        let result = type_text(db, &detail.result);
        let throws = throws_text;
        let mut signature = format!("{name}(");
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                signature.push_str(", ");
            }
            let _ = write!(signature, "{}: {}", param.name, param.ty);
            if param.has_default {
                signature.push_str(" = default");
            }
        }
        let _ = write!(signature, ") -> {result}");
        if let Some(throws) = &throws {
            let _ = write!(signature, " throws {throws}");
        }
        let hash = fnv(signature.as_bytes(), FNV_START);
        functions.push(ExportedFunction {
            name,
            params,
            result,
            throws_typed: detail.throws.is_some(),
            throws,
            signature,
            hash,
            result_tree: walk.tree(&detail.result),
            throws_tree: detail.throws.as_ref().map(|ty| walk.tree(ty)),
        });
        details.push(detail);
    }
    let mut hash = FNV_START;
    for function in &functions {
        hash = fnv(&[0], fnv(function.signature.as_bytes(), hash));
    }
    for ty in &walk.types {
        hash = fnv(ty.name.as_bytes(), fnv(&[1], hash));
        match &ty.kind {
            TypeKind::Class(fields) => {
                for (name, field_ty) in fields {
                    hash = fnv(
                        field_ty.as_bytes(),
                        fnv(&[2], fnv(name.as_bytes(), fnv(&[2], hash))),
                    );
                }
            }
            TypeKind::Enum(variants) => {
                for variant in variants {
                    hash = fnv(variant.as_bytes(), fnv(&[3], hash));
                }
            }
        }
    }
    Ok((
        LibraryInterface {
            hash,
            functions,
            types: walk.types,
        },
        Detail {
            functions: details,
            types: walk.reached,
        },
    ))
}

impl LibraryInterface {
    /// The interface as JSON: what `native_pack --lib --describe` prints.
    pub fn to_json(&self) -> String {
        let string = json_string;
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\n  \"interface_hash\": \"{:016x}\",\n  \"functions\": [",
            self.hash
        );
        for (index, function) in self.functions.iter().enumerate() {
            out.push_str(if index == 0 { "\n    " } else { ",\n    " });
            let _ = write!(out, "{{\"number\": {index}, \"name\": ");
            string(&mut out, &function.name);
            out.push_str(", \"params\": [");
            for (place, param) in function.params.iter().enumerate() {
                if place > 0 {
                    out.push_str(", ");
                }
                out.push_str("{\"name\": ");
                string(&mut out, &param.name);
                out.push_str(", \"type\": ");
                string(&mut out, &param.ty);
                out.push_str(", \"tree\": ");
                param.tree.write_json(&mut out);
                let _ = write!(out, ", \"has_default\": {}}}", param.has_default);
            }
            out.push_str("], \"result\": ");
            string(&mut out, &function.result);
            out.push_str(", \"result_tree\": ");
            function.result_tree.write_json(&mut out);
            out.push_str(", \"throws\": ");
            match &function.throws {
                Some(throws) => string(&mut out, throws),
                None => out.push_str("null"),
            }
            out.push_str(", \"throws_tree\": ");
            match &function.throws_tree {
                Some(tree) => tree.write_json(&mut out),
                None => out.push_str("null"),
            }
            out.push_str(", \"signature\": ");
            string(&mut out, &function.signature);
            let _ = write!(
                out,
                ", \"throws_typed\": {}, \"hash\": \"{:016x}\"}}",
                function.throws_typed, function.hash
            );
        }
        out.push_str("\n  ],\n  \"types\": [");
        for (index, ty) in self.types.iter().enumerate() {
            out.push_str(if index == 0 { "\n    " } else { ",\n    " });
            let _ = write!(out, "{{\"number\": {index}, \"name\": ");
            string(&mut out, &ty.name);
            match &ty.kind {
                TypeKind::Class(fields) => {
                    out.push_str(", \"kind\": \"class\", \"fields\": [");
                    for (place, (name, field_ty)) in fields.iter().enumerate() {
                        if place > 0 {
                            out.push_str(", ");
                        }
                        let _ = write!(out, "{{\"slot\": {place}, \"name\": ");
                        string(&mut out, name);
                        out.push_str(", \"type\": ");
                        string(&mut out, field_ty);
                        if let Some(tree) = ty.field_trees.get(place) {
                            out.push_str(", \"tree\": ");
                            tree.write_json(&mut out);
                        }
                        out.push('}');
                    }
                    out.push_str("]}");
                }
                TypeKind::Enum(variants) => {
                    out.push_str(", \"kind\": \"enum\", \"variants\": [");
                    for (place, variant) in variants.iter().enumerate() {
                        if place > 0 {
                            out.push_str(", ");
                        }
                        string(&mut out, variant);
                    }
                    out.push_str("]}");
                }
            }
        }
        out.push_str("\n  ]\n}\n");
        out
    }
}
