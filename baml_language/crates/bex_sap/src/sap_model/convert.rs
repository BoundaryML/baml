//! Converting from [`baml_type`] types to SAP model types.

use std::{borrow::Cow, fmt};

use ::std::collections::{HashMap, HashSet};
use ::sys_types::{DefKey, EnumDefinition};
use baml_type::{Name, RealizedTy};
use indexmap::{IndexMap, IndexSet};

use crate::sap_model::{
    self, AnnotatedEnumVariant, AnnotatedField, ArrayTy, BigintLiteralTy, BigintTy, BoolLiteralTy,
    BoolTy, ClassTy, DefaultValue, EnumTy, EnumVariantTy, FloatTy, IntLiteralTy, IntTy, MapTy,
    MediaTy, NullTy, StringLiteralTy, StringTy, Ty, TyResolved, TypeRefDb, UnionTy,
};

/// A type as the parse model takes it: realized, so it names no type parameter
/// or associated-type projection, with each nominal head the declaration
/// identity the model's tables are keyed by.
pub type ParseTy = RealizedTy<DefKey>;

/// A named type of the parse model: a class, enum, type alias, or interface,
/// at the type arguments it is instantiated with.
///
/// It stands for exactly the type it names ([`NamedTy::to_ty`]): a value the
/// parser builds, and every container around one, is typed from it.
///
/// A generic class is a type constructor, not a type: `Box<int>` and
/// `Box<string>` are different types whose fields align different values, so
/// each instantiation a target reaches is its own name.
///
/// The arguments are kept in one spelling: each union flat and without repeated
/// members ([`RealizedTy::flatten_union`]), so `Box<(int | null) | null>` and
/// `Box<int | null>` are one name. Member order is not normalized away, as
/// SAP reads it: `Box<int | float>` and `Box<float | int>` are two names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NamedTy(NamedTyKind);

/// What a [`NamedTy`] names. Built only through [`NamedTy`]'s constructors, so
/// its arguments are always in the flat spelling.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NamedTyKind {
    Class(DefKey, Box<[ParseTy]>),
    Enum(DefKey),
    TypeAlias(DefKey),
    /// An interface existential, with its arguments and associated-type
    /// bindings. A parse has no implementor to build, so nothing parses as
    /// one; it is named so that a container of it is typed exactly.
    Interface(DefKey, Box<[ParseTy]>, Box<[(Name, ParseTy)]>),
}

impl NamedTy {
    /// `decl` instantiated at `args`.
    #[must_use]
    pub fn class(decl: DefKey, args: impl IntoIterator<Item = ParseTy>) -> Self {
        Self(NamedTyKind::Class(decl, flat_args(args)))
    }

    #[must_use]
    pub fn enumeration(decl: DefKey) -> Self {
        Self(NamedTyKind::Enum(decl))
    }

    #[must_use]
    pub fn type_alias(decl: DefKey) -> Self {
        Self(NamedTyKind::TypeAlias(decl))
    }

    #[must_use]
    pub fn interface(
        decl: DefKey,
        args: impl IntoIterator<Item = ParseTy>,
        associated: impl IntoIterator<Item = (Name, ParseTy)>,
    ) -> Self {
        Self(NamedTyKind::Interface(
            decl,
            flat_args(args),
            associated
                .into_iter()
                .map(|(name, ty)| (name, ty.flatten_union()))
                .collect(),
        ))
    }

    /// The named type of `ty`, if it is one: the inverse of [`Self::to_ty`].
    #[must_use]
    pub fn of(ty: &ParseTy) -> Option<Self> {
        match ty {
            ParseTy::Class(decl, args) => Some(Self::class(decl.clone(), args.iter().cloned())),
            ParseTy::Enum(decl) => Some(Self::enumeration(decl.clone())),
            ParseTy::TypeAlias(decl) => Some(Self::type_alias(decl.clone())),
            ParseTy::Interface(decl, args, associated) => Some(Self::interface(
                decl.clone(),
                args.iter().cloned(),
                associated.iter().cloned(),
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn kind(&self) -> &NamedTyKind {
        &self.0
    }

    /// The declaration named.
    #[must_use]
    pub fn decl(&self) -> &DefKey {
        match &self.0 {
            NamedTyKind::Class(decl, _)
            | NamedTyKind::Enum(decl)
            | NamedTyKind::TypeAlias(decl)
            | NamedTyKind::Interface(decl, _, _) => decl,
        }
    }

    /// Exactly the type this names.
    #[must_use]
    pub fn to_ty(&self) -> ParseTy {
        match &self.0 {
            NamedTyKind::Class(decl, args) => ParseTy::Class(decl.clone(), args.clone()),
            NamedTyKind::Enum(decl) => ParseTy::Enum(decl.clone()),
            NamedTyKind::TypeAlias(decl) => ParseTy::TypeAlias(decl.clone()),
            NamedTyKind::Interface(decl, args, associated) => {
                ParseTy::Interface(decl.clone(), args.clone(), associated.clone())
            }
        }
    }
}

fn flat_args(args: impl IntoIterator<Item = ParseTy>) -> Box<[ParseTy]> {
    args.into_iter().map(ParseTy::flatten_union).collect()
}

/// The declaration's name, then its arguments, as error messages show it.
impl fmt::Display for NamedTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.decl())?;
        let args: &[ParseTy] = match &self.0 {
            NamedTyKind::Class(_, args) | NamedTyKind::Interface(_, args, _) => args,
            NamedTyKind::Enum(_) | NamedTyKind::TypeAlias(_) => &[],
        };
        if let [first, rest @ ..] = args {
            write!(f, "<{first}")?;
            for arg in rest {
                write!(f, ", {arg}")?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

impl crate::sap_model::TypeIdent for NamedTy {}

/// The schema of one class instantiation ([`NamedTyKind::Class`]): the fields the parser reads
/// (`@skip` fields are not among them), each typed at the instantiation's
/// type arguments.
#[derive(Clone, Debug)]
pub struct ParseClass {
    pub fields: Vec<ParseField>,
    /// `@@stream.done`: no partial parse until the whole object is complete.
    pub stream_done: bool,
}

/// A field the parser reads.
#[derive(Clone, Debug)]
pub struct ParseField {
    pub name: String,
    pub ty: ParseTy,
    /// The name the output format shows, which the parser also reads.
    pub alias: Option<String>,
    /// `@stream.done`: the field holds its default until its value is complete.
    pub stream_done: bool,
    /// `@stream.must_exist`: the field has no default, so its class has no
    /// partial parse until the field is present.
    pub must_exist: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum ConvertError {
    #[error("Failed to parse float: {0}")]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[error("Unknown media kind")]
    UnknownMediaKind,
    #[error("Float literals cannot be parsed")]
    FloatLiteral,
    #[error("Non-parsable type: {0}")]
    NonParsableType(Box<ParseTy>),
    #[error("Unknown class: {0}")]
    UnknownClass(DefKey),
    #[error("Unknown enum: {0}")]
    UnknownEnum(DefKey),
    #[error("Unknown type alias: {0}")]
    UnknownTypeAlias(DefKey),
    #[error("Unknown name (could not determine if it was a class, enum, or type alias): {0}")]
    UnknownName(NamedTy),
    #[error("Could not add a type to the database as the name `{0}` is already present")]
    AlreadyPresent(NamedTy),
    #[error("Recursion depth exceeded for {0}")]
    RecursionDepthExceeded(&'static str),
    #[error("Unions must be flattened")]
    UnflattenedUnion,
    /// Something like `type A = B; type B = A;` is invalid.
    #[error("Recursive type alias without indirection: {0}")]
    DirectRecursiveTypeAlias(DefKey),
    #[error("Internal error (please report): {0}")]
    InternalError(&'static str),
}

const MAX_RECURSION_DEPTH: usize = 16;

/// The class instantiations, enums, and type aliases one parse target reaches, which the
/// converter needs to build the sap model. The caller reads them off the target's heap
/// declarations, so a parse pays only for the types its target uses, not for the whole
/// program's.
///
/// A class appears once per instantiation the target reaches, as its [`ParseClass`]: the
/// converter never sees a type parameter. Each interface the target reaches is named too,
/// as a type nothing parses as.
///
/// ## Representation
/// - Unions should be flattened:
///   - Union members cannot be unions
///   - Union members cannot be optional
///   - Union members cannot be type aliases which themselves resolve to unions (or optionals)
///   - Same rules for the inner type of an optional type
/// - Type aliases should be flattened:
///   - Type aliases cannot directly contain the name of another type alias (or itself)
///   - example: `type A = int; type B = A;` is invalid (`B` should be updated to directly reference `int`)
///   - Type aliases that contain a class or enum name *are* permitted.
///
/// The `new` method calls [`baml_type::simplify_sap::simplify`], which does all this.
#[allow(clippy::struct_field_names)]
pub struct TypeCtx {
    class_definitions: IndexMap<NamedTy, ParseClass>,
    enum_definitions: IndexMap<DefKey, EnumDefinition>,
    type_alias_definitions: HashMap<DefKey, ParseTy>,
    interfaces: IndexSet<NamedTy>,
    sap_parseable: HashMap<NamedTy, bool>,
}
impl TypeCtx {
    /// Builds the context from definitions that are closed under reference:
    /// every named type a class field, an alias body, or the target refers to
    /// is present too (the named types a type reaches on the heap are such a
    /// set). A missing one makes the types that refer to it unparsable.
    ///
    /// `class_definitions` are keyed by [`NamedTy::class`], and `interfaces`
    /// are each a [`NamedTy::interface`].
    pub fn new(
        class_definitions: impl IntoIterator<Item = (NamedTy, ParseClass)>,
        enum_definitions: IndexMap<DefKey, EnumDefinition>,
        type_alias_definitions: &HashMap<DefKey, ParseTy>,
        interfaces: impl IntoIterator<Item = NamedTy>,
    ) -> Self {
        let interfaces: IndexSet<NamedTy> = interfaces.into_iter().collect();
        // todo: we can hold more of this by reference probably
        let recursive_aliases = type_alias_definitions.keys().cloned().collect();
        let type_alias_definitions = type_alias_definitions
            .iter()
            .map(|(k, v)| {
                let v = ::baml_type::simplify_sap::simplify(
                    v.clone(),
                    type_alias_definitions,
                    &recursive_aliases,
                );
                (k.clone(), v)
            })
            .collect();
        let class_definitions: IndexMap<NamedTy, ParseClass> = class_definitions
            .into_iter()
            .map(|(k, class)| {
                let fields = class
                    .fields
                    .into_iter()
                    .map(|field| ParseField {
                        ty: ::baml_type::simplify_sap::simplify_parse_target(
                            field.ty,
                            &type_alias_definitions,
                            &recursive_aliases,
                        ),
                        ..field
                    })
                    .collect();
                (
                    k,
                    ParseClass {
                        fields,
                        stream_done: class.stream_done,
                    },
                )
            })
            .collect();
        // Recursively check which named types are SAP-parsable.
        // Types can be recursive (e.g. `class Tree { children: Tree[] }`), so we
        // track which names are currently being checked. If we encounter a name
        // already on the stack, we optimistically treat it as parsable — the
        // recursion itself is fine, only structurally unparsable leaves cause a
        // type to be non-parsable.
        let mut sap_parseable: HashMap<NamedTy, bool> = HashMap::new();
        let all_names: Vec<NamedTy> = class_definitions
            .keys()
            .cloned()
            .chain(enum_definitions.keys().cloned().map(NamedTy::enumeration))
            .chain(
                type_alias_definitions
                    .keys()
                    .cloned()
                    .map(NamedTy::type_alias),
            )
            .chain(interfaces.iter().cloned())
            .collect();

        let mut checking = HashSet::new();
        let definitions = Definitions {
            classes: &class_definitions,
            enums: &enum_definitions,
            aliases: &type_alias_definitions,
            interfaces: &interfaces,
        };
        for name in &all_names {
            check_parseable(name, &definitions, &mut sap_parseable, &mut checking);
        }

        Self {
            class_definitions,
            enum_definitions,
            type_alias_definitions,
            interfaces,
            sap_parseable,
        }
    }

    /// Normalize a runtime-materialized parse target before converting it to
    /// the SAP model. [`TypeCtx::new`] simplifies the class fields and alias
    /// bodies it is given, but not the target, which a generic caller can
    /// realize with a fresh nested union such as `(string | int) | ToolCalls`.
    pub(crate) fn normalize_parse_target(&self, ty: ParseTy) -> ParseTy {
        let recursive_aliases = self.type_alias_definitions.keys().cloned().collect();
        ::baml_type::simplify_sap::simplify_parse_target(
            ty,
            &self.type_alias_definitions,
            &recursive_aliases,
        )
    }

    /// Constructs a full [`TypeRefDb`] from the given context with all types converted.
    pub fn build_db(&self) -> Result<TypeRefDb<'_, NamedTy>, ConvertError> {
        fn add<'a>(
            types: &mut IndexMap<NamedTy, TyResolved<'a, NamedTy>>,
            name: NamedTy,
            ty: TyResolved<'a, NamedTy>,
        ) -> Result<(), ConvertError> {
            match types.entry(name) {
                indexmap::map::Entry::Occupied(entry) => {
                    Err(ConvertError::AlreadyPresent(entry.key().clone()))
                }
                indexmap::map::Entry::Vacant(entry) => {
                    entry.insert(ty);
                    Ok(())
                }
            }
        }

        let mut types = IndexMap::new();
        for (name, cls) in &self.class_definitions {
            if self.sap_parseable.get(name).is_some_and(|v| !v) {
                continue;
            }
            add(
                &mut types,
                name.clone(),
                TyResolved::Class(self.convert_class(name, cls)?),
            )?;
        }
        for (name, enum_def) in &self.enum_definitions {
            add(
                &mut types,
                NamedTy::enumeration(name.clone()),
                TyResolved::Enum(Self::convert_enum(name, enum_def)),
            )?;
        }
        for name in &self.interfaces {
            add(&mut types, name.clone(), never())?;
        }
        for (name, alias_ty) in &self.type_alias_definitions {
            let key = NamedTy::type_alias(name.clone());
            if self.sap_parseable.get(&key).is_some_and(|v| !v) {
                continue;
            }
            let alias = self.convert_type_alias(name, alias_ty, 0)?;
            add(&mut types, key, alias)?;
        }
        Ok(TypeRefDb::from_types(types))
    }

    fn convert_class<'a>(
        &'a self,
        name: &NamedTy,
        class_def: &'a ParseClass,
    ) -> Result<ClassTy<'a, NamedTy>, ConvertError> {
        let fields = class_def
            .fields
            .iter()
            .map(|field| {
                let ParseField {
                    name,
                    ty,
                    alias,
                    stream_done,
                    must_exist,
                } = field;
                Ok(AnnotatedField {
                    name: Cow::Borrowed(name),
                    ty: self.convert_ty(ty)?,
                    aliases: alias.iter().map(Into::into).collect(),
                    stream_done: *stream_done,
                    default: must_exist.then_some(DefaultValue::Never),
                })
            })
            .collect::<Result<_, ConvertError>>()?;
        Ok(ClassTy {
            name: name.clone(),
            fields,
            stream_done: class_def.stream_done,
        })
    }

    fn convert_enum<'a>(
        name: &DefKey,
        enum_def: &'a ::sys_types::EnumDefinition,
    ) -> EnumTy<'a, NamedTy> {
        let variants = enum_def
            .variants()
            .map(|variant| AnnotatedEnumVariant {
                name: variant.name.as_str().into(),
                aliases: variant.alias.iter().map(|a| a.as_str().into()).collect(),
                description: variant.rendered_description().map(Into::into),
            })
            .collect();

        EnumTy {
            name: NamedTy::enumeration(name.clone()),
            variants,
        }
    }

    /// Converts a type alias declaration into a sap model type for inclusion in a [`TypeRefDb`].
    ///
    /// ## Arguments
    /// - `name`: The name of the type alias. Not used in the output, only for error checking and reporting.
    /// - `alias_ty`: The type alias declaration.
    /// - `recursion_depth`: The current recursion depth. Used to prevent infinite recursion.
    fn convert_type_alias<'a>(
        &'a self,
        name: &DefKey,
        alias_ty: &'a ParseTy,
        recursion_depth: usize,
    ) -> Result<TyResolved<'a, NamedTy>, ConvertError> {
        if recursion_depth > MAX_RECURSION_DEPTH {
            return Err(ConvertError::RecursionDepthExceeded("type alias"));
        }

        let resolved = match self.convert_ty(alias_ty)? {
            sap_model::Ty::Resolved(r) => r,
            sap_model::Ty::ResolvedRef(..) => {
                return Err(ConvertError::InternalError(concat!(
                    file!(),
                    ":",
                    line!(),
                    ": `DbBuilder::convert_ty` returned `sap_model::Ty::ResolvedRef`"
                )));
            }
            sap_model::Ty::Unresolved(inner_name) => {
                // I think this should usually already be resolved, but we can do our best here.
                if inner_name.decl() == name {
                    return Err(ConvertError::RecursionDepthExceeded(
                        "type alias due to self-reference without indirection",
                    ));
                }
                let found = match inner_name.kind() {
                    NamedTyKind::Class(..) => {
                        self.class_definitions.get(&inner_name).map(|class_ty| {
                            self.convert_class(&inner_name, class_ty)
                                .map(TyResolved::Class)
                        })
                    }
                    NamedTyKind::Enum(decl) => self
                        .enum_definitions
                        .get(decl)
                        .map(|enum_ty| Ok(TyResolved::Enum(Self::convert_enum(decl, enum_ty)))),
                    NamedTyKind::TypeAlias(decl) => {
                        self.type_alias_definitions.get(decl).map(|alias_ty| {
                            self.convert_type_alias(decl, alias_ty, recursion_depth + 1)
                        })
                    }
                    NamedTyKind::Interface(..) => {
                        self.interfaces.contains(&inner_name).then(|| Ok(never()))
                    }
                };
                return found.unwrap_or(Err(ConvertError::UnknownName(inner_name)));
            }
        };
        Ok(resolved)
    }

    /// Converts a BAML type into a sap model type.
    pub fn convert_ty<'a>(&'a self, ty: &'a ParseTy) -> Result<Ty<'a, NamedTy>, ConvertError> {
        let ty = match ty {
            ParseTy::Int => Ty::Resolved(TyResolved::Int(IntTy)),
            ParseTy::Bigint => Ty::Resolved(TyResolved::Bigint(BigintTy)),
            ParseTy::Float => Ty::Resolved(TyResolved::Float(FloatTy)),
            ParseTy::String => Ty::Resolved(TyResolved::String(StringTy)),
            ParseTy::Bool => Ty::Resolved(TyResolved::Bool(BoolTy)),
            ParseTy::Null => Ty::Resolved(TyResolved::Null(NullTy)),
            ParseTy::Media(media_kind) => {
                let media_kind = match media_kind {
                    baml_type::MediaKind::Image => MediaTy::Image,
                    baml_type::MediaKind::Video => MediaTy::Video,
                    baml_type::MediaKind::Audio => MediaTy::Audio,
                    baml_type::MediaKind::Pdf => MediaTy::Pdf,
                    baml_type::MediaKind::Generic => {
                        return Err(ConvertError::UnknownMediaKind);
                    }
                };
                Ty::Resolved(TyResolved::Media(media_kind))
            }
            ParseTy::Literal(baml_type::Literal::Int(i), _) => {
                Ty::Resolved(TyResolved::LiteralInt(IntLiteralTy(*i)))
            }
            ParseTy::Literal(baml_type::Literal::Bigint(bi), _) => {
                Ty::Resolved(TyResolved::LiteralBigint(BigintLiteralTy(bi.clone())))
            }
            ParseTy::Literal(baml_type::Literal::Float(..), ..) => {
                return Err(ConvertError::FloatLiteral);
            }
            ParseTy::Literal(baml_type::Literal::String(s), _) => {
                Ty::Resolved(TyResolved::LiteralString(StringLiteralTy(Cow::Borrowed(s))))
            }
            ParseTy::Literal(baml_type::Literal::Bool(b), _) => {
                Ty::Resolved(TyResolved::LiteralBool(BoolLiteralTy(*b)))
            }
            ParseTy::Class(..) | ParseTy::Interface(..) => {
                let name = NamedTy::of(ty)
                    .unwrap_or_else(|| unreachable!("a class or interface is named"));
                if self.sap_parseable.get(&name).is_some_and(|v| !v) {
                    return Err(ConvertError::NonParsableType(Box::new(ty.clone())));
                }
                Ty::Unresolved(name)
            }
            ParseTy::Enum(type_name) => Ty::Unresolved(NamedTy::enumeration(type_name.clone())),
            ParseTy::EnumVariant(type_name, variant) => {
                let enum_def = self
                    .enum_definitions
                    .get(type_name)
                    .ok_or_else(|| ConvertError::UnknownEnum(type_name.clone()))?;
                let variant_def = enum_def
                    .variants()
                    .find(|v| v.name == AsRef::<str>::as_ref(variant))
                    .ok_or(ConvertError::InternalError(
                        "enum variant not found in enum definition",
                    ))?;
                let enum_variant_ty = EnumVariantTy {
                    name: NamedTy::enumeration(type_name.clone()),
                    value: AnnotatedEnumVariant {
                        name: variant_def.name.as_str().into(),
                        aliases: variant_def
                            .alias
                            .iter()
                            .map(|a| a.as_str().into())
                            .collect(),
                        description: variant_def.rendered_description().map(Into::into),
                    },
                };
                Ty::Resolved(TyResolved::EnumVariant(enum_variant_ty))
            }
            ParseTy::List(ty) => Ty::Resolved(TyResolved::Array(ArrayTy {
                ty: Box::new(self.convert_ty(ty)?),
            })),
            ParseTy::Map { key, value } => Ty::Resolved(TyResolved::Map(MapTy {
                key: Box::new(self.convert_ty(key)?),
                value: Box::new(self.convert_ty(value)?),
            })),
            ParseTy::Union(items) => {
                if items.iter().any(|ty| self.is_union_like(ty)) {
                    return Err(ConvertError::UnflattenedUnion);
                }
                let variants = items
                    .iter()
                    .map(|ty| self.convert_ty(ty))
                    .collect::<Result<Vec<_>, _>>()?;
                Ty::Resolved(TyResolved::Union(UnionTy { variants }))
            }
            ParseTy::TypeAlias(type_name) => {
                if self
                    .sap_parseable
                    .get(&NamedTy::type_alias(type_name.clone()))
                    .is_some_and(|v| !v)
                {
                    return Err(ConvertError::NonParsableType(Box::new(ty.clone())));
                }
                // if it hasn't already, we flatten type aliases:
                // with `type A = B; type B = C; class C { ... }`,
                // a type reference `name:A` becomes `name:C`
                let mut innermost_name = type_name;
                let innermost = loop {
                    let Some(inner_ty) = self.type_alias_definitions.get(innermost_name) else {
                        return Err(ConvertError::UnknownTypeAlias(innermost_name.clone()));
                    };
                    match inner_ty {
                        ParseTy::TypeAlias(name) => {
                            if innermost_name == type_name {
                                return Err(ConvertError::DirectRecursiveTypeAlias(
                                    type_name.clone(),
                                ));
                            }
                            innermost_name = name;
                        }
                        ParseTy::Class(name, args) => {
                            break NamedTy::class(name.clone(), args.iter().cloned());
                        }
                        ParseTy::Enum(name) => break NamedTy::enumeration(name.clone()),
                        // Any other body, interfaces included, keeps the alias
                        // named: its entry resolves to the body.
                        _ => break NamedTy::type_alias(innermost_name.clone()),
                    }
                };
                Ty::Unresolved(innermost)
            }
            unparsable @ (ParseTy::Uint8Array
            | ParseTy::Resource
            | ParseTy::PromptAst
            | ParseTy::Function { .. }
            | ParseTy::Unknown
            | ParseTy::Future(_, _)
            | ParseTy::Never
            | ParseTy::RustType
            | ParseTy::Type) => {
                return Err(ConvertError::NonParsableType(Box::new(unparsable.clone())));
            }
        };
        Ok(ty)
    }

    fn is_union_like(&self, ty: &ParseTy) -> bool {
        match ty {
            ParseTy::Union(..) => true,
            ParseTy::TypeAlias(name, ..) => self
                .type_alias_definitions
                .get(name)
                .is_some_and(|ty| self.is_union_like(ty)),
            _ => false,
        }
    }
}

/// The named types a [`TypeCtx`] holds, for [`check_parseable`].
struct Definitions<'a> {
    classes: &'a IndexMap<NamedTy, ParseClass>,
    enums: &'a IndexMap<DefKey, EnumDefinition>,
    aliases: &'a HashMap<DefKey, ParseTy>,
    interfaces: &'a IndexSet<NamedTy>,
}

/// The model of a type nothing parses as: `never`, the empty union. An
/// interface is one to the parser (see [`NamedTyKind::Interface`]).
fn never<'a>() -> TyResolved<'a, NamedTy> {
    TyResolved::Union(UnionTy { variants: vec![] })
}

/// Whether the named type `name` can be modeled: a class whose read fields all
/// can, any enum, an alias whose body can, and any interface. A name the
/// context does not hold cannot be.
fn check_parseable(
    name: &NamedTy,
    definitions: &Definitions<'_>,
    cache: &mut HashMap<NamedTy, bool>,
    checking: &mut HashSet<NamedTy>,
) -> bool {
    if let Some(&result) = cache.get(name) {
        return result;
    }
    // Recursive type — assume parsable to break the cycle.
    if !checking.insert(name.clone()) {
        return true;
    }

    let mut deps_parseable = |ty: &ParseTy| match is_sap_parseable(ty) {
        Err(()) => false,
        Ok(deps) => deps
            .iter()
            .all(|dep| check_parseable(dep, definitions, cache, checking)),
    };
    let result = match name.kind() {
        NamedTyKind::Class(..) => definitions
            .classes
            .get(name)
            .is_some_and(|class| class.fields.iter().all(|field| deps_parseable(&field.ty))),
        NamedTyKind::Enum(decl) => definitions.enums.contains_key(decl),
        NamedTyKind::TypeAlias(decl) => definitions
            .aliases
            .get(decl)
            .is_some_and(&mut deps_parseable),
        NamedTyKind::Interface(..) => definitions.interfaces.contains(name),
    };

    checking.remove(name);
    cache.insert(name.clone(), result);
    result
}

fn is_sap_parseable(ty: &ParseTy) -> Result<Vec<NamedTy>, ()> {
    match ty {
        ParseTy::Int
        | ParseTy::Bigint
        | ParseTy::Float
        | ParseTy::String
        | ParseTy::Bool
        | ParseTy::Null
        | ParseTy::Literal(..) => Ok(Vec::new()),
        ParseTy::Uint8Array | ParseTy::Media(..) => Err(()),
        ParseTy::Class(..) | ParseTy::Interface(..) => Ok(NamedTy::of(ty).into_iter().collect()),
        ParseTy::Enum(..) | ParseTy::EnumVariant(..) => Ok(Vec::new()),
        ParseTy::List(inner) => is_sap_parseable(inner),
        ParseTy::Map { key, value, .. } => {
            let keys = is_sap_parseable(key)?;
            let values = is_sap_parseable(value)?;
            Ok(keys.into_iter().chain(values).collect())
        }
        ParseTy::Union(members) => {
            let mut names = Vec::new();
            for member in members {
                names.extend(is_sap_parseable(member)?);
            }
            Ok(names)
        }
        ParseTy::TypeAlias(name) => Ok(vec![NamedTy::type_alias(name.clone())]),
        ParseTy::Resource
        | ParseTy::PromptAst
        | ParseTy::Function { .. }
        | ParseTy::Unknown
        | ParseTy::Future(..)
        | ParseTy::Never
        | ParseTy::RustType
        | ParseTy::Type => Err(()),
    }
}
