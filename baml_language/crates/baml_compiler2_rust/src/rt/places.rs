use super::*;

impl Emitter<'_, '_> {
    pub(super) fn local(&self, i: usize) -> Gen<Repr> {
        if self
            .body
            .blocks
            .iter()
            .any(|b| b.landing.is_some_and(|l| l.error_local.0 == i))
        {
            return Ok(Repr::Tagged);
        }
        if self.iters.contains(&i) {
            return Ok(Repr::Iter);
        }
        self.plan.local_repr(self.body, i, 0)
    }
    pub(super) fn tag(&self, repr: Repr) -> Gen<usize> {
        match repr {
            Repr::Int => Ok(0),
            Repr::Bool => Ok(1),
            Repr::Null => Ok(2),
            Repr::Str => Ok(3),
            Repr::Ptr(c) => Ok(self.descriptors[&c] + 3),
            r @ (Repr::ListInt
            | Repr::ListStr
            | Repr::ListListStr
            | Repr::ListPtr(_)
            | Repr::MapStrInt) => Ok(self.lists[&r] + 3),
            _ => Err(no("static tag")),
        }
    }
    pub(super) fn null_test(value: &(String, Repr)) -> String {
        if value.1 == Repr::Tagged {
            format!("({}).tag==2", value.0)
        } else if value.1.pointer() || value.1 == Repr::Null {
            format!("({})==0", value.0)
        } else {
            "false".into()
        }
    }
    pub(super) fn coerce(&self, value: (String, Repr), target: Repr) -> Gen<String> {
        let (value, source) = value;
        if source == target || (source == Repr::Null && target.pointer()) {
            return Ok(value);
        }
        if target == Repr::Tagged {
            let tag = self.tag(source)?;
            let test = if source.pointer() {
                "if v==0 {2} else {TAG}".replace("TAG", &tag.to_string())
            } else {
                tag.to_string()
            };
            return Ok(format!(
                "{{let v={value};rt::Dyn{{tag:{test},payload:v as u64}}}}"
            ));
        }
        if source == Repr::Tagged {
            let tag = self.tag(target)?;
            let bits = self.fallible(&format!("rt::untag({value},{tag},{})", target.pointer()));
            return Ok(if target == Repr::Bool {
                format!("({bits}!=0)")
            } else {
                format!("({bits} as {})", target.rust())
            });
        }
        if let (Repr::Ptr(c), Repr::Dyn(i)) = (source, target) {
            if self
                .plan
                .implementors(i)?
                .iter()
                .any(|(class, _)| *class == c)
            {
                return Ok(format!(
                    "rt::Dyn{{tag:{},payload:({value}) as u64}}",
                    self.descriptors[&c] + 3
                ));
            }
        }
        Err(no(format!("conversion {source:?} to {target:?}")))
    }
    pub(super) fn place(&self, p: &Place) -> Gen<(String, Repr)> {
        match p {
            Place::Local(l) => Ok((format!("l{}", l.0), self.local(l.0)?)),
            Place::Field { base, field } => {
                let (base, repr) = self.place(base)?;
                if repr == Repr::Tagged {
                    let mut expression = format!("{{let value={base};match value.tag{{");
                    for (&class, layout) in &self.plan.layouts {
                        let Some(&field_repr) = layout.fields.get(*field) else {
                            continue;
                        };
                        let raw = format!("(*(value.payload as *const C{class})).f{field}");
                        let raw = if field_repr == Repr::Bool {
                            format!("({raw}!=0)")
                        } else {
                            raw
                        };
                        let tagged = self.coerce((raw, field_repr), Repr::Tagged)?;
                        write!(
                            &mut expression,
                            "{}=>{tagged},",
                            self.descriptors[&class] + 3
                        )
                        .unwrap();
                    }
                    let fail=self.fallible("Err::<rt::Dyn,_>(rt::Error::internal(\"invalid typed field access\".into()))");
                    write!(&mut expression, "_=>{fail}}}}}").unwrap();
                    return Ok((expression, Repr::Tagged));
                }
                let Repr::Ptr(c) = repr else {
                    return Err(no("field on non-class"));
                };
                let layout = &self.plan.layouts[&c];
                let repr = *layout.fields.get(*field).ok_or_else(|| no("field index"))?;
                let checked = self.fallible(&format!("rt::nonnull({base})"));
                let value = format!("(*({checked} as *const C{c})).f{field}");
                Ok((
                    if repr == Repr::Bool {
                        format!("({value} != 0)")
                    } else {
                        value
                    },
                    repr,
                ))
            }
            Place::Index {
                base,
                index,
                kind: baml_compiler2_mir::IndexKind::Array,
            } => {
                let (base, repr) = self.place(base)?;
                let element = Self::element(repr)?;
                if self.local(index.0)? != Repr::Int {
                    return Err(no("non-int list index"));
                }
                if repr.growing() {
                    let slot = element.slot()?;
                    return Ok((self.fallible(&format!("rt::lists::List::<{slot}>::from_raw({base}).get(&rt::strings::Cx::current(),l{}).map(|v|v.as_raw())", index.0)), element));
                }
                let slot = self.fallible(&format!("rt::list_slot({base},l{})", index.0));
                Ok((format!("*({slot} as *const {})", element.rust()), element))
            }
            _ => Err(no(format!("place {p:?}"))),
        }
    }
    pub(super) fn operand(&self, o: &Operand<'_>) -> Gen<(String, Repr)> {
        match o {
            Operand::Copy(p) | Operand::Move(p) => self.place(p),
            Operand::Constant(Constant::Int(i)) => Ok((format!("{i}i64"), Repr::Int)),
            Operand::Constant(Constant::Bool(b)) => Ok((b.to_string(), Repr::Bool)),
            Operand::Constant(Constant::String(text)) => self.string_literal(text.as_str()),
            Operand::Constant(Constant::EnumVariant { enum_ref, index }) => {
                let head = baml_compiler2_hir_ty::layout::enum_head(self.db, *enum_ref);
                if self.plan.repr(&RuntimeTy::Enum(head))? != Repr::Naming || *index > 1 {
                    return Err(no("enum constant"));
                }
                Ok((format!("{index}usize"), Repr::Naming))
            }
            Operand::Constant(Constant::Null) => Ok(("0usize".into(), Repr::Null)),
            Operand::Constant(_) => Err(no(format!("operand {o:?}"))),
        }
    }
    pub(super) fn assign(&self, p: &Place, value: (String, Repr)) -> Gen<String> {
        let (dst, repr) = self.place(p)?;
        let src = self.coerce(value, repr)?;
        match p {
            Place::Local(_) => Ok(format!("{dst}={src};\n")),
            Place::Field { base, field } => {
                let (obj, Repr::Ptr(c)) = self.place(base)? else {
                    return Err(no("field store"));
                };
                let obj = self.fallible(&format!("rt::nonnull({obj})"));
                let slot = format!("&raw mut (*(object as *mut C{c})).f{field}");
                let write = if repr.pointer() {
                    format!("baml_rt_gc::write_ptr(object as *mut u8,{slot},value);")
                } else if matches!(repr, Repr::Dyn(_) | Repr::Tagged) {
                    format!("rt::write_dyn(object as *mut u8,{slot},value);")
                } else {
                    format!("({slot}).write(value as _);")
                };
                Ok(format!("{{let value={src};let object={obj};{write}}}\n"))
            }
            Place::Index {
                base,
                index,
                kind: baml_compiler2_mir::IndexKind::Array,
            } => {
                let (obj, container) = self.place(base)?;
                if container.growing() {
                    return Err(no("growing list indexed assignment"));
                }
                let slot = self.fallible(&format!("rt::list_slot(object,l{})", index.0));
                let write = if repr.pointer() {
                    "baml_rt_gc::write_ptr(object as *mut u8,slot.cast(),value);".to_owned()
                } else {
                    format!("slot.cast::<{}>().write(value);", repr.rust())
                };
                Ok(format!(
                    "{{let value={src};let object={obj};let slot={slot};{write}}}\n"
                ))
            }
            _ => Err(no("store")),
        }
    }
}
