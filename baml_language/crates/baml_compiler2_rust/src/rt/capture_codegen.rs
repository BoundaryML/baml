//! Freeze canonical declaration metadata at compile time. Generated packs only
//! carry owned CAS blobs; the VM dependency belongs to this compiler, not them.
use super::*;

pub(super) fn emit(
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    after_lists: usize,
) -> Gen<String> {
    let named = |name: &str| {
        plan.program
            .packages
            .iter()
            .find_map(|package| {
                package
                    .classes
                    .iter()
                    .chain(package.enums.iter())
                    .find_map(|(path, index)| {
                        (path.under(&package.name).to_string() == name).then_some(index.raw())
                    })
            })
            .ok_or_else(|| no(format!("capture declaration {name}")))
    };
    let runtime = [
        "baml.panics.IntegerOverflow",
        "baml.panics.NegativeBitShift",
        "baml.panics.Cancelled",
        "baml.panics.StackOverflow",
        "baml.panics.AllocFailure",
        "baml.panics.IndexOutOfBounds",
        "baml.json.ParseError",
        "baml.json.DecodeError",
        "baml.panics.DivisionByZero",
        "baml.json.SerializationError",
        "baml.errors.InvalidArgument",
    ]
    .map(named)
    .into_iter()
    .collect::<Gen<Vec<_>>>()?;
    let naming = named("baml.json.Naming")?;
    let objects: BTreeSet<_> = runtime
        .iter()
        .copied()
        .chain(descriptors.keys().copied())
        .chain([naming])
        .collect();
    let objects: Vec<_> = objects.into_iter().collect();
    let declarations =
        bex_vm::telemetry::frozen::declarations(plan.program, &objects).map_err(no)?;
    let mut out = String::new();
    let mut groups = BTreeMap::new();
    for (&object, declaration) in objects.iter().zip(&declarations) {
        let group = emit_group(&declaration.definition.group, &mut groups, &mut out);
        let name = borsh::to_vec(&declaration.name).expect("declaration name");
        writeln!(&mut out,"fn CAPTURE_D{object}()->&'static rt::telemetry::capture::Declaration{{static VALUE:std::sync::OnceLock<rt::telemetry::capture::Declaration>=std::sync::OnceLock::new();VALUE.get_or_init(||rt::telemetry::capture::Declaration::from_bytes(&{name:?},{},rt::telemetry::types::Definition{{group:CAPTURE_G{group}(),member:{}}},{}))}}",declaration.tag.as_i64(),declaration.definition.member,declaration.is_enum).unwrap();
    }
    out.push_str("static CAPTURE_SHAPES:&[rt::telemetry::capture::Shape]=&[rt::telemetry::capture::Shape::String,\n");
    for desc in 1..after_lists {
        let class = if desc < rt_descriptors() {
            Some(runtime[desc - 1])
        } else {
            descriptors
                .iter()
                .find_map(|(&object, &index)| (index == desc).then_some(object))
        };
        let shape = if let Some(class) = class {
            format!("Class{{declaration:CAPTURE_D{class},fields:DESCRIPTORS[{desc}].fields}}")
        } else if let Some((&repr, _)) = lists.iter().find(|(_, index)| **index == desc) {
            if repr == Repr::MapStrInt {
                "MapStringInt".to_owned()
            } else {
                let (tag, ty) = capture_list_element(plan, descriptors, lists, repr)?;
                format!(
                    "List{{growing:true,tag:{tag},element:&rt::telemetry::capture::Type::{ty}}}"
                )
            }
        } else {
            "Unsupported".to_owned()
        };
        writeln!(&mut out, "rt::telemetry::capture::Shape::{shape},").unwrap();
    }
    let declaration = &declarations[objects.iter().position(|&object| object == naming).unwrap()];
    writeln!(&mut out,"rt::telemetry::capture::Shape::Enum{{declaration:CAPTURE_D{naming},variants:&{:?}}},\n];\nconst CAPTURE_NAMING_TAG:u64={};\nstatic CAPTURE_METADATA:rt::telemetry::capture::Metadata=rt::telemetry::capture::Metadata{{shapes:CAPTURE_SHAPES,naming_tag:CAPTURE_NAMING_TAG}};",declaration.variants,after_lists+3).unwrap();
    Ok(out)
}
fn emit_group(
    group: &btel_types::DefinitionBlob,
    groups: &mut BTreeMap<[u8; 16], usize>,
    out: &mut String,
) -> usize {
    if let Some(&index) = groups.get(&group.id()) {
        return index;
    }
    let children = group
        .children()
        .iter()
        .map(|child| emit_group(child, groups, out))
        .collect::<Vec<_>>();
    let index = groups.len();
    groups.insert(group.id(), index);
    let children = children
        .into_iter()
        .map(|child| format!("CAPTURE_G{child}()"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(out,"fn CAPTURE_G{index}()->std::sync::Arc<rt::telemetry::types::DefinitionBlob>{{static VALUE:std::sync::OnceLock<std::sync::Arc<rt::telemetry::types::DefinitionBlob>>=std::sync::OnceLock::new();VALUE.get_or_init(||std::sync::Arc::new(rt::telemetry::types::DefinitionBlob::new({:?},vec!{:?}.into_boxed_slice(),vec![{children}].into_boxed_slice()))).clone()}}",group.id(),group.bytes()).unwrap();
    index
}

fn capture_element(
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    repr: Repr,
) -> Gen<(usize, String)> {
    Ok(match repr {
        Repr::Int => (0, "Int".into()),
        Repr::Bool => (1, "Bool".into()),
        Repr::Float => (u32::MAX as usize, "Float".into()),
        Repr::Str => (3, "String".into()),
        Repr::Ptr(c) => (descriptors[&c] + 3, format!("Class(CAPTURE_D{c})")),
        r if r.growing() => {
            let (_, element) = capture_list_element(plan, descriptors, lists, r)?;
            (
                lists[&r] + 3,
                format!("List(&rt::telemetry::capture::Type::{element})"),
            )
        }
        Repr::MapStrInt => (
            lists[&Repr::MapStrInt] + 3,
            "Map(&rt::telemetry::capture::Type::String,&rt::telemetry::capture::Type::Int)".into(),
        ),
        Repr::Tagged => (usize::MAX - 1, "Unknown".into()),
        Repr::Null => (2, "Null".into()),
        _ => return Err(no("capture list element")),
    })
}

fn capture_list_element(
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    repr: Repr,
) -> Gen<(usize, String)> {
    let (tag, mut ty) = capture_element(plan, descriptors, lists, plan.element(repr)?)?;
    if let Repr::List(id) = repr {
        let members = plan.list_elements.borrow()[id].members.clone();
        if !members.is_empty() {
            let types = members
                .into_iter()
                .map(|m| {
                    capture_element(plan, descriptors, lists, m)
                        .map(|(_, ty)| format!("rt::telemetry::capture::Type::{ty}"))
                })
                .collect::<Gen<Vec<_>>>()?;
            ty = if types.len() == 1 {
                types[0]
                    .strip_prefix("rt::telemetry::capture::Type::")
                    .unwrap()
                    .to_owned()
            } else {
                format!("Union(&[{}])", types.join(","))
            };
        }
    }
    Ok((tag, ty))
}
