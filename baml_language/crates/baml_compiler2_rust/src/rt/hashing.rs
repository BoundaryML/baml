//! `baml.hash.DefaultHasher` natives and the structural `Hash` default.
use super::*;

const NEW: &str = "baml.hash.DefaultHasher.new";
const WRITE: &str = "baml.hash.Hasher$for$DefaultHasher.write";
const FINISH: &str = "baml.hash.Hasher$for$DefaultHasher.finish";
const STRUCTURAL: &str = "baml._hash_structural_default";

pub(super) fn native(name: &str) -> bool {
    matches!(name, NEW | WRITE | FINISH | STRUCTURAL)
}

fn default_hasher(plan: &Plan<'_>) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .filter(|p| p.name == "baml")
        .flat_map(|p| &p.classes)
        .find_map(|(path, id)| (path.to_string() == "hash.DefaultHasher").then_some(id.raw()))
        .ok_or_else(|| no("baml.hash.DefaultHasher"))
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
            callee: Operand::Constant(Constant::Function(function)),
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *function)?;
        native(plan.index.builtin_path(id)).then(|| {
            plan.add(default_hasher(plan)?)?;
            Ok(registry::Reach {
                targets: vec![id],
                ..Default::default()
            })
        })
    }

    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        native(name).then(|| emit(e, name, values))
    }
}

fn emit(e: &Emitter<'_, '_>, name: &str, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
    let class = default_hasher(e.plan)?;
    let layout = &e.plan.layouts[&class];
    if layout.fields != [Repr::Opaque] || e.plan.class(class).fields[0].name.as_str() != "_state" {
        return Err(no("changed DefaultHasher layout"));
    }
    let tag = e.descriptors[&class] + 3;
    let alloc = format!(
        "{{let s=rt::hashing::new_state()?;let p=rt::try_alloc({}, {})? as *mut C{class};baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),s);(p as usize,s)}}",
        e.descriptors[&class], layout.bytes
    );
    Ok(match name {
        NEW => (
            e.fallible(&format!("(||->R<usize>{{unsafe{{Ok({alloc}.0)}}}})()")),
            Repr::Ptr(class),
        ),
        WRITE | FINISH => {
            let receiver = e.coerce(values[0].clone(), Repr::Ptr(class))?;
            let state = format!("(*({receiver} as *const C{class})).f0");
            if name == FINISH {
                (format!("rt::hashing::finish({state})"), Repr::Int)
            } else {
                let bytes = e.coerce(values[1].clone(), Repr::Bytes)?;
                (
                    format!("{{rt::hashing::write({state},rt::bytes::data({bytes}));0usize}}"),
                    Repr::Null,
                )
            }
        }
        _ => {
            reject_overrides(e.plan, class)?;
            let value = e.coerce(values[0].clone(), Repr::Tagged)?;
            let state = e.coerce(values[1].clone(), Repr::Tagged)?;
            let code = format!(
                "{{struct H;impl rt::hashing::Hooks for H{{fn write(&mut self,state:rt::Dyn,bytes:&[u8])->R<()>{{unsafe{{if state.tag=={tag}&&state.payload!=0{{rt::hashing::write((*(state.payload as *const C{class})).f0,bytes);Ok(())}}else{{Err(rt::Error::internal(\"hash state without a Hasher\".into()))}}}}}}fn new_hasher(&mut self)->R<(rt::Dyn,usize)>{{unsafe{{let (p,s)={alloc};Ok((rt::Dyn{{tag:{tag},payload:p as u64}},s))}}}}}}rt::hashing::structural({value},{state},CAPTURE_SHAPES,&mut H)}}"
            );
            (
                format!(
                    "{{{};0usize}}",
                    super::operators::native_fallible(e, name, &code)?
                ),
                Repr::Null,
            )
        }
    })
}

/// The walk dispatches only the native `DefaultHasher` and never a user
/// `Hash.hash`; programs that could need either are rejected.
fn reject_overrides(plan: &Plan<'_>, default: usize) -> Gen<()> {
    let default_tag = plan.class(default).type_tag;
    let mut tags = BTreeSet::new();
    for &class in plan.layouts.keys() {
        tags.insert(plan.class(class).type_tag);
    }
    for &enumeration in &plan.enums {
        if let Object::Enum(en) =
            &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(enumeration)]
        {
            tags.insert(en.type_tag);
        }
    }
    for package in &plan.program.packages {
        for (index, rules) in &package.impl_rules {
            let Object::Interface(def) = &plan.program.objects[*index] else {
                continue;
            };
            let interface = def.path.to_string();
            let hash = def
                .structural_default
                .as_ref()
                .is_some_and(|d| d.kind == baml_type::StructuralInterface::Hash);
            if !hash && interface != "hash.Hasher" {
                continue;
            }
            for rule in rules {
                let head = match &rule.for_ty_pattern {
                    bex_vm_types::TyTemplate::Class(head, _)
                    | bex_vm_types::TyTemplate::Enum(head) => head.tag(),
                    _ => continue,
                };
                if tags.contains(&head)
                    && head != default_tag
                    && (!hash || rule.methods.contains_key("hash"))
                {
                    return Err(no(
                        "structural hashing with a user Hash or Hasher implementation",
                    ));
                }
            }
        }
    }
    Ok(())
}
