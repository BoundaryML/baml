use super::{BinOp, Constant, Db, Emitter, Gen, Operand, Plan, Repr, Rvalue, Terminator, registry};

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
        let (callee, _) = plan.index.function(db, *f)?;
        let name = plan.index.display(callee).to_owned();
        if !matches!(
            name.as_str(),
            "baml.Int.parse" | "baml.Float.parse" | "baml.String.from_code_points"
        ) {
            return None;
        }
        let error_class = if matches!(name.as_str(), "baml.Int.parse" | "baml.Float.parse") {
            "baml.errors.ParseError"
        } else {
            "baml.errors.InvalidArgument"
        };
        Some((|| {
            let error = builtin_class(plan, error_class)?;
            plan.add(error)?;
            Ok(registry::Reach {
                builtin: Some(name.to_owned()),
                ..Default::default()
            })
        })())
    }

    fn call(
        &self,
        emitter: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name == "baml._to_string_primitive" {
            return Some((|| {
                if !types.is_empty()
                    || values.len() != 1
                    || !matches!(
                        values[0].1,
                        Repr::Int | Repr::Float | Repr::Bool | Repr::Null
                    )
                {
                    return Err(super::no(
                        "primitive string conversion of dynamic or unsupported scalar",
                    ));
                }
                let value = emitter.coerce(values[0].clone(), Repr::Tagged)?;
                Ok((emitter.fallible(&format!("rt_primitive_to_string(&mut rt::strings::Cx::current(),{value}).map(|s|s.as_raw())")),Repr::Str))
            })());
        }
        if matches!(
            name,
            "baml.Int.parse" | "baml.Float.parse" | "baml.String.from_code_points"
        ) {
            return Some((|| {
                let integer = name == "baml.Int.parse";
                let float = name == "baml.Float.parse";
                let error_class = if integer || float {
                    "baml.errors.ParseError"
                } else {
                    "baml.errors.InvalidArgument"
                };
                let error = builtin_class(emitter.plan, error_class)?;
                if values.len() != 1
                    || values[0].1
                        != if integer || float {
                            Repr::Str
                        } else {
                            Repr::ListInt
                        }
                {
                    return Err(super::no("string conversion arguments"));
                }
                let desc = emitter.descriptors[&error];
                let value = &values[0].0;
                let (call, repr) = if integer || float {
                    (
                        format!(
                            "{}(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw({value}),{desc})",
                            if float {
                                "rt_float_parse"
                            } else {
                                "rt_int_parse"
                            }
                        ),
                        if float { Repr::Float } else { Repr::Int },
                    )
                } else {
                    (
                        format!(
                            "rt_from_code_points(&mut rt::strings::Cx::current(),{value},{desc}).map(|s|s.as_raw())"
                        ),
                        Repr::Str,
                    )
                };
                Ok((emitter.fallible(&call), repr))
            })());
        }
        if name == "baml._to_string_default" && values.len() == 1 {
            match values[0].1 {
                Repr::Int=>return Some(Ok((emitter.fallible(&format!("rt_int_to_string(&mut rt::strings::Cx::current(),{}).map(|s|s.as_raw())",values[0].0)),Repr::Str))),
                Repr::Str=>return Some(Ok(values[0].clone())),
                Repr::Float=>return Some(Ok((emitter.fallible(&format!("rt::strings::float_to_string(&mut rt::strings::Cx::current(),{}).map(|s|s.as_raw())",values[0].0)),Repr::Str))),
                Repr::Bool=>return Some(Ok((emitter.fallible(&format!("rt::strings::Cx::current().string(if {}{{\"true\"}}else{{\"false\"}}).map(|s|s.as_raw())",values[0].0)),Repr::Str))),
                Repr::Null=>return Some(Ok((emitter.fallible("rt::strings::Cx::current().string(\"null\").map(|s|s.as_raw())"),Repr::Str))),
                _=>{},
            }
        }
        let method = name.strip_prefix("baml.String.")?;
        (baml_builtins2_codegen::supports_rt(name)
            && !matches!(method, "char_count" | "slice" | "to_code_points"))
        .then(|| emitter.string_call(method, types, values))
    }
    fn value(
        &self,
        emitter: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if matches!(value, Rvalue::ConcatStrings(_)) {
            return Some(emitter.strings_value(value, expected));
        }
        let Rvalue::BinaryOp {
            op: op @ (BinOp::Add | BinOp::Eq | BinOp::Ne),
            left,
            right,
        } = value
        else {
            return None;
        };
        let ascii_literal = match right {
            Operand::Constant(Constant::String(text)) if *op == BinOp::Add && text.len() == 1 => {
                Some(text.as_str().to_owned())
            }
            _ => None,
        };
        let left = emitter.operand(left).ok()?;
        let right = emitter.operand(right).ok()?;
        if left.1 != Repr::Str && right.1 != Repr::Str {
            return None;
        }
        if *op == BinOp::Add {
            if left.1 != Repr::Str || right.1 != Repr::Str {
                return Some(Err(super::no("string concatenation arguments")));
            }
            emitter.string_cx.set(true);
            let (left, right) = (left.0, right.0);
            if let Some(text) = ascii_literal {
                let suffix = text.as_bytes()[0];
                let index = emitter.string_literals.borrow()[&text];
                let call = format!(
                    "rt::strings::append_ascii(&mut string_cx,rt::strings::Str::from_raw({left}),{suffix},|cx|{{if string_literal{index}==0{{string_literal{index}=cx.string({text:?})?.as_raw();}}Ok(rt::strings::Str::from_raw(string_literal{index}))}}).map(|s|s.as_raw())"
                );
                return Some(Ok((emitter.fallible(&call), Repr::Str)));
            }
            return Some(Ok((emitter.fallible(&format!("rt::strings::concat(&mut string_cx,&[rt::strings::Str::from_raw({left}),rt::strings::Str::from_raw({right})]).map(|s|s.as_raw())")), Repr::Str)));
        }
        Some((|| {
            let a = emitter.coerce(left, Repr::Tagged)?;
            let b = emitter.coerce(right, Repr::Tagged)?;
            let compare=emitter.fallible("rt::strings::equals_raw(&rt::strings::Cx::current(),a.payload as usize,b.payload as usize)");
            let expression = format!(
                "{{let a={a};let b={b};a.tag==b.tag && (a.tag==2 || (a.tag==3 && {compare}))}}"
            );
            Ok((
                if *op == BinOp::Ne {
                    format!("!({expression})")
                } else {
                    expression
                },
                Repr::Bool,
            ))
        })())
    }
}
impl Emitter<'_, '_> {
    fn strings_value(&self, r: &Rvalue<'_>, _expected: Repr) -> Gen<(String, Repr)> {
        match r {
            Rvalue::ConcatStrings(parts) => {
                self.string_cx.set(true);
                let values = parts
                    .iter()
                    .map(|part| {
                        let value = self.coerce(self.operand(part)?, Repr::Str)?;
                        Ok(format!("rt::strings::Str::from_raw({value})"))
                    })
                    .collect::<Gen<Vec<_>>>()?
                    .join(",");
                Ok((
                    self.fallible(&format!(
                        "rt::strings::concat(&mut string_cx,&[{values}]).map(|s|s.as_raw())"
                    )),
                    Repr::Str,
                ))
            }
            _ => unreachable!("feature value guard"),
        }
    }
}

impl Emitter<'_, '_> {
    fn string_call(
        &self,
        method: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Gen<(String, Repr)> {
        if !types.is_empty() || values.is_empty() {
            return Err(super::no("string builtin arguments"));
        }
        self.string_cx.set(true);
        let mut args = vec!["&mut string_cx".to_owned()];
        for (i, value) in values.iter().enumerate() {
            let repr = if i > 0 && matches!(method, "at" | "code_point_at" | "repeat") {
                Repr::Int
            } else {
                Repr::Str
            };
            let value = self.coerce(value.clone(), repr)?;
            args.push(if repr == Repr::Str {
                format!("rt::strings::Str::from_raw({value})")
            } else {
                value
            });
        }
        let repr = match method {
            "split" | "chars" | "lines" => {
                args.push(self.lists[&Repr::ListStr].to_string());
                Repr::ListStr
            }
            "byte_length" => Repr::Int,
            "index_of" | "last_index_of" | "code_point_at" => Repr::Tagged,
            "at"
            | "to_lower_case"
            | "to_upper_case"
            | "trim"
            | "trim_start"
            | "trim_end"
            | "repeat"
            | "_replace_literal"
            | "_replace_all_literal" => Repr::Str,
            _ => Repr::Bool,
        };
        let mut expression = format!("rt_string_{method}({})", args.join(","));
        if repr == Repr::Str && method != "at" {
            expression.push_str(".map(|s|s.as_raw())")
        }
        Ok((self.fallible(&expression), repr))
    }
}

impl Emitter<'_, '_> {
    pub(super) fn string_literal(&self, text: &str) -> Gen<(String, Repr)> {
        let mut literals = self.string_literals.borrow_mut();
        let next = literals.len();
        let index = *literals.entry(text.to_owned()).or_insert(next);
        let mut ids = self.plan.literal_ids.borrow_mut();
        let next_id = ids.len();
        let id = *ids.entry(text.to_owned()).or_insert(next_id);
        let value = self.fallible(&format!("rt::globals::string_literal({id},{text:?})"));
        // Initialize lazily at the original error site. The per-function local
        // avoids repeated pool access inside loops; the instance-owned pool
        // shares the immutable value across calls and traces it precisely.
        Ok((
            format!(
                "{{if string_literal{index}==0{{string_literal{index}={value};}}string_literal{index}}}"
            ),
            Repr::Str,
        ))
    }
    pub(super) fn string_literal_locals(&self) -> String {
        let mut locals = if self.string_cx.get() {
            "let mut string_cx=rt::strings::Cx::current();\n".to_owned()
        } else {
            String::new()
        };
        for i in 0..self.string_literals.borrow().len() {
            locals.push_str(&format!("let mut string_literal{i}=0usize;\n"));
        }
        locals
    }
}

pub(super) fn builtin_class(plan: &Plan<'_>, name: &str) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .find_map(|package| {
            package.classes.iter().find_map(|(path, index)| {
                (path.under(&package.name).to_string() == name).then_some(index.raw())
            })
        })
        .ok_or_else(|| super::no(format!("class declaration {name}")))
}
