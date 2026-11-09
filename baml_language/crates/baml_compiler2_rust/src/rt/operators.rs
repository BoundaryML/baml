//! Primitive operator implementations (`baml.ops.<Op><Rhs>$for$<Self>.<method>`)
//! reached through generic code, and the erased `__union_*` operator drivers.
//! Both lower to the exact operations the operator syntax uses.
use super::*;

/// `(interface, method)` of a primitive operator implementation path.
fn implementation(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("baml.ops.")?;
    let (interface, method) = rest.rsplit_once('.')?;
    interface.contains("$for$").then_some((interface, method))
}

fn symbol(method: &str) -> Option<&'static str> {
    Some(match method {
        "add" => "+",
        "sub" => "-",
        "mul" => "*",
        "div" => "/",
        "rem" => "%",
        "shl" => "<<",
        "shr" => ">>",
        "bit_and" => "&",
        "bit_or" => "|",
        "bit_xor" => "^",
        _ => return None,
    })
}

fn union_symbol(name: &str) -> Option<&'static str> {
    Some(match name.strip_prefix("baml.ops.__union_")? {
        "add" => "+",
        "sub" => "-",
        "mul" => "*",
        "div" => "/",
        "rem" => "%",
        "neg" => "neg",
        _ => return None,
    })
}

/// A fallible native body: errors raised inside it carry the native frame
/// the interpreter reports above the caller's frame.
pub(super) fn native_fallible(e: &Emitter<'_, '_>, name: &str, code: &str) -> Gen<String> {
    // An erased adapter of the native itself has no caller frame to attach.
    if !matches!(e.all[&e.object].kind, MirFunctionKind::Bytecode(_)) {
        return Ok(e.fallible(code));
    }
    let native = e
        .plan
        .program
        .objects
        .iter()
        .enumerate()
        .find_map(|(i, o)| {
            (matches!(o, Object::Function(_)) && e.plan.index.builtin_path(i) == name).then_some(i)
        })
        .ok_or_else(|| no("operator native"))?;
    Ok(e.fallible(&format!(
        "({code}).map_err(|mut error|{{rt::telemetry::evidence::raised(&mut error,&S{},{},Some(TELEMETRY_INDEX_{native}));error}})",
        e.object, e.site
    )))
}

const STRING_ADD: &str = "baml.<(string as baml.ops.Add<string>)>.add";

/// `string + string` through the erased driver calls the BAML-bodied string
/// implementation, which the interpreter records as an ordinary call.
fn string_add(plan: &Plan<'_>) -> Option<usize> {
    plan.program.objects.iter().enumerate().find_map(|(i, o)| {
        (matches!(o, Object::Function(_)) && plan.index.display(i) == STRING_ADD).then_some(i)
    })
}

pub(super) fn native(name: &str) -> bool {
    implementation(name)
        .is_some_and(|(_, method)| symbol(method).is_some() || matches!(method, "eq" | "neg"))
        || union_symbol(name).is_some()
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
        // The native itself is reached so its error frames have a telemetry
        // function identity.
        let name = plan.index.builtin_path(id);
        native(name).then(|| {
            let mut targets = vec![id];
            if name == "baml.ops.__union_add"
                && let Some(add) = string_add(plan)
            {
                targets.push(add);
            }
            Ok(registry::Reach {
                targets,
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
        if let Some(op) = union_symbol(name) {
            return Some(union(e, name, op, values));
        }
        let (_, method) = implementation(name)?;
        if !native(name) {
            return None;
        }
        Some(primitive(e, name, method, values))
    }
}

fn primitive(
    e: &Emitter<'_, '_>,
    name: &str,
    method: &str,
    values: &[(String, Repr)],
) -> Gen<(String, Repr)> {
    let tagged = |v: &(String, Repr)| e.coerce(v.clone(), Repr::Tagged);
    if method == "neg" {
        let [(v, repr)] = values else {
            return Err(no("negation arity"));
        };
        return Ok(match repr {
            Repr::Int => (
                native_fallible(e, name, &format!("rt::sub(0,{v})"))?,
                Repr::Int,
            ),
            Repr::Float => (format!("(-({v}))"), Repr::Float),
            Repr::Bigint => (
                e.fallible(&format!(
                    "rt::native::bigint(-rt::native::bigint_owned({v}))"
                )),
                Repr::Bigint,
            ),
            _ => return Err(no("negation operand")),
        });
    }
    let [l, r] = values else {
        return Err(no("operator arity"));
    };
    if method == "eq" {
        return Ok((
            match (l.1, r.1) {
                (Repr::Int | Repr::Bool, _) => format!("(({})==({}))", l.0, r.0),
                (Repr::Float, _) => format!("rt::float_eq({},{})", l.0, r.0),
                (Repr::Str, _) => e.fallible(&format!(
                    "rt::strings::equals_raw(&rt::strings::Cx::current(),{},{})",
                    l.0, r.0
                )),
                (Repr::Bigint, _) => e.fallible(&format!(
                    "rt::native::compare({},{},\"==\")",
                    tagged(l)?,
                    tagged(r)?
                )),
                (Repr::Bytes, _) => format!(
                    "(rt::native::bytes_owned({})==rt::native::bytes_owned({}))",
                    l.0, r.0
                ),
                _ => return Err(no("Equals operand")),
            },
            Repr::Bool,
        ));
    }
    let op = symbol(method).ok_or_else(|| no("operator method"))?;
    if l.1 == Repr::Bigint || r.1 == Repr::Bigint {
        return Ok((
            native_fallible(
                e,
                name,
                &format!("rt::native::binary({},{},{op:?})", tagged(l)?, tagged(r)?),
            )?,
            Repr::Bigint,
        ));
    }
    match (l.1, r.1) {
        (Repr::Int, Repr::Int) => Ok(match method {
            "bit_and" | "bit_or" | "bit_xor" => (format!("(({}) {op} ({}))", l.0, r.0), Repr::Int),
            _ => (
                e.fallible(&format!("rt::{method}({},{})", l.0, r.0)),
                Repr::Int,
            ),
        }),
        (Repr::Int | Repr::Float, Repr::Int | Repr::Float)
            if matches!(method, "add" | "sub" | "mul" | "div" | "rem") =>
        {
            Ok((
                format!("(({}) as f64 {op} ({}) as f64)", l.0, r.0),
                Repr::Float,
            ))
        }
        _ => Err(no("operator operands")),
    }
}

/// The erased drivers select the implementation from the operands' runtime
/// types. A class or enum implementing the operator would need dispatch to
/// its BAML body; programs that could reach one are rejected.
fn union(
    e: &Emitter<'_, '_>,
    name: &str,
    op: &str,
    values: &[(String, Repr)],
) -> Gen<(String, Repr)> {
    let interface = match op {
        "+" => "Add",
        "-" => "Subtract",
        "*" => "Multiply",
        "/" => "Divide",
        "%" => "Remainder",
        _ => "Negate",
    };
    let mut tags = BTreeSet::new();
    for &class in e.plan.layouts.keys() {
        tags.insert(e.plan.class(class).type_tag);
    }
    for &enumeration in &e.plan.enums {
        if let Object::Enum(en) =
            &e.plan.program.objects[bex_vm_types::ObjectIndex::from_raw(enumeration)]
        {
            tags.insert(en.type_tag);
        }
    }
    for package in &e.plan.program.packages {
        for (index, rules) in &package.impl_rules {
            let Object::Interface(def) = &e.plan.program.objects[*index] else {
                continue;
            };
            if def.path.to_string() != format!("ops.{interface}") {
                continue;
            }
            if rules.iter().any(|rule| match &rule.for_ty_pattern {
                bex_vm_types::TyTemplate::Class(head, _) | bex_vm_types::TyTemplate::Enum(head) => {
                    tags.contains(&head.tag())
                }
                _ => false,
            }) {
                return Err(no("erased operator with a class implementation"));
            }
        }
    }
    let bigint = if e.plan.bigints {
        "Some(BIGINT_TAG)"
    } else {
        "None"
    };
    let operands = values
        .iter()
        .map(|v| e.coerce(v.clone(), Repr::Tagged))
        .collect::<Gen<Vec<_>>>()?;
    let right = operands
        .get(1)
        .cloned()
        .unwrap_or_else(|| "rt::Dyn{tag:2,payload:0}".into());
    let erased = native_fallible(
        e,
        name,
        &format!("rt::operators::erased(l,r,{op:?},{bigint})"),
    )?;
    let strings = match string_add(e.plan) {
        Some(add) if op == "+" && matches!(e.all[&e.object].kind, MirFunctionKind::Bytecode(_)) => {
            let call = e.fallible(&format!(
                "f{add}::<OBSERVE>(cx,{}, l.payload as usize, r.payload as usize)",
                e.telemetry_site(add)
            ));
            format!(
                "if l.tag==3&&r.tag==3&&l.payload!=0&&r.payload!=0{{rt::Dyn{{tag:3,payload:{call} as u64}}}}else "
            )
        }
        _ => String::new(),
    };
    Ok((
        format!(
            "{{let l={};let r={right};{strings}{{{erased}}}}}",
            operands[0]
        ),
        Repr::Tagged,
    ))
}
