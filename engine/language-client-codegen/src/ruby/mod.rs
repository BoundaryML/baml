mod expression;
mod field_type;
mod generate_types;
mod ruby_language_features;

use std::path::Path;

use anyhow::Result;
use askama::Template;
use ruby_language_features::ToRuby;

use either::Either;

use internal_baml_core::ir::repr::IntermediateRepr;

use crate::dir_writer::FileCollector;

use self::ruby_language_features::RubyLanguageFeatures;

#[derive(askama::Template)]
#[template(path = "client.rb.j2", escape = "none")]
struct RubyClient {
    funcs: Vec<RubyFunction>,
}
struct RubyFunction {
    name: String,
    return_type: String,
    args: Vec<(String, String)>,
}

pub(crate) fn generate(
    ir: &IntermediateRepr,
    generator: &crate::GeneratorInstructions,
) -> Result<()> {
    let mut collector = FileCollector::<RubyLanguageFeatures>::new();

    collector.add_file(
        "types.rb",
        TryInto::<generate_types::RubyTypes>::try_into(ir)
            .map_err(|e| e.context("Error while building types.rb"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "=begin\n{:?}\n=end",
                        anyhow::Error::new(e).context("Error while rendering types.rb")
                    )
                },
                |r| r,
            ),
    );

    collector.add_file(
        "client.rb",
        TryInto::<RubyClient>::try_into(ir)
            .map_err(|e| e.context("Error while building client.rb"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "=begin\n{:?}\n=end",
                        anyhow::Error::new(e).context("Error while rendering client.rb")
                    )
                },
                |r| r,
            ),
    );

    collector.commit(&generator.project_root)?;

    Ok(())
}

impl TryFrom<&IntermediateRepr> for RubyClient {
    type Error = anyhow::Error;

    fn try_from(ir: &IntermediateRepr) -> Result<Self> {
        let functions = ir
            .walk_functions()
            .map(|f| {
                let Either::Right(configs) = f.walk_impls() else {
                    return Ok(vec![]);
                };
                let funcs = configs
                    .map(|c| {
                        let (function, impl_) = c.item;
                        Ok(RubyFunction {
                            name: f.name().to_string(),
                            return_type: f.elem().output().to_ruby(),
                            args: match f.inputs() {
                                either::Either::Left(args) => anyhow::bail!("Ruby codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
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
