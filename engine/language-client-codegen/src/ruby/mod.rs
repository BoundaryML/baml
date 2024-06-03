mod expression;
mod field_type;
mod generate_types;
mod ruby_language_features;

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Result;
use indexmap::IndexMap;
use ruby_language_features::ToRuby;

use either::Either;

use internal_baml_core::ir::repr::IntermediateRepr;

use crate::dir_writer::FileCollector;

use generate_types::ToTypeReferenceInTypeDefinition;
use ruby_language_features::RubyLanguageFeatures;

#[derive(askama::Template)]
#[template(path = "client.rb.j2", escape = "none")]
struct RubyClient {
    funcs: Vec<RubyFunction>,
}
struct RubyFunction {
    name: String,
    partial_return_type: String,
    return_type: String,
    args: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "inlined.rb.j2", escape = "none")]
struct InlinedBaml {
    file_map: Vec<(String, String)>,
}

pub(crate) fn generate(
    ir: &IntermediateRepr,
    generator: &crate::GeneratorArgs,
) -> Result<IndexMap<PathBuf, String>> {
    let mut collector = FileCollector::<RubyLanguageFeatures>::new();

    collector
        .add_template::<generate_types::RubyStreamTypes>("partial-types.rb", (ir, generator))?;
    collector.add_template::<generate_types::RubyTypes>("types.rb", (ir, generator))?;
    collector.add_template::<RubyClient>("client.rb", (ir, generator))?;
    collector.add_template::<InlinedBaml>("inlined.rb", (ir, generator))?;

    collector.commit(&generator.output_dir())
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'ir crate::GeneratorArgs)> for RubyClient {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'ir IntermediateRepr, &'ir crate::GeneratorArgs)) -> Result<Self> {
        let functions = ir
            .walk_functions()
            .map(|f| {
                let Either::Right(configs) = f.walk_impls() else {
                    return Ok(vec![]);
                };
                let funcs = configs
                    .map(|c| {
                        let (_function, _impl_) = c.item;
                        Ok(RubyFunction {
                            name: f.name().to_string(),
                            partial_return_type: f.elem().output().to_partial_type_ref(),
                            return_type: f.elem().output().to_ruby(),
                            args: match f.inputs() {
                                either::Either::Left(_args) => anyhow::bail!("Ruby codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_ruby()))
                                    .collect(),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<RubyFunction>>>>()?
            .into_iter()
            .flatten().collect();
        Ok(RubyClient { funcs: functions })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for InlinedBaml {
    type Error = anyhow::Error;

    fn try_from((_ir, args): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(InlinedBaml {
            file_map: args.file_map()?,
        })
    }
}
