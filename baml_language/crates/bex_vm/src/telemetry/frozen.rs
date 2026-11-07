//! Compile-time export of the canonical capture declaration graph.
use super::definitions;
/// Canonical declaration metadata for native producers. This is a compilation
/// operation: it binds a private immutable image but executes no BAML code.
/// The returned definitions contain only owned names and CAS blobs.
pub struct FrozenDeclaration {
    pub name: baml_type::DeclarationName,
    pub tag: baml_type::typetag::TypeTag,
    pub definition: btel_types::Definition,
    pub is_enum: bool,
    pub variants: Vec<String>,
}
pub fn declarations(
    program: &bex_vm_types::Program,
    objects: &[usize],
) -> Result<Vec<FrozenDeclaration>, String> {
    program.validate().map_err(|error| error.to_string())?;
    for &index in objects {
        if !matches!(
            program.objects.get(index),
            Some(bex_vm_types::Object::Class(_) | bex_vm_types::Object::Enum(_))
        ) {
            return Err(format!("object {index} is not a capture declaration"));
        }
    }
    let mut image: Vec<_> = program.objects.iter().cloned().collect();
    bex_vm_types::types::box_compile_time_floats(&mut image, &program.globals);
    crate::prepare_compact_code(&mut image, &program.globals);
    let (heap, _packages) =
        crate::package_load::build_heap_with_packages(image, &program.packages, program.root);
    Ok(objects
        .iter()
        .map(|&index| {
            let ptr = heap.compile_time_ptr(index);
            // The private image has no mutator or collector; compile-time objects
            // remain pinned until all metadata has been copied out.
            let (name, tag, is_enum, variants) = match unsafe { ptr.get() } {
                bex_vm_types::Object::Class(class) => (class.name(), class.type_tag, false, vec![]),
                bex_vm_types::Object::Enum(enm) => (
                    enm.name(),
                    enm.type_tag,
                    true,
                    enm.variants.iter().map(|v| v.name.clone()).collect(),
                ),
                _ => panic!("capture declaration must be a class or enum"),
            };
            let definition = unsafe { definitions::of(ptr) }
                .expect("declaration")
                .clone();
            FrozenDeclaration {
                name,
                tag,
                definition,
                is_enum,
                variants,
            }
        })
        .collect())
}
