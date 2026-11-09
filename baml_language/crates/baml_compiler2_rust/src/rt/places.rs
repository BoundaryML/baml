use super::*;

impl Emitter<'_, '_> {
    /// Entry ABI before captured parameters are moved into shared cells.
    pub(super) fn argument_repr(&self, i: usize) -> Gen<Repr> {
        if let Some(repr) = super::string_defaults::parameter(self.plan, self.object, i) {
            return Ok(repr);
        }
        self.plan.local_repr(self.body, i, 0)
    }

    pub(super) fn local(&self, i: usize) -> Gen<Repr> {
        if self
            .body
            .blocks
            .iter()
            .any(|b| b.landing.is_some_and(|l| l.error_local.0 == i))
        {
            return Ok(Repr::Tagged);
        }
        if self.body.locals[i].is_captured {
            return Ok(Repr::Cell);
        }
        if self.iters.contains(&i) {
            return Ok(Repr::Iter);
        }
        if let Some(repr) = super::string_defaults::parameter(self.plan, self.object, i) {
            return Ok(repr);
        }
        self.plan.local_repr(self.body, i, 0)
    }
    pub(super) fn tag(&self, repr: Repr) -> Gen<usize> {
        match repr {
            Repr::Int => Ok(0),
            Repr::Bigint => Ok(self.plan.bigint_descriptor() + 3),
            Repr::Float => Ok(u32::MAX as usize - 1),
            Repr::Bool => Ok(1),
            Repr::Null => Ok(2),
            Repr::Str => Ok(3),
            Repr::Cell => Ok(self.plan.closure_descriptors().unwrap().0 + 3),
            Repr::Closure => Ok(self.plan.closure_descriptors().unwrap().1 + 3),
            Repr::Opaque => Ok(18),
            Repr::Bytes => Ok(19),
            Repr::Future => Ok(self.plan.future_descriptor().unwrap() + 3),
            Repr::Ptr(c) => Ok(self.descriptors[&c] + 3),
            r @ (Repr::List(_)
            | Repr::ListInt
            | Repr::ListStr
            | Repr::ListListStr
            | Repr::ListClosure(_)
            | Repr::ListFuture(_)
            | Repr::ListPtr(_)
            | Repr::MapStrInt
            | Repr::Map(_)
            | Repr::MapStrPtr(_)) => Ok(self.lists[&r] + 3),
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
        if source == target
            || (source == Repr::Null && target.pointer())
            || (matches!(source, Repr::Dyn(_)) && target == Repr::Tagged)
            || (source == Repr::Tagged && matches!(target, Repr::Dyn(_)))
        {
            return Ok(value);
        }
        if matches!(source, Repr::Dyn(_)) && matches!(target, Repr::Tagged | Repr::Dyn(_))
            || source == Repr::Tagged && matches!(target, Repr::Dyn(_))
        {
            return Ok(value);
        }
        if target == Repr::Tagged {
            let tag = match source {
                Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                Repr::Enum(e) => format!("CAPTURE_ENUM_{e}_TAG"),
                _ => self.tag(source)?.to_string(),
            };
            let test = if source.pointer() {
                "if v==0 {2} else {TAG}".replace("TAG", &tag)
            } else {
                tag
            };
            let payload = if source == Repr::Float {
                "v.to_bits()"
            } else {
                "v as u64"
            };
            return Ok(format!(
                "{{let v={value};rt::Dyn{{tag:{test},payload:{payload}}}}}"
            ));
        }
        if matches!(source, Repr::Tagged | Repr::Dyn(_)) {
            let tag = match target {
                Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                Repr::Enum(e) => format!("CAPTURE_ENUM_{e}_TAG"),
                _ => self.tag(target)?.to_string(),
            };
            let bits = self.fallible(&format!("rt::untag({value},{tag},{})", target.pointer()));
            return Ok(target.decode_payload(&bits));
        }
        if matches!(target, Repr::Dyn(_)) {
            return self.coerce((value, source), Repr::Tagged);
        }
        Err(no(format!("conversion {source:?} to {target:?}")))
    }
    pub(super) fn place(&self, p: &Place) -> Gen<(String, Repr)> {
        if let Some(result) = self.map_place(p) {
            return result;
        }
        match p {
            Place::Local(l) => Ok((format!("l{}", l.0), self.local(l.0)?)),
            Place::Capture(i) => Ok((format!("rt::closures::capture(env,{i})"), Repr::Cell)),
            Place::Deref(cell) => {
                let (cell, repr) = self.closure_cell(cell)?;
                Ok((
                    self.coerce((format!("rt::closures::load({cell})"), Repr::Tagged), repr)?,
                    repr,
                ))
            }
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
                let element = self.plan.element(repr)?;
                if self.local(index.0)? != Repr::Int {
                    return Err(no("non-int list index"));
                }
                if repr.growing() {
                    let slot = element.slot()?;
                    return Ok((self.fallible(&format!("rt::lists::List::<{slot}>::from_raw({base}).get(&rt::strings::Cx::current(),l{}).map(|v|{})", index.0, element.from_slot("v"))), element));
                }
                let slot = self.fallible(&format!("rt::list_slot({base},l{})", index.0));
                Ok((format!("*({slot} as *const {})", element.rust()), element))
            }
            _ => Err(no(format!("place {p:?}"))),
        }
    }
    pub(super) fn operand(&self, o: &Operand<'_>) -> Gen<(String, Repr)> {
        match o {
            Operand::Copy(p) | Operand::Move(p) => {
                let value = self.place(p)?;
                if let Place::Local(l) = p {
                    if l.0 > 0
                        && l.0 <= self.arity
                        && !self.body.locals[l.0].is_captured
                        && value.1 == Repr::Tagged
                    {
                        let repr = self.plan.repr(&self.body.locals[l.0].ty)?;
                        if repr != Repr::Tagged {
                            return Ok((self.coerce(value, repr)?, repr));
                        }
                    }
                }
                Ok(value)
            }
            Operand::Constant(Constant::GlobalItem(binding)) => {
                let slot = self
                    .plan
                    .index
                    .let_slot(self.db, *binding)
                    .ok_or_else(|| no("unlinked global"))?;
                let repr = *self
                    .plan
                    .globals
                    .get(&slot)
                    .ok_or_else(|| no("unplanned global"))?;
                let name = baml_compiler2_mir::global_display_name(self.db, *binding);
                let value = self.fallible(&format!("rt::globals::load_named({slot},{name:?})"));
                Ok((self.coerce((value, Repr::Tagged), repr)?, repr))
            }
            Operand::Constant(Constant::Float(f)) => {
                Ok((format!("f64::from_bits({}u64)", f.to_bits()), Repr::Float))
            }
            Operand::Constant(Constant::Bigint(i)) => Ok((
                self.fallible(&format!(
                    "rt::tasks::bigint_text({}, {:?})",
                    self.plan.bigint_descriptor(),
                    i.to_string()
                )),
                Repr::Bigint,
            )),
            Operand::Constant(Constant::Int(i)) => Ok((format!("{i}i64"), Repr::Int)),
            Operand::Constant(Constant::Bool(b)) => Ok((b.to_string(), Repr::Bool)),
            Operand::Constant(Constant::String(text)) => self.string_literal(text.as_str()),
            Operand::Constant(Constant::EnumVariant { enum_ref, index }) => {
                let head = baml_compiler2_hir_ty::layout::enum_head(self.db, *enum_ref);
                Ok((
                    format!("{index}usize"),
                    self.plan.repr(&RuntimeTy::Enum(head))?,
                ))
            }
            Operand::Constant(Constant::Function(f)) => {
                let (object, _) = self
                    .plan
                    .index
                    .function(self.db, *f)
                    .ok_or_else(|| no("unlinked function value"))?;
                Ok((
                    self.fallible(&format!(
                        "rt::closures::function({}, {object},closure{object},{:?})",
                        self.plan.closure_descriptors().unwrap().1,
                        self.plan.index.info(object).name().as_str()
                    )),
                    Repr::Closure,
                ))
            }
            Operand::Constant(Constant::GenericFunction { func, type_args }) => {
                let (base, _) = self
                    .plan
                    .index
                    .function(self.db, *func)
                    .ok_or_else(|| no("generic function"))?;
                let frame = type_args
                    .iter()
                    .map(|t| RuntimeTy::from(t.clone()))
                    .collect::<Vec<_>>();
                let frame = super::generics::callable_frame(self.db, *func, frame);
                let object = self
                    .plan
                    .generic_functions
                    .iter()
                    .find(|(c, t, _)| *c == base && *t == frame)
                    .map(|(_, _, id)| *id)
                    .or_else(|| frame.is_empty().then_some(base))
                    .ok_or_else(|| no("generic function instance"))?;
                Ok((
                    self.fallible(&format!(
                        "rt::closures::generic_function({}, {object},closure{object})",
                        self.plan.closure_descriptors().unwrap().1
                    )),
                    Repr::Closure,
                ))
            }
            Operand::Constant(Constant::OmittedArg) => {
                Ok(("rt::Dyn{tag:u64::MAX,payload:0}".into(), Repr::Tagged))
            }
            Operand::Constant(Constant::Null) => Ok(("0usize".into(), Repr::Null)),
            Operand::Constant(_) => Err(no(format!("operand {o:?}"))),
        }
    }
    pub(super) fn assign(&self, p: &Place, value: (String, Repr)) -> Gen<String> {
        if let Place::Deref(cell) = p {
            let (cell, _) = self.closure_cell(cell)?;
            let value = self.coerce(value, Repr::Tagged)?;
            return Ok(format!("rt::closures::store({cell},{value});\n"));
        }
        if let Some(result) = self.map_assign(p, value.clone()) {
            return result;
        }
        let (dst, repr) = self.place(p)?;
        // MIR uses anonymous null temporaries for discarded expressions.
        // Evaluate the source (including fallible calls), then store null.
        let discard = matches!(p,Place::Local(l) if self.body.locals[l.0].name.is_none())
            && repr == Repr::Null;
        let src = if discard {
            format!("{{let _={};0usize}}", value.0)
        } else {
            self.coerce(value, repr)
                .map_err(|error| format!("{error} (destination {p:?})"))?
        };
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
                    let slot = repr.slot()?;
                    let value = repr.to_slot(&src)?;
                    let store = self.fallible(&format!("rt::lists::List::<{slot}>::from_raw(object).set(&rt::strings::Cx::current(),l{},value)", index.0));
                    return Ok(format!("{{let value={value};let object={obj};{store};}}\n"));
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
