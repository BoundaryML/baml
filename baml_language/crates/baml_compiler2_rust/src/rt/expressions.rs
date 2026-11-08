use super::*;

impl Emitter<'_, '_> {
    pub(super) fn value(&self, r: &Rvalue<'_>, expected: Repr) -> Gen<(String, Repr)> {
        if let Some(result) = registry::value(self, r, expected) {
            return result;
        }
        match r {
            Rvalue::IsTypeTag {
                operand: Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)),
                tag,
            } if *tag == baml_type::typetag::PANIC => {
                let handler = self
                    .body
                    .blocks
                    .iter()
                    .find(|b| b.landing.is_some_and(|l| l.error_local == *local))
                    .ok_or_else(|| no("panic test outside catch"))?
                    .id
                    .0;
                Ok((
                    format!(
                        "caught{handler}.as_ref().expect(\"missing caught error\").error().class.starts_with(\"baml.panics.\")"
                    ),
                    Repr::Bool,
                ))
            }
            Rvalue::IsType { operand, test } => {
                let value = self.coerce(self.operand(operand)?, Repr::Tagged)?;
                if let baml_compiler2_mir::TypeTest::Template(
                    baml_compiler2_mir::TyTemplate::Literal(baml_type::Literal::String(literal), _),
                ) = test
                {
                    return Ok((
                        format!(
                            "{{let v={value};v.tag==3 && rt::strings::Str::from_raw(v.payload as usize).as_str(&rt::strings::Cx::current())=={:?}}}",
                            literal.as_str()
                        ),
                        Repr::Bool,
                    ));
                }
                let tag = match test {
                    baml_compiler2_mir::TypeTest::Class { class, args } if args.is_empty() => {
                        let head = baml_compiler2_hir_ty::layout::class_head(self.db, *class);
                        if Some(head.root()) == self.plan.lang.get(baml_base::LangPackage::Baml)
                            && head.namespace().as_slice() == ["iter"]
                            && head.name().as_str() == "Done"
                        {
                            "u64::MAX".to_owned()
                        } else {
                            let (c, _) = self
                                .plan
                                .index
                                .class(self.db, *class)
                                .ok_or_else(|| no("type-test class"))?;
                            self.descriptors
                                .get(&c)
                                .ok_or_else(|| no("unplanned type-test class"))?
                                .checked_add(3)
                                .unwrap()
                                .to_string()
                        }
                    }
                    _ => return Err(no("type test")),
                };
                Ok((format!("({value}).tag=={tag}"), Repr::Bool))
            }
            Rvalue::LoadType(ty) => Ok(("0usize".into(), Repr::Type(self.plan.type_code(ty)?))),
            Rvalue::Use(o) => self.operand(o),
            Rvalue::BinaryOp { op, left, right } => {
                let (l, lt) = self.operand(left)?;
                let (r, rt) = self.operand(right)?;
                if *op == BinOp::Add && lt == Repr::Str && rt == Repr::Str {
                    return Ok((self.fallible(&format!("rt::strings::concat(&mut rt::strings::Cx::current(),&[rt::strings::Str::from_raw({l}),rt::strings::Str::from_raw({r})]).map(|s|s.as_raw())")), Repr::Str));
                }
                if matches!(lt, Repr::Enum(_) | Repr::Naming)
                    && lt == rt
                    && matches!(op, BinOp::Eq | BinOp::Ne)
                {
                    return Ok((format!("({l} {op} {r})"), Repr::Bool));
                }
                if lt == Repr::Float && rt == Repr::Float {
                    let repr = match op {
                        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                            Repr::Float
                        }
                        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                            Repr::Bool
                        }
                        _ => return Err(no("float binary operation")),
                    };
                    return Ok((format!("({l} {op} {r})"), repr));
                }
                if lt != Repr::Int || rt != Repr::Int {
                    return Err(no(format!(
                        "non-integer binary operation in {}: {lt:?} {op} {rt:?}",
                        self.plan.index.display(self.object)
                    )));
                }
                let helper = match op {
                    BinOp::Add => Some("add"),
                    BinOp::Sub => Some("sub"),
                    BinOp::Mul => Some("mul"),
                    BinOp::Div => Some("div"),
                    BinOp::Mod => Some("rem"),
                    BinOp::Shl => Some("shl"),
                    BinOp::Shr => Some("shr"),
                    _ => None,
                };
                if let Some(h) = helper {
                    return Ok((self.fallible(&format!("rt::{h}({l},{r})")), Repr::Int));
                }
                let ty = match op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Repr::Bool
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => Repr::Int,
                    _ => return Err(no("division/remainder")),
                };
                Ok((format!("({l} {op} {r})"), ty))
            }
            Rvalue::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => {
                let (v, t) = self.operand(operand)?;
                if t != Repr::Bool {
                    return Err(no("non-bool not"));
                }
                Ok((format!("!({v})"), t))
            }
            Rvalue::Aggregate {
                kind:
                    AggregateKind::Class {
                        class,
                        type_arg_templates,
                    },
                fields,
            } => {
                let (c, _) = self
                    .plan
                    .index
                    .class(self.db, *class)
                    .ok_or_else(|| no("class"))?;
                let c = if type_arg_templates.is_empty() {
                    c
                } else {
                    let head = baml_compiler2_hir_ty::layout::class_head(self.db, *class);
                    let ty = baml_compiler2_mir::TyTemplate::Class(
                        head,
                        type_arg_templates.clone().into_boxed_slice(),
                    )
                    .substitute_symbolic(&[]);
                    let Repr::Ptr(c) = self.plan.repr(&ty)? else {
                        return Err(no("generic class representation"));
                    };
                    c
                };
                let layout = &self.plan.layouts[&c];
                if fields.len() != layout.fields.len() {
                    return Err(no("partial class literal"));
                }
                let mut init = String::from("{");
                // Bind every input before allocation: roots stay in addressable Rust locals.
                for (i, v) in fields.iter().enumerate() {
                    let v = self.coerce(self.operand(v)?, layout.fields[i])?;
                    write!(&mut init, "let a{i}={v};").unwrap();
                }
                write!(
                    &mut init,
                    "let p={} as *mut C{c};",
                    self.fallible(&format!(
                        "rt::try_alloc({}, {})",
                        self.descriptors[&c], layout.bytes
                    ))
                )
                .unwrap();
                for (i, t) in layout.fields.iter().enumerate() {
                    if t.pointer() {
                        write!(
                            &mut init,
                            "baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f{i}).cast(),a{i});"
                        )
                        .unwrap();
                    } else if matches!(t, Repr::Dyn(_) | Repr::Tagged) {
                        write!(
                            &mut init,
                            "rt::write_dyn(p.cast(),&raw mut (*p).f{i},a{i});"
                        )
                        .unwrap();
                    } else {
                        write!(&mut init, "(*p).f{i}=a{i} as _;").unwrap();
                    }
                }
                init.push_str("p as usize}");
                Ok((init, Repr::Ptr(c)))
            }
            _ => Err(no(format!("rvalue {r:?}"))),
        }
    }
}
