//! Compile the linked initializer schedule, never a second dependency analysis.
use super::*;

pub(super) struct Feature;
impl registry::Feature for Feature {}

pub(super) fn prepare<'db>(
    db: &'db dyn Db,
    root: SourceRoot,
    opt: OptLevel,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, MirFunction<'db>>,
) -> Gen<Vec<usize>> {
    for &file in world_files(db, root) {
        for &binding in file_lets(db, file) {
            let Some((mut body, lambdas)) = baml_compiler2_mir::lower_let_body(db, binding, opt)
                .map_err(|e| no(format!("initializer MIR: {e}")))?
            else {
                continue;
            };
            if let Some(ty) = baml_compiler2_hir_ty::globals::global_value_type(db, binding) {
                body.locals[0].ty =
                    RuntimeTy::try_from(&ty).map_err(|_| no("initializer value type"))?;
            }
            let Some(object) = plan.index.let_helper(db, binding) else {
                return Err(no("unlinked initializer helper"));
            };
            all.insert(
                object,
                MirFunction {
                    arity: 0,
                    kind: std::borrow::Cow::Owned(MirFunctionKind::Bytecode(body)),
                    lambdas: std::borrow::Cow::Owned(lambdas),
                },
            );
        }
    }
    let mut roots = Vec::new();
    for &ordinal in &plan.program.init_order {
        let package = &plan.program.packages[ordinal as usize];
        let chain = package
            .init
            .ok_or_else(|| no("missing initializer chain"))?
            .raw();
        let instructions = plan.index.positions(chain).instructions.clone();
        let mut steps = Vec::new();
        for (pc, pair) in instructions.chunks_exact(2).enumerate() {
            use bex_vm_types::{ConstValue, Instruction as I};
            match pair {
                [
                    I::Call {
                        callee,
                        ntypeargs: 0,
                    },
                    I::StoreGlobal(slot),
                ] => {
                    let Some(ConstValue::Object(callee)) = plan.program.globals.get(callee.raw())
                    else {
                        return Err(no("initializer callee"));
                    };
                    let object = callee.raw();
                    let repr = if let Some(mir) = all.get(&object) {
                        let MirFunctionKind::Bytecode(body) = mir.kind.as_ref() else {
                            return Err(no("initializer body"));
                        };
                        roots.push(object);
                        plan.repr(&body.locals[0].ty)?
                    } else {
                        return Err(no("initializer MIR unavailable"));
                    };
                    plan.record(repr)?;
                    plan.globals.insert(slot.raw(), repr);
                    steps.push((object, slot.raw(), pc * 2));
                }
                [I::LoadConst(0), I::Return] => {}
                _ => return Err(no("initializer chain shape")),
            }
        }
        plan.init_chains.push((chain, steps));
    }
    Ok(roots)
}

pub(super) fn emit(
    out: &mut String,
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    sites: &std::cell::RefCell<Vec<(Option<usize>, usize, usize)>>,
) -> Gen<()> {
    if plan.init_chains.is_empty() {
        out.push_str("fn initialize_globals()->R<()>{Ok(())}\n");
        return Ok(());
    }
    for (chain, steps) in &plan.init_chains {
        let bc = plan.index.positions(*chain);
        let lines: Vec<_> = steps
            .iter()
            .map(|(_, _, pc)| bc.source_line_for_pc(*pc))
            .collect();
        let pcs: Vec<_> = steps
            .iter()
            .map(|(_, _, pc)| bc.lower_to_compact_with_offsets().offset_of(*pc).unwrap())
            .collect();
        let kinds = vec!["rt::telemetry::RaiseKind::NativeBoundary"; steps.len()].join(",");
        writeln!(out,"static S{chain}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&{lines:?},pcs:&{pcs:?},kinds:&[{kinds}]}};",plan.index.info(*chain).source_file,plan.index.display(*chain)).unwrap();
    }
    out.push_str("fn initialize_globals()->R<()>{Ok(())}\n");
    let mut startup = String::from(
        "fn initialize_program(instance:&mut rt::Instance,session:&mut rt::telemetry::Session)->R<()>{",
    );
    for (package_index, (chain, steps)) in plan.init_chains.iter().enumerate() {
        let site = sites.borrow().len();
        sites.borrow_mut().push((None, 0, *chain));
        writeln!(out,"fn initialize_chain_{chain}()->R<i64>{{if rt::globals::is_ready(){{return Ok(0);}}unsafe{{").unwrap();
        if package_index == 0 {
            writeln!(out, "rt::globals::begin({})?;", plan.program.globals.len()).unwrap();
        }
        writeln!(out,"rt::telemetry::observed_captured(&O{chain},{site},||rt::telemetry::capture_arguments(&[]),|r:&R<()>|match r{{Ok(_)=>rt::telemetry::capture_value(rt::Dyn{{tag:2,payload:0}}),Err(e)=>rt::telemetry::capture_error(e)}},||{{").unwrap();
        for (step, &(callee, slot, pc)) in steps.iter().enumerate() {
            let repr = plan.globals[&slot];
            let site = sites.borrow().len();
            let pc = plan
                .index
                .positions(*chain)
                .lower_to_compact_with_offsets()
                .offset_of(pc)
                .ok_or_else(|| no("initializer source site"))?;
            sites.borrow_mut().push((Some(*chain), pc, callee));
            let tag = match repr {
                Repr::Int => 0,
                Repr::Float => u32::MAX as usize - 1,
                Repr::Bool => 1,
                Repr::Null => 2,
                Repr::Str => 3,
                Repr::Ptr(c) => descriptors[&c] + 3,
                Repr::Closure => plan.closure_descriptors().unwrap().1 + 3,
                r if r.pointer() => lists[&r] + 3,
                Repr::Tagged | Repr::Dyn(_) => 0,
                _ => return Err(no("global representation")),
            };
            let value = if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
                "value".to_owned()
            } else {
                let payload = if repr == Repr::Float {
                    "value.to_bits()"
                } else {
                    "value as u64"
                };
                let tag = if repr.pointer() {
                    format!("if value==0{{2}}else{{{tag}}}")
                } else {
                    tag.to_string()
                };
                format!("rt::Dyn{{tag:{tag},payload:{payload}}}")
            };
            writeln!(out,"let cx=rt::calls::Context::current();let value=(if rt::telemetry::active(){{f{callee}::<true>(cx,{site})}}else{{f{callee}::<false>(cx,{site})}}).map_err(|e|e.at(&S{chain},{step}))?;rt::globals::store({slot},{value},{});",repr.pointer()).unwrap();
        }
        let package = plan
            .program
            .packages
            .iter()
            .find(|p| p.init.is_some_and(|id| id.raw() == *chain))
            .ok_or_else(|| no("initializer package"))?;
        out.push_str("Ok(())})?;}");
        if package_index + 1 == plan.init_chains.len() {
            out.push_str("rt::globals::complete();");
        }
        out.push_str("Ok(0)}\n");
        writeln!(startup,"session.run_initializer_at({site},||unsafe{{instance.run_baml(initialize_chain_{chain})}}).map_err(|e|rt::globals::init_failed(e,{:?}))?;",package.name).unwrap();
    }
    startup.push_str("Ok(())}\n");
    out.push_str(&startup);
    Ok(())
}
