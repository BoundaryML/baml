//! Match control flow. VM tags and storage tags have separate namespaces.
use super::*;
use baml_compiler2_mir::{SwitchKey, TyTemplate};
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        _: &dyn Db,
        _: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        matches!(
            term,
            Some(
                Terminator::Switch { .. } | Terminator::NarrowBind { .. } | Terminator::Unreachable
            )
        )
        .then(|| Ok(registry::Reach::default()))
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        match term {
            Terminator::Switch { discriminant, .. } => Some((|| {
                let (v, r) = e.operand(discriminant)?;
                if !matches!(r, Repr::Int | Repr::Naming | Repr::Enum(_)) {
                    return Err(no("non-integer switch discriminant"));
                }
                Ok(format!("switch{}=({v}) as i64;", e.block))
            })()),
            Terminator::NarrowBind {
                source,
                test,
                destination,
                ..
            } => Some((|| {
                let value = e.operand(source)?;
                let check = e.membership(value.clone(), &test.template(e.db))?;
                let store = e.assign(&Place::Local(*destination), value)?;
                Ok(format!("c{}={check};if c{}{{{store}}}", e.block, e.block))
            })()),
            Terminator::Unreachable => Some(Ok("unreachable!(\"exhaustive BAML match\");".into())),
            _ => None,
        }
    }
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        match value {
            Rvalue::Discriminant(p) => {
                Some((|| {
                    let (v, r) = e.place(p)?;
                    if matches!(r, Repr::Tagged | Repr::Dyn(_)) {
                        let value = e.coerce((v, r), Repr::Tagged)?;
                        let tags = std::iter::once("CAPTURE_NAMING_TAG".to_owned())
                            .chain(e.plan.enums.iter().map(|i| format!("CAPTURE_ENUM_{i}_TAG")))
                            .collect::<Vec<_>>()
                            .join("|");
                        let fail=e.fallible("Err::<i64,_>(rt::Error::internal(\"invalid enum discriminant\".into()))");
                        return Ok((
                            format!(
                                "{{let v={value};match v.tag{{{tags}=>v.payload as i64,_=>{fail}}}}}"
                            ),
                            Repr::Int,
                        ));
                    }
                    if !matches!(r, Repr::Naming | Repr::Enum(_)) {
                        return Err(no("non-enum discriminant"));
                    }
                    Ok((format!("({v}) as i64"), Repr::Int))
                })())
            }
            Rvalue::TypeTag(p) => {
                Some(e.place(p).and_then(|v| e.vm_tag(v)).map(|v| (v, Repr::Int)))
            }
            Rvalue::IsType { operand, test } => Some(
                e.operand(operand)
                    .and_then(|v| e.membership(v, &test.template(e.db)))
                    .map(|v| (v, Repr::Bool)),
            ),
            Rvalue::IsTypeTag { operand, tag } if *tag != baml_type::typetag::PANIC => Some(
                e.operand(operand)
                    .and_then(|v| e.vm_tag(v))
                    .map(|v| (format!("({v})=={tag}i64"), Repr::Bool)),
            ),
            _ => None,
        }
    }
}
impl Emitter<'_, '_> {
    pub(super) fn alias_test_id(&self, head: &baml_type::DeclName) -> Gen<i64> {
        let t = self
            .plan
            .index
            .anchor(&TyTemplate::TypeAlias(head.clone()))
            .ok_or_else(|| no("unlinked alias membership"))?;
        let bex_vm_types::TyTemplate::TypeAlias(h) = t else {
            unreachable!()
        };
        Ok(h.tag().as_i64())
    }

    pub(super) fn switch_key(&self, key: SwitchKey<'_>) -> Gen<String> {
        Ok(match key {
            SwitchKey::Int(i) => format!("{i}i64"),
            SwitchKey::Class(c) => {
                let (c, _) = self
                    .plan
                    .index
                    .class(self.db, c)
                    .ok_or_else(|| no("switch class"))?;
                let c = self.plan.class(c);
                format!("{}i64", c.type_tag.as_i64())
            }
        })
    }
    pub(super) fn vm_tag(&self, value: (String, Repr)) -> Gen<String> {
        let (v, r) = value;
        let scalar = match r {
            Repr::Int => Some(0),
            Repr::Float => Some(4),
            Repr::Str => Some(1),
            Repr::Bool => Some(2),
            Repr::Null => Some(3),
            Repr::Naming | Repr::Enum(_) => Some(5),
            Repr::ListInt | Repr::ListStr | Repr::ListListStr | Repr::ListPtr(_) => Some(6),
            Repr::MapStrInt => Some(7),
            Repr::Ptr(c) => {
                let c = self.plan.class(c);
                Some(c.type_tag.as_i64())
            }
            _ => None,
        };
        if let Some(tag) = scalar {
            return Ok(if r.pointer() {
                format!("if ({v})==0{{3i64}}else{{{tag}i64}}")
            } else {
                format!("{tag}i64")
            });
        }
        if !matches!(r, Repr::Tagged | Repr::Dyn(_)) {
            return Err(no("type tag"));
        }
        let mut out = format!("{{let v={v};match v.tag{{0=>0i64,1=>2,2=>3,3=>1,4294967295=>4,");
        for (&c, &desc) in self.descriptors {
            let class = self.plan.class(c);
            write!(out, "{}=>{},", desc + 3, class.type_tag.as_i64()).unwrap();
        }
        for (repr, desc) in self.lists {
            write!(
                out,
                "{}=>{},",
                desc + 3,
                if *repr == Repr::MapStrInt { 7 } else { 6 }
            )
            .unwrap();
        }
        out.push_str("CAPTURE_NAMING_TAG=>5,");
        for e in &self.plan.enums {
            write!(out, "CAPTURE_ENUM_{e}_TAG=>5,").unwrap();
        }
        out.push_str("_=>-1}}");
        Ok(out)
    }
    pub(super) fn membership(&self, value: (String, Repr), ty: &TyTemplate) -> Gen<String> {
        if let TyTemplate::Union(members) = ty {
            return Ok(format!(
                "({})",
                members
                    .iter()
                    .map(|t| self.membership(value.clone(), t))
                    .collect::<Gen<Vec<_>>>()?
                    .join(" || ")
            ));
        }
        if matches!(ty, TyTemplate::Unknown) {
            return Ok("true".into());
        }
        if matches!(ty, TyTemplate::Never) {
            return Ok("false".into());
        }
        let tag = match ty {
            TyTemplate::Int => "0".into(),
            TyTemplate::Float => "4294967295".into(),
            TyTemplate::Bool => "1".into(),
            TyTemplate::Null => "2".into(),
            TyTemplate::String => "3".into(),
            TyTemplate::Literal(lit, _) => {
                let tagged = self.coerce(value, Repr::Tagged)?;
                return Ok(match lit {
                    baml_type::Literal::String(s) => format!(
                        "{{let v={tagged};v.tag==3 && rt::strings::Str::from_raw(v.payload as usize).as_str(&rt::strings::Cx::current())=={:?}}}",
                        s.as_str()
                    ),
                    baml_type::Literal::Int(i) => {
                        format!("{{let v={tagged};v.tag==0 && v.payload==({i}i64 as u64)}}")
                    }
                    baml_type::Literal::Float(f) => {
                        let bits = f.parse::<f64>().map_err(|_| no("float literal"))?.to_bits();
                        format!(
                            "{{let v={tagged};v.tag==4294967295 && f64::from_bits(v.payload)==f64::from_bits({bits}u64)}}"
                        )
                    }
                    baml_type::Literal::Bigint(_) => {
                        return Err(no("numeric literal membership"));
                    }
                    baml_type::Literal::Bool(b) => format!(
                        "{{let v={tagged};v.tag==1 && v.payload=={}u64}}",
                        u8::from(*b)
                    ),
                });
            }
            TyTemplate::Class(head, args) if !args.is_empty() => {
                let Repr::Ptr(c) = self.plan.repr(&ty.substitute_symbolic(&[]))? else {
                    return Err(no("generic membership layout"));
                };
                (self.descriptors[&c] + 3).to_string()
            }
            TyTemplate::Class(head, args) if args.is_empty() => {
                let (c, _) = self
                    .plan
                    .index
                    .class_at(head)
                    .ok_or_else(|| no("membership class"))?;
                if head.namespace().as_slice() == ["iter"] && head.name().as_str() == "Done" {
                    "u64::MAX".into()
                } else {
                    self.descriptors
                        .get(&c)
                        .map(|d| (d + 3).to_string())
                        .ok_or_else(|| no("unplanned membership class"))?
                }
            }
            TyTemplate::Enum(head) => match self.plan.repr(&RuntimeTy::Enum(head.clone()))? {
                Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                Repr::Enum(e) => format!("CAPTURE_ENUM_{e}_TAG"),
                _ => unreachable!(),
            },
            TyTemplate::Interface(head, args, assoc) => {
                let iface = baml_compiler2_mir::TyTemplateInterface {
                    name: head.clone(),
                    generics: args.clone(),
                    associated_types: assoc.clone(),
                };
                let v = self.coerce(value, Repr::Tagged)?;
                let mut tags = BTreeSet::new();
                for t in &self.plan.concrete_types {
                    if super::dispatch::implements(self.db, self.plan, t, &iface)? {
                        let Ok(repr) = self.plan.repr(t) else {
                            continue;
                        };
                        let tag = match repr {
                            Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                            Repr::Enum(e) => format!("CAPTURE_ENUM_{e}_TAG"),
                            Repr::Ptr(c) if !self.descriptors.contains_key(&c) => continue,
                            _ => self.tag(repr)?.to_string(),
                        };
                        tags.insert(tag);
                    }
                }
                let checks = tags
                    .iter()
                    .map(|t| format!("v.tag=={t}"))
                    .collect::<Vec<_>>()
                    .join(" || ");
                return Ok(format!(
                    "{{let v={v};{}}}",
                    if checks.is_empty() { "false" } else { &checks }
                ));
            }
            TyTemplate::EnumVariant(head, variant) => {
                let e = self
                    .plan
                    .index
                    .enum_at(head)
                    .ok_or_else(|| no("enum variant test"))?;
                let Object::Enum(def) =
                    &self.plan.program.objects[bex_vm_types::ObjectIndex::from_raw(e)]
                else {
                    unreachable!()
                };
                let index = def
                    .variants
                    .iter()
                    .position(|v| v.name == variant.as_str())
                    .ok_or_else(|| no("enum variant index"))?;
                let tag = match self.plan.repr(&RuntimeTy::Enum(head.clone()))? {
                    Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                    _ => format!("CAPTURE_ENUM_{e}_TAG"),
                };
                let v = self.coerce(value, Repr::Tagged)?;
                return Ok(format!(
                    "{{let v={v};v.tag=={tag} && v.payload=={index}u64}}"
                ));
            }
            TyTemplate::TypeAlias(head) => {
                self.plan.alias_tests.borrow_mut().insert(head.clone());
                let id = self.alias_test_id(head)?;
                let v = self.coerce(value, Repr::Tagged)?;
                return Ok(format!("rt_alias{id}({v})"));
            }
            TyTemplate::Map { .. } => {
                let repr = self.plan.repr(&ty.substitute_symbolic(&[]))?;
                let Some(desc) = self.lists.get(&repr) else {
                    return Ok("false".into());
                };
                let v = self.coerce(value, Repr::Tagged)?;
                return Ok(format!("({v}).tag=={}", desc + 3));
            }
            TyTemplate::List(_) => {
                // VM membership uses the container's declared element type,
                // invariantly. Inspecting its current contents is unsound (an
                // int[] does not become Tree[] merely because int <: Tree).
                let requested = baml_type::Ty::from(ty.substitute_symbolic(&[]));
                let facts = baml_compiler2_hir_ty::facts::Facts::new(self.db);
                let v = self.coerce(value, Repr::Tagged)?;
                let mut code = format!("{{let value={v};match value.tag{{");
                for (&repr, &desc) in self.lists {
                    if repr == Repr::MapStrInt {
                        continue;
                    }
                    let mut answer = None;
                    for concrete in &self.plan.concrete_types {
                        if !matches!(concrete, RuntimeTy::List(_))
                            || self.plan.repr(concrete).ok() != Some(repr)
                        {
                            continue;
                        }
                        let matches = baml_type::normalize::is_subtype(
                            &baml_type::Ty::from(concrete.clone()),
                            &requested,
                            &facts,
                        );
                        if answer.is_some_and(|previous| previous != matches) {
                            return Err(no("ambiguous list type identity"));
                        }
                        answer = Some(matches);
                    }
                    let answer = answer.ok_or_else(|| no("missing list type identity"))?;
                    write!(code, "{}=>{answer},", desc + 3).unwrap();
                }
                code.push_str("_=>false}}");
                return Ok(code);
            }
            _ => return Err(no(format!("type membership {ty:?}"))),
        };
        let v = self.coerce(value, Repr::Tagged)?;
        Ok(format!("({v}).tag=={tag}"))
    }
}
