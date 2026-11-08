//! Recursive JSON schemas used when a specialization contains lists.
use super::*;
pub(super) const TYPE_BASE: usize = 1_000_000;
impl Plan<'_> {
    pub(super) fn template_repr(&self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<Repr> {
        self.repr(&ty.substitute_symbolic(&[]))
    }
}
fn slot(repr: Repr) -> &'static str {
    if repr.pointer() {
        "Pointer"
    } else if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
        "Tagged"
    } else {
        "Scalar"
    }
}
struct SchemaEmitter<'a, 'p> {
    plan: &'a Plan<'p>,
    descriptors: &'a BTreeMap<usize, usize>,
    lists: &'a BTreeMap<Repr, usize>,
    aliased: bool,
    code: usize,
    classes: BTreeSet<usize>,
    output: String,
}
impl SchemaEmitter<'_, '_> {
    fn class(&mut self, c: usize) -> Gen<String> {
        let symbol = format!("JSON_LIST_{}_C{c}_{}", self.code, usize::from(self.aliased));
        if !self.classes.insert(c) {
            return Ok(symbol);
        }
        let class = self.plan.class(c);
        // Preserve the existing closed-world decoder's admission for custom overrides.
        for package in &self.plan.program.packages {
            for rules in package.impl_rules.values() {
                for rule in rules {
                    if rule.methods.contains_key("from_json")
                        && matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Class(head,_) if head.tag()==class.type_tag)
                    {
                        return Err(no("JSON class with custom interface implementations"));
                    }
                }
            }
        }
        let layout = &self.plan.layouts[&c];
        let mut fields = String::new();
        let field_types = self.plan.class_field_types(c);
        for (i, f) in class.fields.iter().enumerate() {
            let ty = self.field(&field_types[i])?;
            write!(
                &mut fields,
                "rt::json_lists::Field{{name:{:?},offset:{},ty:&{ty},slot:SlotKind::{},skip:{}}},",
                if self.aliased {
                    f.alias.as_ref().unwrap_or(&f.name)
                } else {
                    &f.name
                },
                layout.offsets[i],
                slot(layout.fields[i]),
                self.aliased && f.skip
            )
            .unwrap();
        }
        writeln!(&mut self.output,"static {symbol}:rt::json_lists::Ty=rt::json_lists::Ty::Class{{desc:{},bytes:{},name:{:?},fields:&[{fields}]}};",self.descriptors[&c],layout.bytes,class.path.to_string()).unwrap();
        Ok(symbol)
    }
    fn list(&mut self, repr: Repr, element: String) -> Gen<String> {
        Ok(format!(
            "rt::json_lists::Ty::List{{desc:{},element:&{element},slot:SlotKind::{}}}",
            self.lists[&repr],
            slot(self.plan.element(repr)?)
        ))
    }
    fn map(&mut self, repr: Repr, value: String) -> Gen<String> {
        let (key, element) = self.plan.map_parts(repr)?;
        if key != Repr::Str {
            return Err(no("JSON list map key type"));
        }
        let desc = self.lists[&repr];
        let element_slot = maps::slot_type(element, self.descriptors, self.lists)?;
        let convert = match element {
            Repr::Tagged | Repr::Dyn(_) => "value".into(),
            Repr::Int => "value.payload as i64".into(),
            Repr::Float => "f64::from_bits(value.payload)".into(),
            Repr::Bool => "value.payload!=0".into(),
            Repr::Null => "rt::maps::Null".into(),
            r if r.pointer() => format!("<{element_slot}>::from_raw(value.payload as usize)"),
            _ => return Err(no("JSON list map value type")),
        };
        let map = format!("rt::maps::Map::<rt::strings::Str,{element_slot}>");
        Ok(format!(
            "rt::json_lists::Ty::Map{{desc:{desc},value:&{value},new:|cx|unsafe{{{map}::new(cx,{desc}).map(|m|m.as_raw())}},insert:|cx,p,key,value|unsafe{{let key=cx.string(key)?;{map}::from_raw(p).set(cx,key,{convert}).map(|_|())}}}}"
        ))
    }
    fn template(&mut self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
        use baml_compiler2_mir::TyTemplate as T;
        Ok(match ty {
            T::Int => "rt::json_lists::Ty::Int".into(),
            T::Float => "rt::json_lists::Ty::Float".into(),
            T::Bool => "rt::json_lists::Ty::Bool".into(),
            T::Null => "rt::json_lists::Ty::Null".into(),
            T::String => "rt::json_lists::Ty::String".into(),
            T::Literal(baml_type::Literal::String(s), _) => {
                format!("rt::json_lists::Ty::Literal({:?})", s.as_str())
            }
            T::Literal(baml_type::Literal::Int(v), _) => {
                format!("rt::json_lists::Ty::LiteralInt({v})")
            }
            T::Literal(baml_type::Literal::Bool(v), _) => {
                format!("rt::json_lists::Ty::LiteralBool({v})")
            }
            T::Literal(baml_type::Literal::Float(v), _) => format!(
                "rt::json_lists::Ty::LiteralFloat(f64::from_bits({}))",
                v.parse::<f64>()
                    .map_err(|_| no("JSON float literal"))?
                    .to_bits()
            ),
            T::List(t) => {
                let element = self.template(t)?;
                self.list(self.plan.template_repr(ty)?, element)?
            }
            T::Map { value, .. } => {
                let value = self.template(value)?;
                self.map(self.plan.template_repr(ty)?, value)?
            }
            T::Class(_, _) => {
                let Repr::Ptr(class) = self.plan.template_repr(ty)? else {
                    return Err(no("JSON list class layout"));
                };
                self.class(class)?
            }
            T::Union(m) => {
                let members = m
                    .iter()
                    .map(|m| {
                        Ok(format!(
                            "rt::json_lists::Member{{name:{:?},ty:&{}}}",
                            template_name(self.plan, m)?,
                            self.template(m)?
                        ))
                    })
                    .collect::<Gen<Vec<_>>>()?;
                format!("rt::json_lists::Ty::Union(&[{}])", members.join(","))
            }
            _ => return Err(no("JSON list element")),
        })
    }
    fn field(&mut self, ty: &bex_vm_types::RuntimeTy) -> Gen<String> {
        use bex_vm_types::RuntimeTy as T;
        Ok(match ty {
            T::Int => "rt::json_lists::Ty::Int".into(),
            T::Float => "rt::json_lists::Ty::Float".into(),
            T::Bool => "rt::json_lists::Ty::Bool".into(),
            T::Null => "rt::json_lists::Ty::Null".into(),
            T::String => "rt::json_lists::Ty::String".into(),
            T::Literal(baml_type::Literal::String(s), _) => {
                format!("rt::json_lists::Ty::Literal({:?})", s.as_str())
            }
            T::Literal(baml_type::Literal::Int(v), _) => {
                format!("rt::json_lists::Ty::LiteralInt({v})")
            }
            T::Literal(baml_type::Literal::Bool(v), _) => {
                format!("rt::json_lists::Ty::LiteralBool({v})")
            }
            T::Literal(baml_type::Literal::Float(v), _) => format!(
                "rt::json_lists::Ty::LiteralFloat(f64::from_bits({}))",
                v.parse::<f64>()
                    .map_err(|_| no("JSON float literal"))?
                    .to_bits()
            ),
            T::List(t) => {
                let element = self.field(t)?;
                self.list(self.plan.field_repr(ty)?, element)?
            }
            T::Map { value, .. } => {
                let value = self.field(value)?;
                self.map(self.plan.field_repr(ty)?, value)?
            }
            T::Class(_, _) => {
                let Repr::Ptr(c) = self.plan.field_repr(ty)? else {
                    unreachable!()
                };
                self.class(c)?
            }
            T::Union(m) => {
                let members = m
                    .iter()
                    .map(|m| {
                        Ok(format!(
                            "rt::json_lists::Member{{name:{:?},ty:&{}}}",
                            m.to_string(),
                            self.field(m)?
                        ))
                    })
                    .collect::<Gen<Vec<_>>>()?;
                format!("rt::json_lists::Ty::Union(&[{}])", members.join(","))
            }
            _ => return Err(no("JSON list field element")),
        })
    }
}
pub(super) fn emit(
    out: &mut String,
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    ty: Option<&baml_compiler2_mir::TyTemplate>,
    classes: &[usize],
    code: usize,
    aliased: bool,
) -> Gen<()> {
    let mut e = SchemaEmitter {
        plan,
        descriptors,
        lists,
        aliased,
        code,
        classes: BTreeSet::new(),
        output: String::new(),
    };
    let root = if let Some(ty) = ty {
        e.template(ty)?
    } else if classes.len() == 1 {
        e.class(classes[0])?
    } else {
        let members = classes
            .iter()
            .map(|c| {
                let class = plan.class(*c);
                Ok(format!(
                    "rt::json_lists::Member{{name:{:?},ty:&{}}}",
                    class.path.to_string(),
                    e.class(*c)?
                ))
            })
            .collect::<Gen<Vec<_>>>()?;
        format!("rt::json_lists::Ty::Union(&[{}])", members.join(","))
    };
    out.push_str(&e.output);
    let mode = usize::from(aliased);
    writeln!(out,"static JSON_LIST_ROOT_{code}_{mode}:rt::json_lists::Ty={root};\nunsafe fn json_specialized_{code}_{mode}(cx:&mut rt::strings::Cx,s:usize)->R<rt::Dyn>{{unsafe{{rt::json_lists::decode(cx,rt::strings::Str::from_raw(s),&JSON_LIST_ROOT_{code}_{mode})}}}}").unwrap();
    Ok(())
}

pub(super) fn capture_template(
    plan: &Plan<'_>,
    ty: &baml_compiler2_mir::TyTemplate,
) -> Gen<String> {
    use baml_compiler2_mir::TyTemplate as T;
    Ok(format!(
        "rt::telemetry::capture::Type::{}",
        match ty {
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => "Int".into(),
            T::Float | T::Literal(baml_type::Literal::Float(_), _) => "Float".into(),
            T::Bool | T::Literal(baml_type::Literal::Bool(_), _) => "Bool".into(),
            T::Null => "Null".into(),
            T::String | T::Literal(baml_type::Literal::String(_), _) => "String".into(),
            T::List(t) => format!("List(&{})", capture_template(plan, t)?),
            T::Map { key, value } => format!(
                "Map(&{},&{})",
                capture_template(plan, key)?,
                capture_template(plan, value)?
            ),
            T::Union(m) => format!(
                "Union(&[{}])",
                m.iter()
                    .map(|t| capture_template(plan, t))
                    .collect::<Gen<Vec<_>>>()?
                    .join(",")
            ),
            T::Class(_, _) => capture_codegen::capture_repr(plan, plan.template_repr(ty)?)?
                .strip_prefix("rt::telemetry::capture::Type::")
                .unwrap()
                .to_owned(),
            _ => return Err(no("JSON list capture type")),
        }
    ))
}

fn template_name(plan: &Plan<'_>, ty: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
    use baml_compiler2_mir::TyTemplate as T;
    Ok(match ty {
        T::Int => "int".into(),
        T::Float => "float".into(),
        T::Bool => "bool".into(),
        T::Null => "null".into(),
        T::String => "string".into(),
        T::Literal(v, _) => v.to_string(),
        T::List(t) => format!("{}[]", template_name(plan, t)?),
        T::Map { key, value } => format!(
            "map<{}, {}>",
            template_name(plan, key)?,
            template_name(plan, value)?
        ),
        T::Class(h, _) => {
            let c = plan.index.class_at(h).ok_or_else(|| no("JSON class"))?.0;
            let Object::Class(c) = &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(c)]
            else {
                unreachable!()
            };
            c.path.to_string()
        }
        T::Union(m) => m
            .iter()
            .map(|m| template_name(plan, m))
            .collect::<Gen<Vec<_>>>()?
            .join(" | "),
        _ => return Err(no("JSON list type name")),
    })
}

pub(super) fn emit_shapes(
    out: &mut String,
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<()> {
    fn tag(
        repr: Repr,
        descriptors: &BTreeMap<usize, usize>,
        lists: &BTreeMap<Repr, usize>,
    ) -> Gen<usize> {
        Ok(match repr {
            Repr::Int => 0,
            Repr::Bool => 1,
            Repr::Null => 2,
            Repr::Str => 3,
            Repr::Float => u32::MAX as usize,
            Repr::Ptr(c) => descriptors[&c] + 3,
            r if r.growing() || matches!(r, Repr::MapStrInt | Repr::Map(_)) => lists[&r] + 3,
            Repr::Tagged | Repr::Dyn(_) => 0,
            _ => return Err(no("list JSON scalar tag")),
        })
    }
    for package in &plan.program.packages {
        for rules in package.impl_rules.values() {
            for rule in rules {
                if rule.methods.contains_key("to_json")
                    && matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Class(head,_) if plan.layouts.keys().any(|c|plan.class(*c).type_tag==head.tag()))
                {
                    return Err(no("list JSON ToJson override"));
                }
            }
        }
    }
    let last = lists
        .iter()
        .map(|(r, id)| {
            id + if matches!(r, Repr::MapStrInt | Repr::Map(_)) {
                4
            } else {
                2
            }
        })
        .max()
        .unwrap_or_else(|| {
            descriptors
                .values()
                .copied()
                .max()
                .map_or(rt_descriptors(), |id| (id + 1).max(rt_descriptors()))
        });
    out.push_str("static JSON_LIST_SHAPES:&[rt::json_lists::Shape]=&[\n");
    for desc in 0..last {
        if let Some((&class, _)) = descriptors.iter().find(|(_, id)| **id == desc) {
            let c = plan.class(class);
            let layout = &plan.layouts[&class];
            out.push_str("rt::json_lists::Shape::Class(&[");
            for (i, field) in c.fields.iter().enumerate() {
                let repr = layout.fields[i];
                writeln!(out,"rt::json_lists::EncodeField{{name:{:?},alias:{:?},skip:{},offset:{},tag:{},tagged:{}}},",field.name,field.alias.as_ref().unwrap_or(&field.name),field.skip,layout.offsets[i],tag(repr,descriptors,lists)?,matches!(repr,Repr::Tagged|Repr::Dyn(_))).unwrap();
            }
            out.push_str("]),\n");
        } else if let Some((&repr, _)) = lists.iter().find(|(_, id)| **id == desc) {
            if matches!(repr, Repr::MapStrInt | Repr::Map(_)) {
                let (key, value) = plan.map_parts(repr)?;
                let key_slot = maps::slot_type(key, descriptors, lists)?;
                let value_slot = maps::slot_type(value, descriptors, lists)?;
                let key_dyn = capture_codegen::capture_dyn(key, "key", descriptors, lists)?;
                let value_dyn = capture_codegen::capture_dyn(value, "value", descriptors, lists)?;
                writeln!(out,"rt::json_lists::Shape::Map{{string_keys:{},len:|p|unsafe{{rt::maps::Map::<{key_slot},{value_slot}>::from_raw(p).len()}},entry:|p,i|unsafe{{let (key,value)=rt::maps::Map::<{key_slot},{value_slot}>::from_raw(p).entry(i).unwrap();({key_dyn},{value_dyn})}}}},",key==Repr::Str).unwrap();
                continue;
            }
            let element = plan.element(repr)?;
            writeln!(
                out,
                "rt::json_lists::Shape::List{{tag:{},tagged:{}}},",
                tag(element, descriptors, lists)?,
                matches!(element, Repr::Tagged | Repr::Dyn(_))
            )
            .unwrap();
        } else {
            out.push_str("rt::json_lists::Shape::Unsupported,\n");
        }
    }
    out.push_str("];\n");
    Ok(())
}
