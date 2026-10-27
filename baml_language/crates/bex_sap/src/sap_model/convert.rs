//! Converting from [`baml_type`] types to SAP model types.

use std::{borrow::Cow, fmt};

use ::std::collections::{HashMap, HashSet};
use ::sys_types::{DefKey, EnumDefinition};
use baml_type::{
    Name, RealizedTy, RuntimeTy,
    json_inhabit::{
        JsonClass, JsonDefs, JsonField, JsonInhabitance, JsonNaming, JsonStatus, JsonVariant,
    },
};
use indexmap::{IndexMap, IndexSet};

use crate::sap_model::{
    self, AnnotatedEnumVariant, AnnotatedField, ArrayTy, BigintLiteralTy, BigintTy, BoolLiteralTy,
    BoolTy, ClassTy, DefaultValue, EnumTy, EnumVariantTy, FloatTy, IntLiteralTy, IntTy, MapTy,
    MediaTy, NullTy, ShapedTy, StringLiteralTy, StringTy, Ty, TyResolved, TypeRefDb, UnionTy,
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
    /// A type none of whose values a parse can build: one with no JSON form,
    /// such as `uint8array`, or with no values, such as `never`. Nothing
    /// parses as one; like an interface, it is named so that a container of
    /// it is typed exactly. `uint8array | string` parses as `string`, and its
    /// list stays a `(uint8array | string)[]`.
    Unparsable(ParseTy),
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

    /// `ty`, a type none of whose values a parse can build.
    #[must_use]
    pub fn unparsable(ty: ParseTy) -> Self {
        Self(NamedTyKind::Unparsable(ty))
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

    /// The declaration named, or `None` for an unparsable type, which names
    /// no declaration.
    #[must_use]
    pub fn decl(&self) -> Option<&DefKey> {
        match &self.0 {
            NamedTyKind::Class(decl, _)
            | NamedTyKind::Enum(decl)
            | NamedTyKind::TypeAlias(decl)
            | NamedTyKind::Interface(decl, _, _) => Some(decl),
            NamedTyKind::Unparsable(_) => None,
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
            NamedTyKind::Unparsable(ty) => ty.clone(),
        }
    }
}

fn flat_args(args: impl IntoIterator<Item = ParseTy>) -> Box<[ParseTy]> {
    args.into_iter().map(ParseTy::flatten_union).collect()
}

/// The declaration's name, then its arguments, as error messages show it.
impl fmt::Display for NamedTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (decl, args): (&DefKey, &[ParseTy]) = match &self.0 {
            NamedTyKind::Class(decl, args) | NamedTyKind::Interface(decl, args, _) => (decl, args),
            NamedTyKind::Enum(decl) | NamedTyKind::TypeAlias(decl) => (decl, &[]),
            NamedTyKind::Unparsable(ty) => return write!(f, "{ty}"),
        };
        write!(f, "{decl}")?;
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
    /// The first `@skip` field whose type has no empty value for a parsed
    /// instance to hold. A parse cannot build such a class.
    pub unfillable: Option<String>,
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
    #[error("no value of `{ty}` has a JSON form: {reason}")]
    NoJsonForm { ty: Box<ParseTy>, reason: String },
    #[error("`{0}` has no values")]
    NoValues(Box<ParseTy>),
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
    /// A `baml.FromLlmOutput` shape that reaches its own type with no class
    /// in between, such as `type Shape = Self | string`: reading it would
    /// read it again, forever. The output format refuses it the same way.
    #[error("The baml.FromLlmOutput shape of '{0}' holds '{0}' itself")]
    ShapeHoldsItself(Box<ParseTy>),
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
/// as a type nothing parses as. A class instantiation or enum that implements
/// `baml.FromLlmOutput` appears as its shape instead ([`ShapedTy`]), and its fields or
/// variants are not needed.
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
    /// Each class instantiation or enum a model writes as another type, its
    /// `baml.FromLlmOutput` shape, keyed by [`NamedTy::class`] or
    /// [`NamedTy::enumeration`].
    shapes: IndexMap<NamedTy, ParseTy>,
    /// Which of the types the definitions and the target name a parse can
    /// build ([`Self::analyzed`]). `None` until then, when every type is
    /// taken to be one.
    json: Option<JsonInhabitance<DefKey>>,
    /// The types [`Self::convert_ty`] models as [`NamedTyKind::Unparsable`],
    /// each of which the database holds as `never`.
    unparsable: IndexSet<NamedTy>,
}
impl TypeCtx {
    /// Builds the context from definitions that are closed under reference:
    /// every named type a class field, an alias body, or the target refers to
    /// is present too (the named types a type reaches on the heap are such a
    /// set). A missing one fails [`crate::CompiledSapModel::from_type_ctx`].
    ///
    /// `class_definitions` are keyed by [`NamedTy::class`], and `interfaces`
    /// are each a [`NamedTy::interface`]. `shapes` holds the shape of each
    /// class instantiation or enum a model writes as another type, keyed by
    /// [`NamedTy::class`] or [`NamedTy::enumeration`]; such a type needs no
    /// definition.
    pub fn new(
        class_definitions: impl IntoIterator<Item = (NamedTy, ParseClass)>,
        enum_definitions: IndexMap<DefKey, EnumDefinition>,
        type_alias_definitions: &HashMap<DefKey, ParseTy>,
        interfaces: impl IntoIterator<Item = NamedTy>,
        shapes: impl IntoIterator<Item = (NamedTy, ParseTy)>,
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
                        unfillable: class.unfillable,
                    },
                )
            })
            .collect();
        let shapes = shapes
            .into_iter()
            .map(|(name, shape)| {
                let shape = ::baml_type::simplify_sap::simplify_parse_target(
                    shape,
                    &type_alias_definitions,
                    &recursive_aliases,
                );
                (name, shape)
            })
            .collect();

        Self {
            class_definitions,
            enum_definitions,
            type_alias_definitions,
            interfaces,
            shapes,
            json: None,
            unparsable: IndexSet::new(),
        }
    }

    /// The context, readied to parse `target`: which of the types it and the
    /// definitions name a parse can build. A union member it cannot build is
    /// dropped, so `uint8array | string` parses as `string`, and an optional
    /// field of such a type is only `null`.
    ///
    /// The analysis is the one `baml.json.schema` makes, under the names a
    /// model is shown: a type with no JSON form or no values is one a parse
    /// cannot build. Nothing parses as an interface either, and media matches
    /// no parsed value, though it keeps its place.
    ///
    /// # Errors
    ///
    /// A type the parser cannot take at all (`unknown`), or a definition the
    /// context does not hold.
    pub(crate) fn analyzed(mut self, target: &ParseTy) -> Result<Self, ConvertError> {
        let mut roots: Vec<RuntimeTy<DefKey>> = vec![target.into()];
        for name in self.class_definitions.keys() {
            roots.push(name.to_ty().into());
        }
        for name in self.type_alias_definitions.keys() {
            roots.push(RuntimeTy::TypeAlias(name.clone()));
        }
        for name in self.shapes.keys() {
            roots.push(name.to_ty().into());
        }
        let json = JsonInhabitance::of_all(roots.iter(), &self, JsonNaming::Aliased)
            .map_err(|e| e.error)?;
        let mut unparsable = IndexSet::new();
        let mut visit = |ty: &ParseTy| collect_unparsable(&json, ty, &mut unparsable);
        visit(target);
        for class in self.class_definitions.values() {
            for field in &class.fields {
                visit(&field.ty);
            }
        }
        for body in self.type_alias_definitions.values() {
            visit(body);
        }
        for shape in self.shapes.values() {
            visit(shape);
        }
        self.json = Some(json);
        self.unparsable = unparsable;
        Ok(self)
    }

    /// Refuse a shape that reaches its own type with no class in between,
    /// as the output format and `baml.json.schema` do. A union, list, map,
    /// type alias, or another shaped type does not stop the walk, so
    /// `type Shape = Self | string` and `type Shape = Self[]` are refused. A
    /// class does: it is read by its fields, one level of the answer deeper.
    pub(crate) fn reject_self_holding_shapes(&self) -> Result<(), ConvertError> {
        for (name, shape) in &self.shapes {
            let mut seen = HashSet::new();
            if self.shape_reaches(shape, name, &mut seen) {
                return Err(ConvertError::ShapeHoldsItself(Box::new(name.to_ty())));
            }
        }
        Ok(())
    }

    /// Whether `ty` reaches the shaped type `target` with no class in between.
    /// `seen` holds the shaped types and aliases already walked.
    fn shape_reaches(&self, ty: &ParseTy, target: &NamedTy, seen: &mut HashSet<NamedTy>) -> bool {
        match ty {
            ParseTy::Class(..) | ParseTy::Enum(_) => {
                let Some(name) = NamedTy::of(ty) else {
                    return false;
                };
                if name == *target {
                    return true;
                }
                match self.shapes.get(&name) {
                    Some(shape) => {
                        seen.insert(name.clone()) && self.shape_reaches(shape, target, seen)
                    }
                    None => false,
                }
            }
            ParseTy::TypeAlias(decl) => {
                seen.insert(NamedTy::type_alias(decl.clone()))
                    && self
                        .type_alias_definitions
                        .get(decl)
                        .is_some_and(|body| self.shape_reaches(body, target, seen))
            }
            ParseTy::List(element) => self.shape_reaches(element, target, seen),
            ParseTy::Map { key, value } => {
                self.shape_reaches(key, target, seen) || self.shape_reaches(value, target, seen)
            }
            ParseTy::Union(members) => members
                .iter()
                .any(|member| self.shape_reaches(member, target, seen)),
            _ => false,
        }
    }

    /// Whether a parse can build a value of `ty`, a type the context names.
    pub(crate) fn status(&self, ty: &ParseTy) -> JsonStatus {
        self.json
            .as_ref()
            .map_or(JsonStatus::Json, |json| json.status(ty.as_runtime_ty()))
    }

    /// Why no value of `target` can be parsed, if none can.
    pub(crate) fn reject_target(&self, target: &ParseTy) -> Result<(), ConvertError> {
        let Some(json) = &self.json else {
            return Ok(());
        };
        let ty = target.as_runtime_ty();
        match json.status(ty) {
            JsonStatus::Json => Ok(()),
            JsonStatus::NoJson => Err(ConvertError::NoJsonForm {
                ty: Box::new(target.clone()),
                reason: json.witness(ty).reason(),
            }),
            JsonStatus::Empty => Err(ConvertError::NoValues(Box::new(target.clone()))),
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
        for (name, shape) in &self.shapes {
            // As for a class: a type that holds one a parse cannot build is
            // modeled as unparsable.
            if self.status(&name.to_ty()) != JsonStatus::Json {
                continue;
            }
            add(
                &mut types,
                name.clone(),
                TyResolved::Shaped(self.convert_shaped(name, shape)?),
            )?;
        }
        for (name, cls) in &self.class_definitions {
            // A class a parse cannot build is never named: a type that holds
            // one is modeled as unparsable.
            if self.status(&name.to_ty()) != JsonStatus::Json || self.shapes.contains_key(name) {
                continue;
            }
            add(
                &mut types,
                name.clone(),
                TyResolved::Class(self.convert_class(name, cls)?),
            )?;
        }
        for (name, enum_def) in &self.enum_definitions {
            if self
                .shapes
                .contains_key(&NamedTy::enumeration(name.clone()))
            {
                continue;
            }
            add(
                &mut types,
                NamedTy::enumeration(name.clone()),
                TyResolved::Enum(Self::convert_enum(name, enum_def)),
            )?;
        }
        for name in self.interfaces.iter().chain(&self.unparsable) {
            add(&mut types, name.clone(), never())?;
        }
        for (name, alias_ty) in &self.type_alias_definitions {
            let key = NamedTy::type_alias(name.clone());
            if self.status(&key.to_ty()) != JsonStatus::Json {
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

    fn convert_shaped<'a>(
        &'a self,
        name: &NamedTy,
        shape: &'a ParseTy,
    ) -> Result<ShapedTy<'a, NamedTy>, ConvertError> {
        Ok(ShapedTy {
            name: name.clone(),
            shape: Box::new(self.convert_ty(shape)?),
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
                if inner_name.decl() == Some(name) {
                    return Err(ConvertError::RecursionDepthExceeded(
                        "type alias due to self-reference without indirection",
                    ));
                }
                if let Some(shape) = self.shapes.get(&inner_name) {
                    return self
                        .convert_shaped(&inner_name, shape)
                        .map(TyResolved::Shaped);
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
                    NamedTyKind::Unparsable(_) => {
                        self.unparsable.contains(&inner_name).then(|| Ok(never()))
                    }
                };
                return found.unwrap_or(Err(ConvertError::UnknownName(inner_name)));
            }
        };
        Ok(resolved)
    }

    /// Converts a BAML type into a sap model type. A type a parse cannot
    /// build is modeled as [`NamedTyKind::Unparsable`], which parses as
    /// nothing.
    pub fn convert_ty<'a>(&'a self, ty: &'a ParseTy) -> Result<Ty<'a, NamedTy>, ConvertError> {
        if self.status(ty) != JsonStatus::Json {
            return Ok(Ty::Unresolved(NamedTy::unparsable(ty.clone())));
        }
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
            ParseTy::Class(..) | ParseTy::Interface(..) => Ty::Unresolved(
                NamedTy::of(ty).unwrap_or_else(|| unreachable!("a class or interface is named")),
            ),
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

/// The model of a type nothing parses as: `never`, the empty union. An
/// interface is one to the parser (see [`NamedTyKind::Interface`]).
fn never<'a>() -> TyResolved<'a, NamedTy> {
    TyResolved::Union(UnionTy { variants: vec![] })
}

/// Records in `out` each type [`TypeCtx::convert_ty`] models as unparsable
/// while converting `ty`: the types a parse cannot build, wherever the
/// conversion reaches one.
fn collect_unparsable(json: &JsonInhabitance<DefKey>, ty: &ParseTy, out: &mut IndexSet<NamedTy>) {
    if json.status(ty.as_runtime_ty()) != JsonStatus::Json {
        out.insert(NamedTy::unparsable(ty.clone()));
        return;
    }
    match ty {
        ParseTy::List(inner) => collect_unparsable(json, inner, out),
        ParseTy::Map { key, value } => {
            collect_unparsable(json, key, out);
            collect_unparsable(json, value, out);
        }
        ParseTy::Union(members) => {
            for member in members {
                collect_unparsable(json, member, out);
            }
        }
        // A named type converts by name, and every other type has no part
        // that converts on its own.
        _ => {}
    }
}

/// The context's definitions, as the JSON analysis reads them: the fields a
/// parse reads, the variants a model is shown, and the alias bodies after
/// simplification.
impl JsonDefs<DefKey> for TypeCtx {
    type Error = ConvertError;

    fn class(
        &self,
        head: &DefKey,
        args: &[RuntimeTy<DefKey>],
    ) -> Result<JsonClass<DefKey>, ConvertError> {
        let args = args
            .iter()
            .map(|arg| {
                ParseTy::try_from(arg)
                    .map_err(|_| ConvertError::InternalError("a class argument is not realized"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let name = NamedTy::class(head.clone(), args);
        let class = self
            .class_definitions
            .get(&name)
            .ok_or_else(|| ConvertError::UnknownName(name.clone()))?;
        let mut fields: Vec<JsonField<DefKey>> = class
            .fields
            .iter()
            .map(|field| JsonField {
                name: field.name.clone(),
                alias: field.alias.clone(),
                skip: false,
                ty: (&field.ty).into(),
            })
            .collect();
        // The `@skip` fields are not part of a parse class, only the first
        // one with no empty value. A `never` field, which has none either,
        // stands in for it.
        if let Some(field) = &class.unfillable {
            fields.push(JsonField {
                name: field.clone(),
                alias: None,
                skip: true,
                ty: RuntimeTy::Never,
            });
        }
        Ok(JsonClass { fields })
    }

    fn variants(&self, head: &DefKey) -> Result<Vec<JsonVariant>, ConvertError> {
        let enum_def = self
            .enum_definitions
            .get(head)
            .ok_or_else(|| ConvertError::UnknownEnum(head.clone()))?;
        Ok(enum_def
            .declared_variants
            .iter()
            .map(|variant| JsonVariant {
                name: variant.name.clone(),
                skip: variant.skip,
            })
            .collect())
    }

    fn shape(&self, ty: &RuntimeTy<DefKey>) -> Result<Option<RuntimeTy<DefKey>>, ConvertError> {
        let name = ParseTy::try_from(ty)
            .ok()
            .and_then(|ty| NamedTy::of(&ty))
            .ok_or(ConvertError::InternalError("a shaped type is not realized"))?;
        Ok(self.shapes.get(&name).map(Into::into))
    }

    fn alias_target(&self, head: &DefKey) -> Result<RuntimeTy<DefKey>, ConvertError> {
        self.type_alias_definitions
            .get(head)
            .map(Into::into)
            .ok_or_else(|| ConvertError::UnknownTypeAlias(head.clone()))
    }

    fn opaque(&self, ty: &RuntimeTy<DefKey>) -> Result<JsonStatus, ConvertError> {
        match ty {
            RuntimeTy::Media(baml_type::MediaKind::Generic) => Err(ConvertError::UnknownMediaKind),
            // A parsed value is never media, but the type keeps its place:
            // `image?` parses as `null` and `image[]` as `[]`.
            RuntimeTy::Media(_) => Ok(JsonStatus::Json),
            // A parse has no implementor to build.
            RuntimeTy::Interface(..) => Ok(JsonStatus::Empty),
            _ => Err(match ParseTy::try_from(ty) {
                Ok(ty) => ConvertError::NonParsableType(Box::new(ty)),
                Err(_) => ConvertError::InternalError("a parse target is not realized"),
            }),
        }
    }
}
