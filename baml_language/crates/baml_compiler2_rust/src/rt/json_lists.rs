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
    aliases: BTreeSet<baml_type::typetag::TypeTag>,
    output: String,
}
impl SchemaEmitter<'_, '_> {
    fn alias(&mut self, tag: baml_type::typetag::TypeTag) -> Gen<String> {
        let (id, alias) = self
            .plan
            .program
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| match o {
                Object::TypeAlias(a) if a.type_tag == tag => Some((i, a.clone())),
                _ => None,
            })
            .ok_or_else(|| no("unlinked JSON alias"))?;
        if self
            .plan
            .program
            .packages
            .iter()
            .any(|p| p.name == "baml" && p.objects.contains(&(id as u32)))
            && alias.path.to_string() == "json.json"
        {
            if !self.plan.json_tree {
                return Err(no("JSON alias decoder configuration"));
            }
            return Ok("rt::json_lists::Ty::Json(&JSON_CONFIG)".into());
        }
        let name = format!(
            "JSON_LIST_ALIAS_{}_{}_{}",
            self.code,
            tag.as_i64() as u64,
            usize::from(self.aliased)
        );
        if self.aliases.insert(tag) {
            let definition = self.field(&bex_vm_types::RuntimeTy::from(alias.definition))?;
            writeln!(
                self.output,
                "static {name}:rt::json_lists::Ty={definition};"
            )
            .unwrap();
        }
        Ok(name)
    }
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
    fn template(&mut self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
        use baml_compiler2_mir::TyTemplate as T;
        Ok(match ty {
            T::Enum(_) | T::EnumVariant(..) => map_json::enum_schema(
                self.plan,
                self.plan.template_repr(ty)?,
                if let T::EnumVariant(_, name) = ty {
                    Some(name)
                } else {
                    None
                },
                self.aliased,
                self.descriptors,
                self.lists,
            )?,
            T::TypeAlias(_) => {
                let Some(bex_vm_types::TyTemplate::TypeAlias(head)) = self.plan.index.anchor(ty)
                else {
                    return Err(no("unlinked JSON alias"));
                };
                self.alias(head.tag())?
            }
            T::Int => "rt::json_lists::Ty::Int".into(),
            T::Bigint => format!(
                "rt::json_lists::Ty::Bigint{{desc:{}}}",
                self.plan.bigint_descriptor()
            ),
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
            T::Map { value, .. } => {
                let repr = self.plan.template_repr(ty)?;
                let value = self.template(value)?;
                map_json::schema(self.plan, repr, &value, self.descriptors, self.lists)?
            }
            T::List(t) => {
                let element = self.template(t)?;
                self.list(self.plan.template_repr(ty)?, element)?
            }
            T::Class(..) => {
                let Repr::Ptr(class) = self.plan.template_repr(ty)? else {
                    return Err(no("JSON class representation"));
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
            T::Enum(_) | T::EnumVariant(..) => map_json::enum_schema(
                self.plan,
                self.plan.field_repr(ty)?,
                if let T::EnumVariant(_, name) = ty {
                    Some(name)
                } else {
                    None
                },
                self.aliased,
                self.descriptors,
                self.lists,
            )?,
            T::TypeAlias(head) => self.alias(head.tag())?,
            T::Int => "rt::json_lists::Ty::Int".into(),
            T::Bigint => format!(
                "rt::json_lists::Ty::Bigint{{desc:{}}}",
                self.plan.bigint_descriptor()
            ),
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
            T::Map { value, .. } => {
                let repr = self.plan.field_repr(ty)?;
                let value = self.field(value)?;
                map_json::schema(self.plan, repr, &value, self.descriptors, self.lists)?
            }
            T::List(t) => {
                let element = self.field(t)?;
                self.list(self.plan.field_repr(ty)?, element)?
            }
            T::Class(..) => {
                let Repr::Ptr(class) = self.plan.field_repr(ty)? else {
                    return Err(no("JSON class representation"));
                };
                self.class(class)?
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
        aliases: BTreeSet::new(),
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
    writeln!(
        out,
        "static JSON_LIST_ROOT_{code}_{mode}:rt::json_lists::Ty={root};"
    )
    .unwrap();
    if !super::json_flat_lists::emit(out, plan, descriptors, lists, ty, code, aliased)? {
        writeln!(out,"unsafe fn json_specialized_{code}_{mode}(cx:&mut rt::strings::Cx,s:usize)->R<rt::Dyn>{{unsafe{{rt::json_lists::decode(cx,rt::strings::Str::from_raw(s),&JSON_LIST_ROOT_{code}_{mode})}}}}").unwrap();
    }
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
            T::TypeAlias(_) =>
                return capture_codegen::capture_type(plan, &ty.substitute_symbolic(&[])),
            T::Enum(_) | T::EnumVariant(..) =>
                return capture_codegen::capture_type(plan, &ty.substitute_symbolic(&[])),
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => "Int".into(),
            T::Bigint => "Bigint".into(),
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
            T::Class(..) =>
                return capture_codegen::capture_type(plan, &ty.substitute_symbolic(&[])),
            _ => return Err(no("JSON list capture type")),
        }
    ))
}

fn template_name(plan: &Plan<'_>, ty: &baml_compiler2_mir::TyTemplate) -> Gen<String> {
    use baml_compiler2_mir::TyTemplate as T;
    Ok(match ty {
        T::Enum(_) | T::EnumVariant(..) => {
            let id = map_json::enum_id(plan, plan.template_repr(ty)?)?;
            let Object::Enum(definition) =
                &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(id)]
            else {
                unreachable!()
            };
            if let T::EnumVariant(_, name) = ty {
                format!("{}.{name}", definition.path)
            } else {
                definition.path.to_string()
            }
        }
        T::Int => "int".into(),
        T::Bigint => "bigint".into(),
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
        plan: &Plan<'_>,
        repr: Repr,
        descriptors: &BTreeMap<usize, usize>,
        lists: &BTreeMap<Repr, usize>,
    ) -> Gen<usize> {
        Ok(match repr {
            Repr::Int => 0,
            Repr::Bool => 1,
            Repr::Null => 2,
            Repr::Str => 3,
            Repr::Bytes => 19,
            Repr::Opaque => 18,
            Repr::Closure => plan.closure_descriptors().unwrap().1 + 3,
            Repr::Future => plan.future_descriptor().unwrap() + 3,
            r @ (Repr::Enum(_) | Repr::Naming) => layout::enum_tag(plan, r, descriptors, lists),
            Repr::Bigint => plan.bigint_descriptor() + 3,
            Repr::Float => (u32::MAX - 1) as usize,
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
    let last = plan.bigint_descriptor() + usize::from(plan.bigints);
    for (&class, layout) in &plan.layouts {
        let definition = plan.class(class);
        writeln!(out,"unsafe fn json_encode_class_{class}(encoder:&mut rt::json_lists::Encoder<'_>,p:usize,depth:usize)->R<()>{{unsafe{{encoder.begin_class();let mut first=true;").unwrap();
        for (i, field) in definition.fields.iter().enumerate() {
            let repr = layout.fields[i];
            let value = if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
                format!("*((p+{}) as *const rt::Dyn)", layout.offsets[i])
            } else {
                format!(
                    "rt::Dyn{{tag:{},payload:*((p+{}) as *const u64)}}",
                    tag(plan, repr, descriptors, lists)?,
                    layout.offsets[i]
                )
            };
            let alias = field.alias.as_ref().unwrap_or(&field.name);
            writeln!(
                out,
                "encoder.class_field({:?},{:?},{:?},{:?},{},{value},&mut first,depth)?;",
                field.name,
                alias,
                serde_json::to_string(&field.name).unwrap(),
                serde_json::to_string(alias).unwrap(),
                field.skip
            )
            .unwrap();
        }
        out.push_str("encoder.finish_class(first,depth);Ok(())}}\n");
    }
    out.push_str("static JSON_LIST_SHAPES:&[rt::json_lists::Shape]=&[\n");
    for desc in 0..last {
        if plan.bigints && desc == plan.bigint_descriptor() {
            out.push_str("rt::json_lists::Shape::Bigint,\n");
        } else if let Some((&class, _)) = descriptors.iter().find(|(_, id)| **id == desc) {
            writeln!(
                out,
                "rt::json_lists::Shape::ClassEncoded(json_encode_class_{class}),"
            )
            .unwrap();
        } else if let Some((&repr, _)) = lists.iter().find(|(_, id)| **id == desc) {
            if matches!(repr, Repr::MapStrInt | Repr::Map(_)) {
                let (key, value) = plan.map_parts(repr)?;
                let k = maps::slot_type(key, descriptors, lists)?;
                let v = maps::slot_type(value, descriptors, lists)?;
                let key_ty = if let Repr::Map(id) = repr {
                    plan.map_types.borrow()[id].key_type.clone()
                } else {
                    "rt::telemetry::capture::Type::String".into()
                };
                let kd = capture_codegen::capture_dyn(key, "key", descriptors, lists)?;
                let vd = capture_codegen::capture_dyn(value, "value", descriptors, lists)?;
                writeln!(out,"rt::json_lists::Shape::Map{{key:&{key_ty},len:|p|unsafe{{rt::maps::Map::<{k},{v}>::from_raw(p).len()}},entry:|p,i|unsafe{{let(key,value)=rt::maps::Map::<{k},{v}>::from_raw(p).entry(i).unwrap();({kd},{vd})}}}},").unwrap();
                continue;
            }
            let element = plan.element(repr)?;
            writeln!(
                out,
                "rt::json_lists::Shape::List{{tag:{},tagged:{}}},",
                tag(plan, element, descriptors, lists)?,
                matches!(element, Repr::Tagged | Repr::Dyn(_))
            )
            .unwrap();
        } else if desc == 16 {
            out.push_str("rt::json_lists::Shape::Bytes,\n");
        } else if plan.closure_descriptors().is_some_and(|(_, c)| c == desc) {
            out.push_str("rt::json_lists::Shape::Function,\n");
        } else if plan.future_descriptor() == Some(desc) {
            out.push_str("rt::json_lists::Shape::Future,\n");
        } else {
            out.push_str("rt::json_lists::Shape::Unsupported,\n");
        }
    }
    let naming = plan
        .program
        .packages
        .iter()
        .find_map(|p| {
            p.enums.iter().find_map(|(path, id)| {
                (path.under(&p.name).to_string() == "baml.json.Naming").then_some(id.raw())
            })
        })
        .ok_or_else(|| no("JSON naming enum"))?;
    for id in std::iter::once(naming).chain(plan.enums.iter().copied()) {
        let Object::Enum(e) = &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(id)] else {
            unreachable!()
        };
        let name = plan
            .program
            .packages
            .iter()
            .find_map(|p| {
                p.enums
                    .iter()
                    .find_map(|(path, i)| (i.raw() == id).then(|| path.under(&p.name).to_string()))
            })
            .unwrap();
        write!(
            out,
            "rt::json_lists::Shape::Enum{{name:{name:?},variants:&["
        )
        .unwrap();
        for v in &e.variants {
            write!(
                out,
                "rt::json_lists::EnumVariant{{name:{:?},alias:{:?},skip:{}}},",
                v.name,
                v.alias.as_ref().unwrap_or(&v.name),
                v.skip
            )
            .unwrap();
        }
        out.push_str("]},\n");
    }
    out.push_str("];\n");
    Ok(())
}

// A recursive alias can hide containers which do not appear in a MIR local.
// Instantiate their descriptors before layout emission, once per declaration.
pub(super) fn record_schema(plan: &mut Plan<'_>, ty: &baml_compiler2_mir::TyTemplate) -> Gen<()> {
    fn walk(
        plan: &mut Plan<'_>,
        ty: &bex_vm_types::RuntimeTy,
        aliases: &mut BTreeSet<baml_type::typetag::TypeTag>,
        classes: &mut BTreeSet<usize>,
    ) -> Gen<()> {
        use bex_vm_types::RuntimeTy as T;
        plan.register_field_class_type(ty)?;
        plan.record_field_ty(ty)?;
        match ty {
            T::TypeAlias(head) if aliases.insert(head.tag()) => {
                let (id, alias) = plan
                    .program
                    .objects
                    .iter()
                    .enumerate()
                    .find_map(|(i, o)| match o {
                        Object::TypeAlias(a) if a.type_tag == head.tag() => Some((i, a.clone())),
                        _ => None,
                    })
                    .ok_or_else(|| no("JSON alias declaration"))?;
                if alias.path.to_string() == "json.json"
                    && plan
                        .program
                        .packages
                        .iter()
                        .any(|p| p.name == "baml" && p.objects.contains(&(id as u32)))
                {
                    super::json_tree::prepare(plan)?;
                } else {
                    walk(plan, &T::from(alias.definition), aliases, classes)?;
                }
            }
            T::Class(..) => {
                if let Repr::Ptr(c) = plan.field_repr(ty)? {
                    if classes.insert(c) {
                        for field in plan.class_field_types(c) {
                            walk(plan, &field, aliases, classes)?;
                        }
                    }
                }
            }
            T::List(t) => walk(plan, t, aliases, classes)?,
            T::Map { key, value } => {
                walk(plan, key, aliases, classes)?;
                walk(plan, value, aliases, classes)?;
            }
            T::Union(m) => {
                for t in m {
                    walk(plan, t, aliases, classes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    plan.register_class_type(&ty.substitute_symbolic(&[]))?;
    let anchored = plan
        .index
        .anchor(ty)
        .ok_or_else(|| no("JSON schema declarations"))?;
    walk(
        plan,
        &anchored.substitute_symbolic(&[]),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
}
