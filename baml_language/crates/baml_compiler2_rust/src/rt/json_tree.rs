//! Parsed JSON values use the same growing list/map layouts as BAML literals.
use super::*;
use baml_compiler2_mir::TyTemplate;
fn handles(name: &str) -> bool {
    matches!(
        name,
        "baml.json.parse"
            | "baml.json._path"
            | "baml.json._to"
            | "baml.json.to"
            | "baml.json.field"
    )
}
impl Plan<'_> {
    pub(super) fn json_alias(&self) -> Gen<RuntimeTy> {
        self.heads
            .values()
            .find(|h| {
                Some(h.root()) == self.lang.get(baml_base::LangPackage::Baml)
                    && h.namespace().as_slice() == ["json"]
                    && h.name().as_str() == "json"
            })
            .cloned()
            .map(RuntimeTy::TypeAlias)
            .ok_or_else(|| no("JSON alias head"))
    }
    fn tree_reprs(&self) -> Gen<(Repr, Repr)> {
        let alias = self.json_alias()?;
        Ok((
            self.repr(&RuntimeTy::List(Box::new(alias.clone())))?,
            self.repr(&RuntimeTy::Map {
                key: Box::new(RuntimeTy::String),
                value: Box::new(alias),
            })?,
        ))
    }
    fn path_error(&self) -> Gen<usize> {
        self.program
            .packages
            .iter()
            .find_map(|p| {
                p.classes.iter().find_map(|(path, id)| {
                    (path.under(&p.name).to_string() == "baml.json.PathError").then_some(id.raw())
                })
            })
            .ok_or_else(|| no("JSON PathError class"))
    }
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            has_trace: false,
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *f)?;
        let name = plan.index.display(id).to_owned();
        handles(&name).then(|| {
            prepare(plan)?;
            plan.json_public_to |= name == "baml.json.to";
            if name == "baml.json._path" {
                plan.add(plan.path_error()?)?;
            }
            Ok(registry::Reach::default())
        })
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            ntypeargs,
            destination,
            ..
        } = term
        else {
            return None;
        };
        let (id, _) = e.plan.index.function(e.db, *f)?;
        let name = e.plan.index.display(id);
        if !handles(name) {
            return None;
        }
        Some((|| {
            let value = e.coerce(
                e.operand(&args[*ntypeargs])?,
                if name == "baml.json.parse" {
                    Repr::Str
                } else {
                    Repr::Tagged
                },
            )?;
            let expression = match name {
                "baml.json.parse" => format!(
                    "rt::json_tree::parse(&mut rt::strings::Cx::current(),&JSON_CONFIG,rt::strings::Str::from_raw({value}))"
                ),
                "baml.json._path" => {
                    let selector = e.coerce(e.operand(&args[1])?, Repr::Str)?;
                    format!(
                        "rt::json_tree::path(&mut rt::strings::Cx::current(),&JSON_CONFIG,{value},rt::strings::Str::from_raw({selector}))"
                    )
                }
                "baml.json.field" => {
                    let key = e.coerce(e.operand(&args[1])?, Repr::Str)?;
                    return e.assign(destination,(format!("rt::json_tree::field(&rt::strings::Cx::current(),&JSON_CONFIG,{value},rt::strings::Str::from_raw({key}))"),Repr::Tagged));
                }
                "baml.json._to" | "baml.json.to" => {
                    let ty = super::generics::type_operand(e.body, &args[0])?;
                    let template = TyTemplate::from(
                        baml_compiler2_mir::RealizedTy::try_from(ty)
                            .map_err(|_| no("non-concrete JSON to"))?,
                    );
                    let index = e
                        .plan
                        .json_to_types
                        .iter()
                        .position(|t| *t == template)
                        .ok_or_else(|| no("unregistered JSON to schema"))?;
                    if name == "baml.json.to" {
                        let missing = matches!(
                            args.get(*ntypeargs + 1),
                            None | Some(Operand::Constant(Constant::OmittedArg))
                        );
                        let naming = if missing {
                            "0usize".to_owned()
                        } else {
                            e.coerce(e.operand(&args[*ntypeargs + 1])?, Repr::Naming)?
                        };
                        format!(
                            "rt_to_public_{index}({},{value},{naming},{missing})",
                            e.telemetry_site(id)
                        )
                    } else {
                        let aliased = e.coerce(e.operand(&args[*ntypeargs + 1])?, Repr::Bool)?;
                        format!("rt_to_{index}({value},{aliased})")
                    }
                }
                _ => unreachable!(),
            };
            e.assign(destination, (e.fallible(&expression), Repr::Tagged))
        })())
    }
}
pub(super) fn register_to(
    plan: &mut Plan<'_>,
    body: &MirFunctionBody<'_>,
    args: &[Operand<'_>],
) -> Gen<()> {
    let ty = super::generics::type_operand(body, &args[0])?;
    plan.record_ty(&ty)?;
    let template = TyTemplate::from(
        baml_compiler2_mir::RealizedTy::try_from(ty).map_err(|_| no("non-concrete JSON to"))?,
    );
    super::json_lists::record_schema(plan, &template)?;
    if !plan.json_to_types.contains(&template) {
        plan.json_to_types.push(template);
    }
    Ok(())
}
pub(super) fn emit(
    out: &mut String,
    cx: &ProgramEmission<'_, '_>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<()> {
    let plan = cx.plan;
    let descriptors = cx.descriptors;
    if !plan.json_tree {
        return Ok(());
    }
    let (list, map) = plan.tree_reprs()?;
    let error = plan
        .path_error()
        .ok()
        .and_then(|id| descriptors.get(&id))
        .copied()
        .unwrap_or(0);
    writeln!(out,"static JSON_CONFIG:rt::json_tree::Config=rt::json_tree::Config{{list:{},map:{},bigint:{},path_error:{error},shapes:JSON_LIST_SHAPES}};",lists[&list],lists[&map],plan.bigint_descriptor()).unwrap();
    let to = plan
        .program
        .objects
        .iter()
        .enumerate()
        .find_map(|(i, o)| {
            (matches!(o, Object::Function(_)) && plan.index.display(i) == "baml.json.to")
                .then_some(i)
        })
        .ok_or_else(|| no("JSON to function"))?;
    let alias = plan
        .program
        .objects
        .iter()
        .enumerate()
        .find_map(|(i, o)| {
            (matches!(o, Object::Function(_)) && plan.index.display(i) == "baml.json._aliased")
                .then_some(i)
        })
        .ok_or_else(|| no("JSON naming function"))?;
    if plan.json_public_to {
        let MirFunctionKind::Bytecode(body) = &cx.all[&to].kind else {
            unreachable!()
        };
        let block=body.blocks.iter().find(|b|matches!(&b.terminator,Some(Terminator::Call{callee:Operand::Constant(Constant::Function(f)),..})if plan.index.function(cx.db,*f).is_some_and(|(id,_)|plan.index.display(id)=="baml.json._to"))).ok_or_else(||no("JSON to boundary"))?;
        let positions = plan.index.positions(to);
        let instruction = positions.block_pcs[block.id.0].terminator.unwrap();
        let pc = positions
            .lower_to_compact_with_offsets()
            .offset_of(instruction)
            .unwrap();
        let line = positions.source_line_for_pc(instruction);
        writeln!(out,"static O{to}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);static S_JSON_TO:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{line}],pcs:&[{pc}],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};",plan.index.info(to).source_file,plan.index.display(to)).unwrap();
    }
    for (i, ty) in plan.json_to_types.iter().enumerate() {
        let code = 3_000_000 + i;
        for aliased in [false, true] {
            super::json_lists::emit(out, plan, descriptors, lists, Some(ty), &[], code, aliased)?;
        }
        writeln!(out,"unsafe fn rt_to_{i}(value:rt::Dyn,aliased:bool)->R<rt::Dyn>{{unsafe{{rt::json_tree::convert(&mut rt::strings::Cx::current(),&JSON_CONFIG,value,if aliased{{&JSON_LIST_ROOT_{code}_1}}else{{&JSON_LIST_ROOT_{code}_0}})}}}}").unwrap();
        if plan.json_public_to {
            let capture = capture_codegen::capture_type(plan, &ty.substitute_symbolic(&[]))?;
            writeln!(out,"unsafe fn rt_to_public_{i}(site:usize,value:rt::Dyn,naming:usize,omitted:bool)->R<rt::Dyn>{{unsafe{{rt::telemetry::observed_captured(&O{to},site,||{{let mut c=rt::telemetry::capture_arguments(&[value,rt::Dyn{{tag:if omitted{{u64::MAX}}else{{CAPTURE_NAMING_TAG}},payload:naming as u64}}]);c.types=rt::telemetry::capture_type_args(&[(\"T\",&{capture})]);c}},|r|match r{{Ok(v)=>rt::telemetry::capture_value(*v),Err(e)=>rt::telemetry::capture_error(e)}},||{{rt::check_stack().map_err(|e|e.at(&S_JSON_TO,0))?;let aliased=rt::telemetry::observed_captured(&O{alias},JSON_TO_ALIAS_SITE,||rt::telemetry::capture_arguments(&[rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:naming as u64}}]),|r|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:1,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||Ok(naming!=0))?;rt_to_{i}(value,aliased).map_err(|e|e.at(&S_JSON_TO,0))}})}}}}").unwrap();
        }
    }
    Ok(())
}

pub(super) fn prepare(plan: &mut Plan<'_>) -> Gen<()> {
    plan.json_tree = true;
    plan.list_stringify = true;
    plan.bigints = true;
    let (list, map) = plan.tree_reprs()?;
    plan.record(list)?;
    plan.record(map)
}
