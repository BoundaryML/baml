mod class;
mod r#enum;
mod field_type;
mod function;
mod template;
mod ts_language_features;

use crate::configuration::Generator;

use super::{dir_writer::WithFileContent, ir::IntermediateRepr};
use ts_language_features::get_file_collector;

pub(crate) fn generate_ts(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
    let mut collector = get_file_collector();

    ir.enums.iter().for_each(|e| e.write(&mut collector));
    ir.classes.iter().for_each(|c| c.write(&mut collector));
    ir.functions.iter().for_each(|f| f.write(&mut collector));

    collector.commit(&gen.output)
}
