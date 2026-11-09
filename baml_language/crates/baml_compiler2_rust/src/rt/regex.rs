//! Typed regex values, captures and the minimum named-group map boundary.
use super::*;
pub(super) struct Feature;
fn classes(plan: &mut Plan<'_>) -> Gen<()> {
    for name in [
        "baml.regex.Regex",
        "baml.regex.Group",
        "baml.regex.Match",
        "baml.regex.Error",
        "baml.panics.UserPanic",
    ] {
        let c = super::strings::builtin_class(plan, name)?;
        plan.add(c)?;
    }
    let group = super::strings::builtin_class(plan, "baml.regex.Group")?;
    let found = super::strings::builtin_class(plan, "baml.regex.Match")?;
    for repr in [
        Repr::ListPtr(group),
        Repr::ListPtr(found),
        Repr::MapStrPtr(group),
        Repr::ListStr,
    ] {
        plan.record(repr)?;
    }
    Ok(())
}
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
        if !name.starts_with("baml.regex.") || !baml_builtins2_codegen::supports_rt(&name) {
            return None;
        }
        Some((|| {
            if name != "baml.regex.escape" && name != "baml.regex._index_of_from" {
                classes(plan)?;
            }
            Ok(registry::Reach {
                builtin: Some(name),
                ..Default::default()
            })
        })())
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name.starts_with("baml.Map.")
            && values
                .first()
                .is_some_and(|v| matches!(v.1, Repr::MapStrPtr(_)))
        {
            return Some(map_call(e, name, values));
        }
        if !name.starts_with("baml.regex.") || !baml_builtins2_codegen::supports_rt(name) {
            return None;
        }
        Some((|| {
            if !types.is_empty() {
                return Err(no("regex type arguments"));
            }
            let method = name.rsplit('.').next().unwrap();
            let mut args = vec!["&mut rt::strings::Cx::current()".to_owned()];
            let (input, output) = match method {
                "_compile" => (
                    vec![Repr::Str, Repr::Bool],
                    Repr::Ptr(super::strings::builtin_class(e.plan, "baml.regex.Regex")?),
                ),
                "escape" => (vec![Repr::Str], Repr::Str),
                "_index_of_from" => (vec![Repr::Str, Repr::Str, Repr::Int], Repr::Tagged),
                _ => {
                    let regex =
                        Repr::Ptr(super::strings::builtin_class(e.plan, "baml.regex.Regex")?);
                    let mut input = vec![regex, Repr::Str];
                    let output = match method {
                        "has_match" => Repr::Bool,
                        "match_once" | "match_full" => {
                            Repr::Ptr(super::strings::builtin_class(e.plan, "baml.regex.Match")?)
                        }
                        "match_all" => Repr::ListPtr(super::strings::builtin_class(
                            e.plan,
                            "baml.regex.Match",
                        )?),
                        "split" => Repr::ListStr,
                        "_replace_template" | "_replace_all_template" => {
                            input.push(Repr::Str);
                            Repr::Str
                        }
                        _ => return Err(no("regex native method")),
                    };
                    (input, output)
                }
            };
            if input.len() != values.len() {
                return Err(no("regex native arity"));
            }
            for (value, repr) in values.iter().cloned().zip(input) {
                let value = e.coerce(value, repr)?;
                args.push(if repr == Repr::Str {
                    format!("rt::strings::Str::from_raw({value})")
                } else {
                    value
                });
            }
            if method == "split" {
                args.push(e.lists[&Repr::ListStr].to_string());
            }
            if method != "escape" && method != "_index_of_from" {
                args.push("&REGEX_SCHEMA".into());
            }
            let adapter = match method {
                "_compile" => "compile",
                "_index_of_from" => "index_from",
                _ => method,
            };
            let conversion = if output == Repr::Str {
                ".map(|s|s.as_raw())"
            } else {
                ""
            };
            Ok((
                e.fallible(&format!(
                    "rt_regex_{adapter}({}){conversion}",
                    args.join(",")
                )),
                output,
            ))
        })())
    }
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if let Rvalue::BinaryOp {
            op: op @ (BinOp::Eq | BinOp::Ne),
            left,
            right,
        } = value
        {
            let left = e.operand(left).ok()?;
            let right = e.operand(right).ok()?;
            if left.1 == Repr::Null || right.1 == Repr::Null {
                let other = if left.1 == Repr::Null { &right } else { &left };
                let test = Emitter::null_test(other);
                return Some(Ok((
                    if *op == BinOp::Eq {
                        test
                    } else {
                        format!("!({test})")
                    },
                    Repr::Bool,
                )));
            }
        }
        if let Rvalue::Len(place) = value {
            let (value, repr) = e.place(place).ok()?;
            if matches!(repr, Repr::MapStrPtr(_)) {
                return Some(Ok((
                    format!("rt::regex_map::StringPtrMap::from_raw({value}).len()"),
                    Repr::Int,
                )));
            }
        }
        let Rvalue::Map(_, value_ty, entries) = value else {
            return None;
        };
        let Repr::MapStrPtr(class) = expected else {
            return None;
        };
        Some((|| {
            let mut code = String::from("{");
            for (i, (key, value)) in entries.iter().enumerate() {
                let key = e.coerce(e.operand(key)?, Repr::Str)?;
                let value = e.coerce(e.operand(value)?, Repr::Ptr(class))?;
                write!(code, "let k{i}={key};let v{i}={value};").unwrap();
            }
            write!(
                code,
                "let map={};",
                e.fallible(&format!(
                    "rt::regex_map::StringPtrMap::new(&mut rt::strings::Cx::current(),{})",
                    e.lists[&expected]
                ))
            )
            .unwrap();
            if matches!(value_ty.substitute_symbolic(&[]),RuntimeTy::Union(ref m) if m.iter().any(|t|matches!(t,RuntimeTy::Null)))
            {
                code.push_str("(*(map.as_raw() as *mut baml_rt_gc::Header)).aux=rt::telemetry::capture::OPTIONAL_ELEMENT_TYPE;");
            }
            for i in 0..entries.len() {
                write!(code,"{};",e.fallible(&format!("map.set(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw(k{i}),v{i})"))).unwrap();
            }
            code.push_str("map.as_raw()}");
            Ok((code, expected))
        })())
    }
}
fn map_call(e: &Emitter<'_, '_>, name: &str, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
    let Repr::MapStrPtr(class) = values[0].1 else {
        unreachable!()
    };
    let map = format!("rt::regex_map::StringPtrMap::from_raw({})", values[0].0);
    let mut args = vec!["&mut rt::strings::Cx::current()".to_owned()];
    let (method, output) = match name {
        "baml.Map.get" | "baml.Map.has" if values.len() == 2 => {
            let key = e.coerce(values[1].clone(), Repr::Str)?;
            args.push(format!("rt::strings::Str::from_raw({key})"));
            if name == "baml.Map.get" {
                ("get", Repr::Ptr(class))
            } else {
                ("has", Repr::Bool)
            }
        }
        "baml.Map.keys" if values.len() == 1 => {
            args.push(e.lists[&Repr::ListStr].to_string());
            ("keys", Repr::ListStr)
        }
        "baml.Map.length" if values.len() == 1 => return Ok((format!("{map}.len()"), Repr::Int)),
        "baml.Map.set" if values.len() == 3 => {
            let key = e.coerce(values[1].clone(), Repr::Str)?;
            let value = e.coerce(values[2].clone(), Repr::Ptr(class))?;
            args.extend([format!("rt::strings::Str::from_raw({key})"), value]);
            ("set", Repr::Ptr(class))
        }
        _ => return Err(no("named regex map operation")),
    };
    Ok((
        e.fallible(&format!("{map}.{method}({})", args.join(","))),
        output,
    ))
}
pub(super) fn emit(
    out: &mut String,
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    builtins: &BTreeSet<String>,
) -> Gen<()> {
    if !builtins
        .iter()
        .any(|name| name == "baml.regex._compile" || name.starts_with("baml.regex.Regex."))
    {
        return Ok(());
    }
    let regex = super::strings::builtin_class(plan, "baml.regex.Regex")?;
    if !plan.layouts.contains_key(&regex) {
        return Ok(());
    }
    let class = |name: &str| -> Gen<String> {
        let c = super::strings::builtin_class(plan, name)?;
        let layout = plan
            .layouts
            .get(&c)
            .ok_or_else(|| no(format!("regex layout {name}")))?;
        Ok(format!(
            "rt::regex::ClassLayout{{desc:{},size:{},fields:&{:?}}}",
            descriptors[&c], layout.bytes, layout.offsets
        ))
    };
    let group = super::strings::builtin_class(plan, "baml.regex.Group")?;
    let found = super::strings::builtin_class(plan, "baml.regex.Match")?;
    let error = super::strings::builtin_class(plan, "baml.regex.Error")?;
    let Repr::Enum(kind) = plan.layouts[&error].fields[0] else {
        return Err(no("regex error kind"));
    };
    let Object::Enum(kind) = &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(kind)]
    else {
        unreachable!()
    };
    let kinds = kind
        .variants
        .iter()
        .map(|v| v.name.as_str())
        .collect::<Vec<_>>();
    writeln!(out,"static REGEX_SCHEMA:rt::regex::Schema=rt::regex::Schema{{regex:{},group:{},found:{},error:{},error_kinds:&{kinds:?},panic_desc:{},groups_desc:{},matches_desc:{},named_desc:{}}};",class("baml.regex.Regex")?,class("baml.regex.Group")?,class("baml.regex.Match")?,class("baml.regex.Error")?,descriptors[&super::strings::builtin_class(plan,"baml.panics.UserPanic")?],lists[&Repr::ListPtr(group)],lists[&Repr::ListPtr(found)],lists[&Repr::MapStrPtr(group)]).unwrap();
    Ok(())
}
