//! Per-type direct decoders for arrays of flat class records.
use super::*;
fn scalar(ty: &bex_vm_types::RuntimeTy) -> Option<&'static str> {
    use bex_vm_types::RuntimeTy as T;
    Some(match ty {
        T::String => "String",
        T::Int => "Int",
        T::Float => "Float",
        T::Bool => "Bool",
        T::Union(m)
            if m.len() == 2
                && m.iter().any(|t| matches!(t, T::Int))
                && m.iter().any(|t| matches!(t, T::Float)) =>
        {
            "NumberUnion"
        }
        T::Union(m) if m.len() == 2 && m.iter().any(|t| matches!(t, T::Null)) => {
            match m.iter().find(|t| !matches!(t, T::Null)).unwrap() {
                T::Int => "OptionalInt",
                T::String => "OptionalString",
                _ => return None,
            }
        }
        _ => return None,
    })
}
pub(super) fn emit(
    out: &mut String,
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    ty: Option<&baml_compiler2_mir::TyTemplate>,
    code: usize,
    aliased: bool,
) -> Gen<bool> {
    use baml_compiler2_mir::TyTemplate as T;
    let Some(ty @ T::List(element)) = ty else {
        return Ok(false);
    };
    let T::Class(head, args) = element.as_ref() else {
        return Ok(false);
    };
    if !args.is_empty() {
        return Ok(false);
    }
    let class = plan
        .index
        .class_at(head)
        .ok_or_else(|| no("flat JSON class"))?
        .0;
    let definition = plan.class(class);
    if definition.fields.len() > 32 || definition.fields.iter().any(|f| aliased && f.skip) {
        return Ok(false);
    }
    let types = plan
        .class_field_types(class)
        .iter()
        .map(scalar)
        .collect::<Option<Vec<_>>>();
    let Some(types) = types else { return Ok(false) };
    let mut names = BTreeSet::new();
    let mut keys = String::new();
    let mut checks = Vec::new();
    let mut stores = String::new();
    for (i, (field, ty)) in definition.fields.iter().zip(types).enumerate() {
        let name = if aliased {
            field.alias.as_ref().unwrap_or(&field.name)
        } else {
            &field.name
        };
        if !names.insert(name) {
            return Ok(false);
        }
        write!(keys, "{name:?}=>Some({i}),").unwrap();
        checks.push(format!(
            "input.matches({i},rt::json_lists::flat::Scalar::{ty})"
        ));
        writeln!(
            stores,
            "input.store(cx,p,{}, {i},rt::json_lists::flat::Scalar::{ty})?;",
            plan.layouts[&class].offsets[i]
        )
        .unwrap();
    }
    let checks = if checks.is_empty() {
        "true".into()
    } else {
        checks.join("&&")
    };
    let mode = usize::from(aliased);
    let list = lists[&plan.template_repr(ty)?];
    let object = descriptors[&class];
    let bytes = plan.layouts[&class].bytes;
    let n = definition.fields.len();
    writeln!(out,"unsafe fn json_specialized_{code}_{mode}(cx:&mut rt::strings::Cx,s:usize)->R<rt::Dyn>{{unsafe{{let read=rt::strings::Cx::current();let text=rt::strings::Str::from_raw(s).try_as_str(&read)?;let fast=(||->R<Option<rt::Dyn>>{{let mut syntax=rt::json::Reader::new(text);if syntax.peek()!=Some(b'['){{return Ok(None)}}syntax.open_array().unwrap();let mut count=0;loop{{match syntax.next_element(count==0){{Ok(false)=>break,Ok(true)=>{{if syntax.skip_value().is_err(){{return Err(rt::json::syntax_error(cx,text))}}count+=1;}},Err(_)=>return Err(rt::json::syntax_error(cx,text)),}}}}if syntax.finish().is_err(){{return Err(rt::json::syntax_error(cx,text))}}let list=rt::lists::List::<rt::lists::Ptr>::with_capacity(cx,{list},count)?;let mut reader=rt::json::Reader::new(text);reader.open_array().unwrap();let mut i=0;while reader.next_element(i==0).unwrap(){{let Ok(mut input)=rt::json_lists::flat::Object::<{n}>::scan(reader,|key|match key{{{keys}_=>None}})else{{return Ok(None)}};if !({checks}){{return Ok(None)}}let p=rt::try_alloc({object},{bytes})?;{stores}list.push(cx,rt::lists::Ptr::from_raw(p as usize))?;reader=input.into_reader();i+=1;if i&255==0{{cx.poll()?;}}}}Ok(Some(rt::Dyn{{tag:{list}+3,payload:list.as_raw() as u64}}))}})()?;if let Some(value)=fast{{Ok(value)}}else{{rt::json_lists::decode(cx,rt::strings::Str::from_raw(s),&JSON_LIST_ROOT_{code}_{mode})}}}}}}").unwrap();
    Ok(true)
}
