//! Converting from [`baml_type`] types to SAP model types.

use std::borrow::Cow;

use ::std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use ::sys_types::{ClassDefinition, DefKey, EnumDefinition, SapTy};
use indexmap::IndexMap;

use crate::sap_model::{
    self, AnnotatedEnumVariant, AnnotatedField, ArrayTy, BigintLiteralTy, BigintTy, BoolLiteralTy,
    BoolTy, ClassTy, DefaultValue, EnumTy, EnumVariantTy, FloatTy, IntLiteralTy, IntTy, MapTy,
    MediaTy, NullTy, StringLiteralTy, StringTy, Ty, TyResolved, TypeRefDb, UnionTy,
};

impl crate::sap_model::TypeIdent for DefKey {}

#[derive(thiserror::Error, Debug)]
pub enum ConvertError {
    #[error("Failed to parse float: {0}")]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[error("Unknown media kind")]
    UnknownMediaKind,
    #[error("Float literals cannot be parsed")]
    FloatLiteral,
    #[error("Non-parsable type: {0:?}")]
    NonParsableType(Box<SapTy>),
    #[error("Unknown class: {0}")]
    UnknownClass(DefKey),
    #[error("Unknown enum: {0}")]
    UnknownEnum(DefKey),
    #[error("Unknown type alias: {0}")]
    UnknownTypeAlias(DefKey),
    #[error("Unknown name (could not determine if it was a class, enum, or type alias): {0}")]
    UnknownName(DefKey),
    #[error("Could not add a type to the database as the name `{0}` is already present")]
    AlreadyPresent(DefKey),
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

/// Contains stuff from [`sys_types::SysOpContext`] that we need for converting to the sap model.
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
/// A the `new` method calls [`baml_type::simplify_sap::simplify`] which should do all this
#[allow(clippy::struct_field_names)]
pub struct TypeCtx {
    class_definitions: IndexMap<DefKey, ClassDefinition>,
    enum_definitions: Arc<IndexMap<DefKey, EnumDefinition>>,
    type_alias_definitions: HashMap<DefKey, SapTy>,
    sap_parseable: HashMap<DefKey, bool>,
}
impl TypeCtx {
    /// The reason `enum_definitions` is an `Arc` while the others aren't is that
    /// we do transformations on the others (so we don't need arc) but we don't
    /// need to transform `enum_definitions` so we can just share it.
    pub fn new(
        class_definitions: &IndexMap<DefKey, ClassDefinition>,
        enum_definitions: Arc<IndexMap<DefKey, EnumDefinition>>,
        type_alias_definitions: &HashMap<DefKey, SapTy>,
    ) -> Self {
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
        let class_definitions: IndexMap<DefKey, ClassDefinition> = class_definitions
            .iter()
            .map(|(k, v)| {
                let fields = v.fields.iter().map(|field| {
                    let mut field = field.clone();
                    field.field_type = ::baml_type::simplify_sap::simplify_parse_target(
                        field.field_type,
                        &type_alias_definitions,
                        &recursive_aliases,
                    );
                    field
                });
                let class = ClassDefinition {
                    name: v.name.clone(),
                    description: v.description.clone(),
                    docstring: v.docstring.clone(),
                    alias: v.alias.clone(),
                    fields: fields.collect(),
                    stream_done: v.stream_done,
                };
                (k.clone(), class)
            })
            .collect();
        // Recursively check which named types are SAP-parsable.
        // Types can be recursive (e.g. `class Tree { children: Tree[] }`), so we
        // track which names are currently being checked. If we encounter a name
        // already on the stack, we optimistically treat it as parsable — the
        // recursion itself is fine, only structurally unparsable leaves cause a
        // type to be non-parsable.
        let mut sap_parseable: HashMap<DefKey, bool> = HashMap::new();
        let all_names: Vec<DefKey> = class_definitions
            .keys()
            .chain(enum_definitions.keys())
            .chain(type_alias_definitions.keys())
            .cloned()
            .collect();

        let mut checking = HashSet::new();
        for name in &all_names {
            check_parseable(
                name,
                &class_definitions,
                &type_alias_definitions,
                &enum_definitions,
                &mut sap_parseable,
                &mut checking,
            );
        }

        Self {
            class_definitions,
            enum_definitions,
            type_alias_definitions,
            sap_parseable,
        }
    }

    pub fn from_sys_op_context<E: Send + Sync + 'static>(
        ctx: &::sys_types::SysOpContext<E>,
    ) -> Self {
        let type_alias_definitions = ctx
            .type_alias_definitions
            .iter()
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect();

        Self::new(
            &ctx.class_definitions,
            ctx.enum_definitions.clone(),
            &type_alias_definitions,
        )
    }

    /// Normalize a runtime-materialized parse target before converting it to
    /// the SAP model. Generic substitution happens after [`TypeCtx::new`] has
    /// simplified the declared class and alias definitions, so it can create a
    /// fresh nested union such as `(string | int) | ToolCalls` at runtime.
    pub(crate) fn normalize_parse_target(&self, ty: SapTy) -> SapTy {
        let recursive_aliases = self.type_alias_definitions.keys().cloned().collect();
        ::baml_type::simplify_sap::simplify_parse_target(
            ty,
            &self.type_alias_definitions,
            &recursive_aliases,
        )
    }

    /// Constructs a full [`TypeRefDb`] from the given context with all types converted.
    pub fn build_db(&self) -> Result<TypeRefDb<'_, DefKey>, ConvertError> {
        fn add<'a>(
            types: &mut IndexMap<DefKey, TyResolved<'a, DefKey>>,
            name: &DefKey,
            ty: TyResolved<'a, DefKey>,
        ) -> Result<(), ConvertError> {
            match types.entry(name.clone()) {
                indexmap::map::Entry::Occupied(_) => {
                    Err(ConvertError::AlreadyPresent(name.clone()))
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
                name,
                TyResolved::Class(self.convert_class(name, cls)?),
            )?;
        }
        for (name, enum_def) in &*self.enum_definitions {
            add(
                &mut types,
                name,
                TyResolved::Enum(Self::convert_enum(name, enum_def)),
            )?;
        }
        for (name, alias_ty) in &self.type_alias_definitions {
            if self.sap_parseable.get(name).is_some_and(|v| !v) {
                continue;
            }
            add(
                &mut types,
                name,
                self.convert_type_alias(name, alias_ty, 0)?,
            )?;
        }
        Ok(TypeRefDb::from_types(types))
    }

    fn convert_class<'a>(
        &'a self,
        name: &DefKey,
        class_def: &'a ::sys_types::ClassDefinition,
    ) -> Result<ClassTy<'a, DefKey>, ConvertError> {
        let fields = class_def
            .fields
            .iter()
            .filter(|field| !field.skip)
            .map(|field| {
                let ::sys_types::ClassFieldDefinition {
                    name,
                    field_type,
                    alias,
                    stream_done,
                    must_exist,
                    ..
                } = field;
                Ok(AnnotatedField {
                    name: Cow::Borrowed(name),
                    ty: self.convert_ty(field_type)?,
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
    ) -> EnumTy<'a, DefKey> {
        let variants = enum_def
            .variants
            .iter()
            .map(|variant| AnnotatedEnumVariant {
                name: variant.name.as_str().into(),
                aliases: variant.alias.iter().map(|a| a.as_str().into()).collect(),
            })
            .collect();

        EnumTy {
            name: name.clone(),
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
        alias_ty: &'a SapTy,
        recursion_depth: usize,
    ) -> Result<TyResolved<'a, DefKey>, ConvertError> {
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
                if inner_name == *name {
                    return Err(ConvertError::RecursionDepthExceeded(
                        "type alias due to self-reference without indirection",
                    ));
                }
                if let Some(class_ty) = self.class_definitions.get(&inner_name) {
                    return self
                        .convert_class(&inner_name, class_ty)
                        .map(TyResolved::Class);
                }
                if let Some(enum_ty) = self.enum_definitions.get(&inner_name) {
                    return Ok(TyResolved::Enum(Self::convert_enum(&inner_name, enum_ty)));
                }
                if let Some(alias_ty) = self.type_alias_definitions.get(&inner_name) {
                    return self.convert_type_alias(&inner_name, alias_ty, recursion_depth + 1);
                }
                return Err(ConvertError::UnknownName(inner_name));
            }
        };
        Ok(resolved)
    }

    /// Converts a BAML type into a sap model type.
    pub fn convert_ty<'a>(&'a self, ty: &'a SapTy) -> Result<Ty<'a, DefKey>, ConvertError> {
        let ty = match ty {
            SapTy::Int => Ty::Resolved(TyResolved::Int(IntTy)),
            SapTy::Bigint => Ty::Resolved(TyResolved::Bigint(BigintTy)),
            SapTy::Float => Ty::Resolved(TyResolved::Float(FloatTy)),
            SapTy::String => Ty::Resolved(TyResolved::String(StringTy)),
            SapTy::Bool => Ty::Resolved(TyResolved::Bool(BoolTy)),
            SapTy::Null => Ty::Resolved(TyResolved::Null(NullTy)),
            SapTy::Media(media_kind) => {
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
            SapTy::Literal(baml_type::Literal::Int(i), _) => {
                Ty::Resolved(TyResolved::LiteralInt(IntLiteralTy(*i)))
            }
            SapTy::Literal(baml_type::Literal::Bigint(bi), _) => {
                Ty::Resolved(TyResolved::LiteralBigint(BigintLiteralTy(bi.clone())))
            }
            SapTy::Literal(baml_type::Literal::Float(..), ..) => {
                return Err(ConvertError::FloatLiteral);
            }
            SapTy::Literal(baml_type::Literal::String(s), _) => {
                Ty::Resolved(TyResolved::LiteralString(StringLiteralTy(Cow::Borrowed(s))))
            }
            SapTy::Literal(baml_type::Literal::Bool(b), _) => {
                Ty::Resolved(TyResolved::LiteralBool(BoolLiteralTy(*b)))
            }
            SapTy::Class(type_name, _) | SapTy::Interface(type_name, _, _) => {
                if self.sap_parseable.get(type_name).is_some_and(|v| !v) {
                    return Err(ConvertError::NonParsableType(Box::new(ty.clone())));
                }
                Ty::Unresolved(type_name.clone())
            }
            SapTy::Enum(type_name) => Ty::Unresolved(type_name.clone()),
            SapTy::EnumVariant(type_name, variant) => {
                let enum_def = self
                    .enum_definitions
                    .get(type_name)
                    .ok_or_else(|| ConvertError::UnknownEnum(type_name.clone()))?;
                let variant_def = enum_def
                    .variants
                    .iter()
                    .find(|v| v.name == AsRef::<str>::as_ref(variant))
                    .ok_or(ConvertError::InternalError(
                        "enum variant not found in enum definition",
                    ))?;
                let enum_variant_ty = EnumVariantTy {
                    name: type_name.clone(),
                    value: AnnotatedEnumVariant {
                        name: variant_def.name.as_str().into(),
                        aliases: variant_def
                            .alias
                            .iter()
                            .map(|a| a.as_str().into())
                            .collect(),
                    },
                };
                Ty::Resolved(TyResolved::EnumVariant(enum_variant_ty))
            }
            SapTy::List(ty) => Ty::Resolved(TyResolved::Array(ArrayTy {
                ty: Box::new(self.convert_ty(ty)?),
            })),
            SapTy::Map { key, value } => Ty::Resolved(TyResolved::Map(MapTy {
                key: Box::new(self.convert_ty(key)?),
                value: Box::new(self.convert_ty(value)?),
            })),
            SapTy::Union(items) => {
                if items.iter().any(|ty| self.is_union_like(ty)) {
                    return Err(ConvertError::UnflattenedUnion);
                }
                let variants = items
                    .iter()
                    .map(|ty| self.convert_ty(ty))
                    .collect::<Result<Vec<_>, _>>()?;
                Ty::Resolved(TyResolved::Union(UnionTy { variants }))
            }
            SapTy::TypeAlias(type_name) => {
                if self.sap_parseable.get(type_name).is_some_and(|v| !v) {
                    return Err(ConvertError::NonParsableType(Box::new(ty.clone())));
                }
                // if it hasn't already, we flatten type aliases:
                // with `type A = B; type B = C; class C { ... }`,
                // a type reference `name:A` becomes `name:C`
                let mut innermost_name = type_name;
                loop {
                    let Some(inner_ty) = self.type_alias_definitions.get(innermost_name) else {
                        return Err(ConvertError::UnknownTypeAlias(innermost_name.clone()));
                    };
                    match inner_ty {
                        SapTy::TypeAlias(name) => {
                            if innermost_name == type_name {
                                return Err(ConvertError::DirectRecursiveTypeAlias(
                                    type_name.clone(),
                                ));
                            }
                            innermost_name = name;
                        }
                        SapTy::Class(name, _)
                        | SapTy::Interface(name, _, _)
                        | SapTy::Enum(name) => {
                            innermost_name = name;
                            break;
                        }
                        unnamed => {
                            let _ = unnamed;
                            break;
                        }
                    }
                }
                Ty::Unresolved(innermost_name.clone())
            }
            unparsable @ (SapTy::Uint8Array
            | SapTy::Resource
            | SapTy::PromptAst
            | SapTy::Function { .. }
            | SapTy::Void
            | SapTy::Unknown
            | SapTy::Future(_, _)
            | SapTy::TypeVar(_)
            | SapTy::AssociatedTypeProjection { .. }
            | SapTy::Never
            | SapTy::RustType
            | SapTy::Type) => {
                return Err(ConvertError::NonParsableType(Box::new(unparsable.clone())));
            }
        };
        Ok(ty)
    }

    fn is_union_like(&self, ty: &SapTy) -> bool {
        match ty {
            SapTy::Union(..) => true,
            SapTy::TypeAlias(name, ..) => self
                .type_alias_definitions
                .get(name)
                .is_some_and(|ty| self.is_union_like(ty)),
            _ => false,
        }
    }
}

fn check_parseable(
    name: &DefKey,
    class_definitions: &IndexMap<DefKey, ClassDefinition>,
    type_alias_definitions: &HashMap<DefKey, SapTy>,
    enum_definitions: &IndexMap<DefKey, EnumDefinition>,
    cache: &mut HashMap<DefKey, bool>,
    checking: &mut HashSet<DefKey>,
) -> bool {
    if let Some(&result) = cache.get(name) {
        return result;
    }
    // Recursive type — assume parsable to break the cycle.
    if !checking.insert(name.clone()) {
        return true;
    }

    let result = if let Some(class_def) = class_definitions.get(name) {
        // A class is parsable if all its non-skipped fields are parsable.
        class_def.fields.iter().filter(|f| !f.skip).all(|field| {
            match is_sap_parseable(&field.field_type) {
                Err(()) => false,
                Ok(deps) => deps.iter().all(|dep| {
                    check_parseable(
                        dep,
                        class_definitions,
                        type_alias_definitions,
                        enum_definitions,
                        cache,
                        checking,
                    )
                }),
            }
        })
    } else if enum_definitions.contains_key(name) {
        // Enums are always parsable.
        true
    } else if let Some(alias_ty) = type_alias_definitions.get(name) {
        match is_sap_parseable(alias_ty) {
            Err(()) => false,
            Ok(deps) => deps.iter().all(|dep| {
                check_parseable(
                    dep,
                    class_definitions,
                    type_alias_definitions,
                    enum_definitions,
                    cache,
                    checking,
                )
            }),
        }
    } else {
        // Unknown name — not parsable.
        false
    };

    checking.remove(name);
    cache.insert(name.clone(), result);
    result
}

fn is_sap_parseable(ty: &SapTy) -> Result<Vec<DefKey>, ()> {
    match ty {
        SapTy::Int
        | SapTy::Bigint
        | SapTy::Float
        | SapTy::String
        | SapTy::Bool
        | SapTy::Null
        | SapTy::Literal(..) => Ok(Vec::new()),
        SapTy::Uint8Array | SapTy::Media(..) => Err(()),
        SapTy::Class(name, _) | SapTy::Interface(name, _, _) => Ok(vec![name.clone()]),
        SapTy::Enum(..) | SapTy::EnumVariant(..) => Ok(Vec::new()),
        SapTy::List(inner) => is_sap_parseable(inner),
        SapTy::Map { key, value, .. } => {
            let keys = is_sap_parseable(key)?;
            let values = is_sap_parseable(value)?;
            Ok(keys.into_iter().chain(values).collect())
        }
        SapTy::Union(members) => {
            let mut names = Vec::new();
            for member in members {
                names.extend(is_sap_parseable(member)?);
            }
            Ok(names)
        }
        SapTy::TypeAlias(name) => Ok(vec![name.clone()]),
        SapTy::Resource
        | SapTy::PromptAst
        | SapTy::Function { .. }
        | SapTy::Void
        | SapTy::Unknown
        | SapTy::Future(..)
        | SapTy::TypeVar(..)
        | SapTy::AssociatedTypeProjection { .. }
        | SapTy::Never
        | SapTy::RustType
        | SapTy::Type => Err(()),
    }
}
