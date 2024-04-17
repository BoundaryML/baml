// mod class;
// mod client;
// mod r#enum;
// mod expression;
// mod field_type;
// mod function;
// mod r#impl;
// mod intermediate_repr;
// mod python_language_features;
// mod template;

// use crate::configuration::Generator;

// use super::{
//     dir_writer::WithFileContentPy as WithFileContent,
//     ir::{IntermediateRepr, WithJsonSchema},
// };

// use python_language_features::get_file_collector;

// pub(crate) fn generate_python(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
//     let mut collector = get_file_collector();

//     ir.walk_enums().for_each(|e| e.write(&mut collector));
//     ir.walk_classes().for_each(|c| c.write(&mut collector));
//     ir.walk_functions().for_each(|f| f.write(&mut collector));
//     ir.walk_functions().for_each(|f| {
//         f.walk_impls().for_each(|i| {
//             i.write(&mut collector);
//         })
//     });

//     ir.walk_clients().for_each(|c| c.write(&mut collector));
//     // TODO: walk retry policies? or configs?

//     collector.commit(&gen.output_path)
// }
