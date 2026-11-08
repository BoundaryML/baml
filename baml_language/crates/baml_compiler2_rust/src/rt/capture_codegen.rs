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
        "baml.errors.InvalidArgument",
        "baml.errors.ParseError",
        "baml.panics.HostUnavailable",
        "baml.errors.Io",
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
                let element = Emitter::element(repr)?;
                let (tag, ty) = match element {
                    Repr::Int => (0, "Int".to_owned()),
                    Repr::Str => (3, "String".to_owned()),
                    Repr::Ptr(class) => {
                        (descriptors[&class] + 3, format!("Class(CAPTURE_D{class})"))
                    }
                    Repr::ListStr => (
                        lists[&Repr::ListStr] + 3,
                        "List(&rt::telemetry::capture::Type::String)".to_owned(),
                    ),
                    _ => return Err(no("capture list type")),
                };
                format!(
                    "List{{growing:{},tag:{tag},element:&rt::telemetry::capture::Type::{ty}}}",
                    repr.growing()
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
