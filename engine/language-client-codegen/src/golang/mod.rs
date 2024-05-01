mod expression;
mod field_type;
mod generate_types;
mod golang_language_features;

use std::path::Path;

use anyhow::Result;
use askama::Template;
use golang_language_features::ToGolang;

use either::Either;

use internal_baml_core::ir::repr::IntermediateRepr;

use crate::dir_writer::FileCollector;

use self::golang_language_features::GolangLanguageFeatures;

#[derive(askama::Template)]
#[template(path = "client.rb.j2", escape = "none")]
struct GolangClient {
    funcs: Vec<GolangFunction>,
}
struct GolangFunction {
    name: String,
    return_type: String,
    args: Vec<(String, String)>,
}

pub(crate) fn generate(ir: &IntermediateRepr, project_root: &Path) -> Result<()> {
    let mut collector = FileCollector::<GolangLanguageFeatures>::new();

    collector.add_file(
        "types",
        TryInto::<generate_types::GolangTypes>::try_into(ir)
            .map_err(|e| e.context("Error while building types.go"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "/*\n\n{:?}\n\n*/",
                        anyhow::Error::new(e).context("Error while rendering types.go")
                    )
                },
                |r| r,
            ),
    );

    collector.add_file(
        "client",
        TryInto::<generate_types::GolangTypes>::try_into(ir)
            .map_err(|e| e.context("Error while building client.go"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "/*\n\n{:?}\n\n*/",
                        anyhow::Error::new(e).context("Error while rendering client.go")
                    )
                },
                |r| r,
            ),
    );

    collector.commit(project_root)?;

    Ok(())
}

impl TryFrom<&IntermediateRepr> for GolangClient {
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
                        Ok(GolangFunction {
                            name: f.name().to_string(),
                            return_type: f.elem().output().to_golang(),
                            args: match f.inputs() {
                                either::Either::Left(args) => anyhow::bail!("Golang codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_golang()))
                                    .collect(),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<GolangFunction>>>>()?
            .into_iter()
            .flatten().collect();
        Ok(GolangClient { funcs: functions })
    }
}
