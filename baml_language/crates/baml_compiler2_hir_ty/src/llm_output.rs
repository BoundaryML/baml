//! The parts of an LLM function's output a model can never fill.
//!
//! A model answers with JSON, so the runtime drops a return-type union
//! member with no JSON form (`uint8array | string` asks for a `string`), and
//! an optional field of such a type is only ever `null`. The runtime decides
//! this with [`baml_type::json_inhabit`]; these queries run the same analysis
//! on the declared return type, so the compiler can warn where the type is
//! written.
//!
//! Only a return type known at compile time is analyzed. A generic one, or
//! one that reaches `unknown`, an interface, or a declaration served from a
//! package interface, gets no findings: the runtime checks it when it is
//! called.

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_type::{
    DeclName, RuntimeTy, Ty,
    json_inhabit::{
        JsonClass, JsonDefs, JsonField, JsonInhabitance, JsonNaming, JsonStatus, JsonVariant,
    },
};
use rustc_hash::{FxHashMap, FxHashSet};

/// What a model can never return from one LLM function.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LlmOutputFindings {
    /// Each member of the return type the model can never return, and
    /// whether that is because it has no values at all (rather than no JSON
    /// form). The whole return type when nothing in it can be returned.
    pub dropped: Vec<(Ty, bool)>,
    /// What the model can return instead: the return type without the
    /// dropped members. `None` when it can return nothing.
    pub kept: Option<Ty>,
    /// The optional fields, in classes the return type reaches, that the
    /// model can never fill, so they are always `null`: each class's name and
    /// the field's name. `@skip` fields are not among them.
    pub always_null: Vec<(DeclName, Name)>,
}

/// The findings for `function`, or none when it is not an LLM function or its
/// return type is not known at compile time.
#[salsa::tracked(returns(ref))]
pub fn llm_output_findings<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> LlmOutputFindings {
    if baml_compiler2_hir::item_data::function_llm_meta(db, function).is_none() {
        return LlmOutputFindings::default();
    }
    let ret = &crate::lower::function_signature(db, function).ret;
    let Ok(root) = RuntimeTy::<DeclName>::try_from(ret) else {
        return LlmOutputFindings::default();
    };
    let defs = CompilerDefs { db };
    let Ok(json) = JsonInhabitance::of(&root, &defs, JsonNaming::Aliased) else {
        return LlmOutputFindings::default();
    };
    let walk = Walk {
        json: &json,
        defs: &defs,
    };
    let mut findings = LlmOutputFindings::default();
    if json.status(&root) == JsonStatus::Json {
        let kept = walk.kept(&root, &mut findings.dropped, &mut FxHashSet::default());
        findings.kept = Some(Ty::from(&kept));
        walk.always_null(&root, &mut findings.always_null, &mut FxHashSet::default());
    } else {
        findings
            .dropped
            .push((ret.clone(), json.status(&root) == JsonStatus::Empty));
    }
    findings
}

/// The fields every LLM function in the package `root` leaves always `null`
/// ([`LlmOutputFindings::always_null`]), so a class's own file can warn on
/// them.
#[salsa::tracked(returns(ref))]
pub fn package_always_null_fields(
    db: &dyn baml_compiler2_hir::Db,
    root: SourceRoot,
) -> Vec<(DeclName, Name)> {
    let mut fields = Vec::new();
    for &file in root.files(db) {
        for &function in baml_compiler2_hir::item_data::file_functions(db, file) {
            fields.extend(
                llm_output_findings(db, function)
                    .always_null
                    .iter()
                    .cloned(),
            );
        }
    }
    fields.sort();
    fields.dedup();
    fields
}

/// The compiler's declarations as the JSON analysis reads them. A
/// declaration served from a package interface has no `@skip` information,
/// so it stops the analysis, as do the types whose JSON form only the
/// runtime decides.
struct CompilerDefs<'db> {
    db: &'db dyn baml_compiler2_hir::Db,
}

impl JsonDefs<DeclName> for CompilerDefs<'_> {
    type Error = ();

    fn class(
        &self,
        head: &DeclName,
        args: &[RuntimeTy<DeclName>],
    ) -> Result<JsonClass<DeclName>, ()> {
        let DeclRef::Source(class) = crate::layout::class_ref_of(self.db, head).ok_or(())? else {
            return Err(());
        };
        let bindings: FxHashMap<_, Ty> = crate::lower::class_generic_frame(self.db, class)
            .into_iter()
            .zip(args.iter().map(Ty::from))
            .collect();
        let fields = crate::lower::resolve_class_fields(self.db, class)
            .iter()
            .map(|(name, ty, attrs)| {
                let ty = baml_type::unify::substitute_ty(ty, &bindings);
                Ok(JsonField {
                    name: name.to_string(),
                    alias: attrs.schema.alias.clone(),
                    skip: attrs.skip,
                    ty: RuntimeTy::try_from(&ty).map_err(|_| ())?,
                })
            })
            .collect::<Result<_, ()>>()?;
        Ok(JsonClass { fields })
    }

    fn variants(&self, head: &DeclName) -> Result<Vec<JsonVariant>, ()> {
        let DeclRef::Source(enum_loc) = crate::layout::enum_ref_of(self.db, head).ok_or(())? else {
            return Err(());
        };
        Ok(baml_compiler2_hir::item_data::enum_data(self.db, enum_loc)
            .variants
            .iter()
            .map(|variant| JsonVariant {
                name: variant.name.to_string(),
                skip: variant.attrs.skip,
            })
            .collect())
    }

    fn alias_target(&self, head: &DeclName) -> Result<RuntimeTy<DeclName>, ()> {
        let alias = crate::layout::alias_ref_of(self.db, head).ok_or(())?;
        RuntimeTy::try_from(&crate::layout::alias_value(self.db, alias)).map_err(|_| ())
    }

    fn opaque(&self, ty: &RuntimeTy<DeclName>) -> Result<JsonStatus, ()> {
        match ty {
            RuntimeTy::Media(_) => Ok(JsonStatus::Json),
            _ => Err(()),
        }
    }
}

struct Walk<'a, 'db> {
    json: &'a JsonInhabitance<DeclName>,
    defs: &'a CompilerDefs<'db>,
}

impl Walk<'_, '_> {
    /// `ty`, a `Json` type, without the members a model can never return,
    /// which go to `dropped`: each union at the top, under a list or map
    /// there, or behind an alias. An alias with nothing dropped keeps its
    /// name.
    fn kept(
        &self,
        ty: &RuntimeTy<DeclName>,
        dropped: &mut Vec<(Ty, bool)>,
        aliases: &mut FxHashSet<DeclName>,
    ) -> RuntimeTy<DeclName> {
        match ty {
            RuntimeTy::Union(members) => {
                let mut flat = Vec::new();
                baml_type::json_inhabit::flatten_union(members, &mut flat);
                let mut kept = Vec::new();
                for member in flat {
                    match self.json.status(member) {
                        JsonStatus::Json => kept.push(self.kept(member, dropped, aliases)),
                        status => dropped.push((Ty::from(member), status == JsonStatus::Empty)),
                    }
                }
                if kept.len() == 1 {
                    kept.remove(0)
                } else {
                    RuntimeTy::Union(kept.into())
                }
            }
            RuntimeTy::List(inner) if self.json.status(inner) == JsonStatus::Json => {
                RuntimeTy::List(Box::new(self.kept(inner, dropped, aliases)))
            }
            RuntimeTy::Map { key, value } if self.json.status(value) == JsonStatus::Json => {
                RuntimeTy::Map {
                    key: key.clone(),
                    value: Box::new(self.kept(value, dropped, aliases)),
                }
            }
            RuntimeTy::TypeAlias(head) if aliases.insert(head.clone()) => {
                let before = dropped.len();
                let kept = self
                    .defs
                    .alias_target(head)
                    .map(|target| self.kept(&target, dropped, aliases));
                aliases.remove(head);
                match kept {
                    Ok(kept) if dropped.len() > before => kept,
                    _ => ty.clone(),
                }
            }
            _ => ty.clone(),
        }
    }

    /// Records in `out` each optional field the model can never fill, in
    /// the classes the `Json` parts of `ty` reach. A generic class's field is
    /// left out: whether it can be filled depends on the arguments.
    fn always_null(
        &self,
        ty: &RuntimeTy<DeclName>,
        out: &mut Vec<(DeclName, Name)>,
        seen: &mut FxHashSet<RuntimeTy<DeclName>>,
    ) {
        if self.json.status(ty) != JsonStatus::Json {
            return;
        }
        match ty {
            RuntimeTy::Union(members) => {
                for member in members {
                    self.always_null(member, out, seen);
                }
            }
            RuntimeTy::List(inner) | RuntimeTy::Map { value: inner, .. } => {
                self.always_null(inner, out, seen);
            }
            RuntimeTy::TypeAlias(head) => {
                if seen.insert(ty.clone())
                    && let Ok(target) = self.defs.alias_target(head)
                {
                    self.always_null(&target, out, seen);
                }
            }
            RuntimeTy::Class(head, args) => {
                if !seen.insert(ty.clone()) {
                    return;
                }
                let Ok(class) = self.defs.class(head, args) else {
                    return;
                };
                for field in class.fields.iter().filter(|field| !field.skip) {
                    if args.is_empty() && self.only_null(&field.ty, &mut FxHashSet::default()) {
                        out.push((head.clone(), Name::new(&field.name)));
                    }
                    self.always_null(&field.ty, out, seen);
                }
            }
            _ => {}
        }
    }

    /// Whether `ty` is optional, but holds nothing else a model can fill:
    /// `uint8array?`, or an alias of one. Plain `null` is not: it is only
    /// `null` as written.
    fn only_null(&self, ty: &RuntimeTy<DeclName>, aliases: &mut FxHashSet<DeclName>) -> bool {
        match ty {
            RuntimeTy::Union(members) => {
                let mut flat = Vec::new();
                baml_type::json_inhabit::flatten_union(members, &mut flat);
                flat.iter().any(|member| member.is_null())
                    && flat.iter().any(|member| !member.is_null())
                    && flat
                        .iter()
                        .filter(|member| !member.is_null())
                        .all(|member| self.json.status(member) != JsonStatus::Json)
            }
            RuntimeTy::TypeAlias(head) if aliases.insert(head.clone()) => self
                .defs
                .alias_target(head)
                .is_ok_and(|target| self.only_null(&target, aliases)),
            _ => false,
        }
    }
}
