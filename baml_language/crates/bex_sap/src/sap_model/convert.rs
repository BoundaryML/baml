//! Converting from [`baml_type`] types to SAP model types.

use std::borrow::Cow;

use ::baml_type::TyAttrValue;
use ::std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use ::sys_types::{ClassDefinition, DefKey, EnumDefinition, SapTy};
use indexmap::IndexMap;

use crate::sap_model::{
    self, AnnotatedEnumVariant, AnnotatedField, AnnotatedTy, ArrayTy, AttrLiteral, BigintLiteralTy,
    BigintTy, BoolLiteralTy, BoolTy, ClassTy, EnumTy, EnumVariantTy, FloatTy, IntLiteralTy, IntTy,
    MapTy, MediaTy, NullTy, StringLiteralTy, StringTy, TyResolved, TyWithMeta, TypeAnnotations,
    TypeRefDb, UnionTy,
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
        let mut db = TypeRefDb::new();
        for (name, cls) in &self.class_definitions {
            if self.sap_parseable.get(name).is_some_and(|v| !v) {
                continue;
            }
            let cls = self.convert_class(name, cls)?;
            db.try_add_inner(name.clone(), TyResolved::Class(cls))
                .map_err(|_| ConvertError::AlreadyPresent(name.clone()))?;
        }
        for (name, enum_def) in &*self.enum_definitions {
            let enum_def = Self::convert_enum(name, enum_def);
            db.try_add_inner(name.clone(), TyResolved::Enum(enum_def))
                .map_err(|_| ConvertError::AlreadyPresent(name.clone()))?;
        }
        for (name, alias_ty) in &self.type_alias_definitions {
            if self.sap_parseable.get(name).is_some_and(|v| !v) {
                continue;
            }
            let alias_ty = self.convert_type_alias(name, alias_ty, 0)?;
            db.try_add_inner(name.clone(), alias_ty)
                .map_err(|_| ConvertError::AlreadyPresent(name.clone()))?;
        }
        Ok(db)
    }

    fn convert_class<'a>(
        &'a self,
        name: &DefKey,
        class_def: &'a ::sys_types::ClassDefinition,
    ) -> Result<ClassTy<'a, DefKey>, ConvertError> {
        let fields = class_def
            .fields
            .iter()
            .filter_map(|field| {
                let ::sys_types::ClassFieldDefinition {
                    name,
                    field_type,
                    alias,
                    skip,
                    ..
                } = &field;
                if *skip {
                    return None;
                }
                let ty = match self.convert_ty(field_type) {
                    Ok(ty) => ty,
                    Err(err) => return Some(Err(err)),
                };
                let (class_in_progress_field_missing, class_completed_field_missing) =
                    match self.get_field_attrs(field_type, 0) {
                        Ok(attrs) => attrs,
                        Err(err) => return Some(Err(err)),
                    };

                let field = AnnotatedField {
                    name: Cow::Borrowed(name),
                    ty,
                    class_in_progress_field_missing,
                    class_completed_field_missing,
                    aliases: alias.iter().map(Into::into).collect(),
                };
                Some(Ok(field))
            })
            .collect::<Result<_, _>>()?;
        let class_ty = ClassTy {
            name: name.clone(),
            fields,
        };
        Ok(class_ty)
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

        let converted = self.convert_ty(alias_ty)?;
        let resolved = match converted.ty {
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
    pub fn convert_ty<'a>(
        &'a self,
        ty: &'a SapTy,
    ) -> Result<AnnotatedTy<'a, DefKey>, ConvertError> {
        let ty = match ty {
            SapTy::Int { attr } => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Int(IntTy)),
                convert_ty_attrs(attr),
            ),
            SapTy::Bigint { attr } => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Bigint(BigintTy)),
                convert_ty_attrs(attr),
            ),
            SapTy::Float { attr } => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Float(FloatTy)),
                convert_ty_attrs(attr),
            ),
            SapTy::String { attr } => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::String(StringTy)),
                convert_ty_attrs(attr),
            ),
            SapTy::Bool { attr } => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Bool(BoolTy)),
                convert_ty_attrs(attr),
            ),
            SapTy::Null { attr } => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Null(NullTy)),
                convert_ty_attrs(attr),
            ),
            SapTy::Media(media_kind, ty_attr) => {
                let media_kind = match media_kind {
                    baml_type::MediaKind::Image => MediaTy::Image,
                    baml_type::MediaKind::Video => MediaTy::Video,
                    baml_type::MediaKind::Audio => MediaTy::Audio,
                    baml_type::MediaKind::Pdf => MediaTy::Pdf,
                    baml_type::MediaKind::Generic => {
                        return Err(ConvertError::UnknownMediaKind);
                    }
                };
                TyWithMeta::new(
                    sap_model::Ty::Resolved(TyResolved::Media(media_kind)),
                    convert_ty_attrs(ty_attr),
                )
            }
            SapTy::Literal(baml_type::Literal::Int(i), _, attr) => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::LiteralInt(IntLiteralTy(*i))),
                convert_ty_attrs(attr),
            ),
            SapTy::Literal(baml_type::Literal::Bigint(bi), _, attr) => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::LiteralBigint(BigintLiteralTy(bi.clone()))),
                convert_ty_attrs(attr),
            ),
            SapTy::Literal(baml_type::Literal::Float(..), ..) => {
                return Err(ConvertError::FloatLiteral);
            }
            SapTy::Literal(baml_type::Literal::String(s), _, attr) => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::LiteralString(StringLiteralTy(Cow::Borrowed(
                    s,
                )))),
                convert_ty_attrs(attr),
            ),
            SapTy::Literal(baml_type::Literal::Bool(b), _, attr) => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::LiteralBool(BoolLiteralTy(*b))),
                convert_ty_attrs(attr),
            ),
            SapTy::Class(type_name, _, attr) | SapTy::Interface(type_name, _, _, attr) => {
                if self.sap_parseable.get(type_name).is_some_and(|v| !v) {
                    return Err(ConvertError::NonParsableType(Box::new(ty.clone())));
                }
                TyWithMeta::new(
                    // currently [`ClassDefinition`] does not have attributes attached to it.
                    // They will probably get lifted earlier in the conversion process, but if not then we would do it here.
                    sap_model::Ty::Unresolved(type_name.clone()),
                    convert_ty_attrs(attr),
                )
            }
            SapTy::Enum(type_name, attr) => TyWithMeta::new(
                sap_model::Ty::Unresolved(type_name.clone()),
                convert_ty_attrs(attr),
            ),
            SapTy::EnumVariant(type_name, variant, attr) => {
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
                TyWithMeta::new(
                    sap_model::Ty::Resolved(TyResolved::EnumVariant(enum_variant_ty)),
                    convert_ty_attrs(attr),
                )
            }
            SapTy::List(ty, attr) => TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Array(ArrayTy {
                    ty: Box::new(self.convert_ty(ty)?),
                })),
                convert_ty_attrs(attr),
            ),
            SapTy::Map { key, value, attr } => {
                let key = self.convert_ty(key)?;
                let value = self.convert_ty(value)?;
                TyWithMeta::new(
                    sap_model::Ty::Resolved(TyResolved::Map(MapTy {
                        key: Box::new(key),
                        value: Box::new(value),
                    })),
                    convert_ty_attrs(attr),
                )
            }
            SapTy::Union(items, ty_attr) => {
                if items.iter().any(|ty| self.is_union_like(ty)) {
                    return Err(ConvertError::UnflattenedUnion);
                }
                let items = items
                    .iter()
                    .map(|ty| self.convert_ty(ty))
                    .collect::<Result<Vec<_>, _>>()?;
                TyWithMeta::new(
                    sap_model::Ty::Resolved(TyResolved::Union(UnionTy { variants: items })),
                    convert_ty_attrs(ty_attr),
                )
            }
            SapTy::TypeAlias(type_name, attr) => {
                if self.sap_parseable.get(type_name).is_some_and(|v| !v) {
                    return Err(ConvertError::NonParsableType(Box::new(ty.clone())));
                }
                // if it hasn't already, we flatten type aliases:
                // with `type A = B; type B = C; class C { ... }`,
                // a type reference `name:A` becomes `name:C`
                let mut attr = attr.clone();
                let mut innermost_name = type_name;
                loop {
                    let Some(inner_ty) = self.type_alias_definitions.get(innermost_name) else {
                        return Err(ConvertError::UnknownTypeAlias(innermost_name.clone()));
                    };
                    match inner_ty {
                        SapTy::TypeAlias(name, inner_attr) => {
                            if innermost_name == type_name {
                                return Err(ConvertError::DirectRecursiveTypeAlias(
                                    type_name.clone(),
                                ));
                            }
                            attr = merge_ty_attrs(&attr, inner_attr);
                            innermost_name = name;
                        }
                        SapTy::Class(name, _, inner_attr)
                        | SapTy::Interface(name, _, _, inner_attr)
                        | SapTy::Enum(name, inner_attr) => {
                            attr = merge_ty_attrs(&attr, inner_attr);
                            innermost_name = name;
                            break;
                        }
                        unnamed => {
                            attr = merge_ty_attrs(&attr, unnamed.attr());
                            break;
                        }
                    }
                }

                TyWithMeta::new(
                    sap_model::Ty::Unresolved(innermost_name.clone()),
                    convert_ty_attrs(&attr),
                )
            }
            unparsable @ (SapTy::Uint8Array { .. }
            | SapTy::Resource { .. }
            | SapTy::PromptAst { .. }
            | SapTy::Function { .. }
            | SapTy::Void { .. }
            | SapTy::Unknown { .. }
            | SapTy::Future(_, _, _)
            | SapTy::TypeVar(_, _)
            | SapTy::AssociatedTypeProjection { .. }
            | SapTy::Never { .. }
            | SapTy::RustType { .. }
            | SapTy::Type { .. }) => {
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

    /// Outside of SAP, the SAP field attributes are treated as type attributes (since they can be attached to type declarations).
    /// This function derives the SAP field attributes from the BAML type and attributes.
    /// May need to recurse into named types.
    ///
    /// ## Returns
    /// `(class_in_progress_field_missing, class_completed_field_missing)`
    fn get_field_attrs<'a>(
        &'a self,
        field_type: &'a SapTy,
        recursion_depth: usize,
    ) -> Result<(AttrLiteral<'a, DefKey>, AttrLiteral<'a, DefKey>), ConvertError> {
        if recursion_depth > MAX_RECURSION_DEPTH {
            return Err(ConvertError::RecursionDepthExceeded(
                "class field attribute derivation",
            ));
        }

        let attrs = field_type.attr();
        if matches!(attrs.sap_pending_never, TyAttrValue::Set) {
            return Ok((AttrLiteral::Never, AttrLiteral::Never));
        }

        if self.field_type_is_nullable(field_type)? {
            return Ok((AttrLiteral::Null, AttrLiteral::Null));
        }

        let field_attrs = match field_type {
            SapTy::Int { .. }
            | SapTy::Bigint { .. }
            | SapTy::Float { .. }
            | SapTy::String { .. }
            | SapTy::Bool { .. }
            | SapTy::Uint8Array { .. }
            | SapTy::Media(..)
            | SapTy::Literal(..)
            | SapTy::Class(..)
            | SapTy::Interface(..)
            | SapTy::Enum(..)
            | SapTy::EnumVariant(..) => (AttrLiteral::Never, AttrLiteral::Never),
            SapTy::Null { .. } => {
                unreachable!("nullable fields should be returned before field attr derivation")
            }
            SapTy::List(..) => (
                AttrLiteral::Array(Vec::new()),
                AttrLiteral::Array(Vec::new()),
            ),
            SapTy::Map { .. } => (
                AttrLiteral::Map(IndexMap::new()),
                AttrLiteral::Map(IndexMap::new()),
            ),
            SapTy::Union(members, ..) => members
                .first()
                .map(|first| self.get_field_attrs(first, recursion_depth + 1))
                .transpose()?
                .unwrap_or((AttrLiteral::Never, AttrLiteral::Never)),
            SapTy::TypeAlias(name, ..) => {
                let Some(alias_ty) = self.type_alias_definitions.get(name) else {
                    return Err(ConvertError::UnknownTypeAlias(name.clone()));
                };
                self.get_field_attrs(alias_ty, recursion_depth + 1)?
            }
            unparsable @ (SapTy::Resource { .. }
            | SapTy::PromptAst { .. }
            | SapTy::Function { .. }
            | SapTy::Void { .. }
            | SapTy::Unknown { .. }
            | SapTy::Future(_, _, _)
            | SapTy::TypeVar(_, _)
            | SapTy::AssociatedTypeProjection { .. }
            | SapTy::Never { .. }
            | SapTy::RustType { .. }
            | SapTy::Type { .. }) => {
                return Err(ConvertError::NonParsableType(Box::new(unparsable.clone())));
            }
        };
        Ok(field_attrs)
    }

    fn field_type_is_nullable(&self, field_type: &SapTy) -> Result<bool, ConvertError> {
        self.field_type_is_nullable_inner(field_type, &mut HashSet::new(), 0)
    }

    fn field_type_is_nullable_inner(
        &self,
        field_type: &SapTy,
        aliases_in_progress: &mut HashSet<DefKey>,
        recursion_depth: usize,
    ) -> Result<bool, ConvertError> {
        if recursion_depth > MAX_RECURSION_DEPTH {
            return Err(ConvertError::RecursionDepthExceeded(
                "class field nullability derivation",
            ));
        }

        Ok(match field_type {
            SapTy::Null { .. } => true,
            SapTy::Union(members, ..) => {
                let mut is_nullable = false;
                for member in members {
                    if self.field_type_is_nullable_inner(
                        member,
                        aliases_in_progress,
                        recursion_depth + 1,
                    )? {
                        is_nullable = true;
                        break;
                    }
                }
                is_nullable
            }
            SapTy::TypeAlias(name, ..) => {
                if !aliases_in_progress.insert(name.clone()) {
                    // A cycle by itself does not prove nullability for this branch.
                    false
                } else {
                    let Some(alias_ty) = self.type_alias_definitions.get(name) else {
                        aliases_in_progress.remove(name);
                        return Err(ConvertError::UnknownTypeAlias(name.clone()));
                    };
                    let is_nullable = self.field_type_is_nullable_inner(
                        alias_ty,
                        aliases_in_progress,
                        recursion_depth + 1,
                    )?;
                    aliases_in_progress.remove(name);
                    is_nullable
                }
            }
            _ => false,
        })
    }
}

fn convert_ty_attrs(attrs: &baml_type::TyAttr) -> TypeAnnotations<'static, DefKey> {
    let in_progress = match attrs.sap_pending_never {
        TyAttrValue::Set => Some(AttrLiteral::Never),
        TyAttrValue::Unset => None,
    };
    let parse_without_null = attrs.sap_parse_without_null == TyAttrValue::Set;

    TypeAnnotations {
        in_progress,
        parse_without_null,
    }
}
/// Merges two type attributes.
/// May return one of the inputs if the output would be identical.
fn merge_ty_attrs(outer: &baml_type::TyAttr, inner: &baml_type::TyAttr) -> baml_type::TyAttr {
    baml_type::TyAttr {
        sap_in_progress_never: outer.sap_in_progress_never.or(inner.sap_in_progress_never),
        sap_parse_without_null: outer
            .sap_parse_without_null
            .or(inner.sap_parse_without_null),
        sap_pending_never: outer.sap_pending_never.or(inner.sap_pending_never),
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
        SapTy::Int { .. }
        | SapTy::Bigint { .. }
        | SapTy::Float { .. }
        | SapTy::String { .. }
        | SapTy::Bool { .. }
        | SapTy::Null { .. }
        | SapTy::Literal(..) => Ok(Vec::new()),
        SapTy::Uint8Array { .. } | SapTy::Media(..) => Err(()),
        SapTy::Class(name, _, _) | SapTy::Interface(name, _, _, _) => Ok(vec![name.clone()]),
        SapTy::Enum(..) | SapTy::EnumVariant(..) => Ok(Vec::new()),
        SapTy::List(inner, _) => is_sap_parseable(inner),
        SapTy::Map { key, value, .. } => {
            let keys = is_sap_parseable(key)?;
            let values = is_sap_parseable(value)?;
            Ok(keys.into_iter().chain(values).collect())
        }
        SapTy::Union(members, _) => {
            let mut names = Vec::new();
            for member in members {
                names.extend(is_sap_parseable(member)?);
            }
            Ok(names)
        }
        SapTy::TypeAlias(name, _) => Ok(vec![name.clone()]),
        SapTy::Resource { .. }
        | SapTy::PromptAst { .. }
        | SapTy::Function { .. }
        | SapTy::Void { .. }
        | SapTy::Unknown { .. }
        | SapTy::Future(..)
        | SapTy::TypeVar(..)
        | SapTy::AssociatedTypeProjection { .. }
        | SapTy::Never { .. }
        | SapTy::RustType { .. }
        | SapTy::Type { .. } => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{RuntimeTy, TyAttr};

    use super::*;

    fn local_type(name: &str) -> DefKey {
        DefKey::new(
            ::baml_type::typetag::TypeTag::of_head(name),
            ::baml_type::DeclarationName::Declared(::baml_type::TypeName::local(name.into())),
        )
    }

    #[test]
    fn field_type_is_nullable_handles_recursive_alias_cycle_with_null_branch() {
        let maybe_text = local_type("MaybeText");
        let text_ref = local_type("TextRef");
        let alias_attr = TyAttr::default();

        let type_alias_definitions = HashMap::from([
            (
                maybe_text.clone(),
                RuntimeTy::union([
                    RuntimeTy::TypeAlias(text_ref.clone(), alias_attr.clone()),
                    RuntimeTy::null(),
                ]),
            ),
            (
                text_ref,
                RuntimeTy::TypeAlias(maybe_text.clone(), alias_attr.clone()),
            ),
        ]);

        let ctx = TypeCtx::new(
            &IndexMap::new(),
            Arc::new(IndexMap::new()),
            &type_alias_definitions,
        );

        assert!(
            ctx.field_type_is_nullable(&RuntimeTy::TypeAlias(maybe_text, alias_attr))
                .unwrap()
        );
    }

    #[test]
    fn field_type_is_nullable_errors_on_deep_alias_union_chain() {
        let alias_attr = TyAttr::default();
        let chain_len = MAX_RECURSION_DEPTH + 2;
        let names: Vec<_> = (0..chain_len)
            .map(|idx| local_type(&format!("DepthAlias{idx}")))
            .collect();

        let mut type_alias_definitions = HashMap::new();
        for window in names.windows(2) {
            let current = window[0].clone();
            let next = window[1].clone();
            type_alias_definitions.insert(
                current,
                RuntimeTy::union([
                    RuntimeTy::TypeAlias(next, alias_attr.clone()),
                    RuntimeTy::string(),
                ]),
            );
        }
        type_alias_definitions.insert(
            names.last().cloned().unwrap(),
            RuntimeTy::union([RuntimeTy::string(), RuntimeTy::bool()]),
        );

        let ctx = TypeCtx::new(
            &IndexMap::new(),
            Arc::new(IndexMap::new()),
            &type_alias_definitions,
        );

        assert!(matches!(
            ctx.field_type_is_nullable(&RuntimeTy::TypeAlias(names[0].clone(), alias_attr)),
            Err(ConvertError::RecursionDepthExceeded(
                "class field nullability derivation"
            ))
        ));
    }
}
