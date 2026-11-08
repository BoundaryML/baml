//! Native Rng methods, including closed-world interface targets.
use super::*;
pub(super) struct Feature;
pub(super) fn supports(name: &str) -> bool {
    name.starts_with("baml.random.")
        && matches!(
            name.rsplit('.').next(),
            Some("_new" | "random" | "random_int")
        )
}
impl registry::Feature for Feature {
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            destination,
            ..
        } = term
        else {
            return None;
        };
        let (id, _) = e.plan.index.function(e.db, *f)?;
        let name = e.plan.index.builtin_path(id);
        if !supports(name) {
            return None;
        }
        Some((|| {
            let values = args.iter().map(|a| e.operand(a)).collect::<Gen<Vec<_>>>()?;
            let value = self.call(e, name, &[], &values).expect("random sysop")?;
            e.assign(destination, value)
        })())
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !supports(name) {
            return None;
        }
        Some((|| {
            if !types.is_empty() {
                return Err(no("random type arguments"));
            }
            let chacha = name.contains("ChaCha20");
            let system = name.contains("SystemRandom");
            let method = name.rsplit('.').next().unwrap();
            if method == "_new" {
                let seed = e.coerce(
                    values.first().cloned().ok_or_else(|| no("random seed"))?,
                    Repr::Bytes,
                )?;
                let path = if chacha {
                    "random.ChaCha20"
                } else {
                    "random.Xoshiro256PlusPlus"
                };
                let (&c, layout) = e
                    .plan
                    .layouts
                    .iter()
                    .find(|(c, _)| e.plan.class(**c).path.to_string() == path)
                    .ok_or_else(|| no("random result layout"))?;
                if layout.fields != [Repr::Opaque]
                    || e.plan.class(c).fields[0].name.as_str() != "_state"
                {
                    return Err(no("changed random state layout"));
                }
                let state = e.fallible(&format!("rt::random::new({seed},{chacha})"));
                let allocation = e.fallible(&format!(
                    "rt::try_alloc({}, {})",
                    e.descriptors[&c], layout.bytes
                ));
                return Ok((
                    format!(
                        "{{let state={state};let p={allocation} as *mut C{c};baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),state);p as usize}}"
                    ),
                    Repr::Ptr(c),
                ));
            }
            if system {
                let (request, repr) = if method == "random" {
                    let n = e.coerce(
                        values
                            .get(1)
                            .cloned()
                            .ok_or_else(|| no("random byte count"))?,
                        Repr::Int,
                    )?;
                    (format!("rt::random::system_random({n})"), Repr::Bytes)
                } else {
                    ("rt::random::system_random_int()".into(), Repr::Int)
                };
                let owned = e.fallible(&format!("rt::io::perform({request})"));
                let value = if repr == Repr::Bytes {
                    e.fallible("rt::native::bytes(owned.as_array().expect(\"typed random bytes\").iter().map(|v|v.as_u64().expect(\"byte\") as u8).collect())")
                } else {
                    "owned.as_i64().expect(\"typed random int\")".into()
                };
                return Ok((format!("{{let owned={owned};{value}}}"), repr));
            }
            let (_, Repr::Ptr(c)) = values.first().ok_or_else(|| no("random receiver"))? else {
                return Err(no("random receiver layout"));
            };
            let layout = &e.plan.layouts[c];
            if layout.fields != [Repr::Opaque] {
                return Err(no("changed random state layout"));
            }
            let state = format!("(*({} as *const C{c})).f0", values[0].0);
            if method == "random_int" {
                Ok((format!("rt::random::random_int({state})"), Repr::Int))
            } else {
                let n = e.coerce(
                    values
                        .get(1)
                        .cloned()
                        .ok_or_else(|| no("random byte count"))?,
                    Repr::Int,
                )?;
                Ok((
                    e.fallible(&format!("rt::random::random({state},{n})")),
                    Repr::Bytes,
                ))
            }
        })())
    }
}
