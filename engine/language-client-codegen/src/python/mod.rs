mod expression;
mod field_type;
mod generate_types;
mod python_language_features;

use std::path::Path;

use anyhow::Result;
use askama::Template;
use python_language_features::ToPython;

use either::Either;

use internal_baml_core::ir::repr::IntermediateRepr;

use crate::dir_writer::FileCollector;

use self::python_language_features::PythonLanguageFeatures;

#[derive(askama::Template)]
#[template(path = "client.py.j2", escape = "none")]
struct PythonClient {
    funcs: Vec<PythonFunction>,
}
struct PythonFunction {
    name: String,
    return_type: String,
    args: Vec<(String, String)>,
}

pub(crate) fn generate(ir: &IntermediateRepr, project_root: &Path) -> Result<()> {
    let mut collector = FileCollector::<PythonLanguageFeatures>::new();

    collector.add_file(
        "types.py",
        TryInto::<generate_types::PythonTypes>::try_into(ir)
            .map_err(|e| e.context("Error while building types.py"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "/*\n\n{:?}\n\n*/",
                        anyhow::Error::new(e).context("Error while rendering types.py")
                    )
                },
                |r| r,
            ),
    );

    collector.add_file(
        "client.py",
        TryInto::<PythonClient>::try_into(ir)
            .map_err(|e| e.context("Error while building client.py"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "/*\n\n{:?}\n\n*/",
                        anyhow::Error::new(e).context("Error while rendering client.py")
                    )
                },
                |r| r,
            ),
    );

    collector.add_file(
        "baml.lock",
        serde_json::to_string(&serde_json::json!({
            "version": 2,
            "content": {
                "cli_version": "0.20.0-canary.0",
                "client_version": null,
            }
        }))?,
    );

    collector.commit(project_root)?;

    Ok(())
}

impl TryFrom<&IntermediateRepr> for PythonClient {
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
                        Ok(PythonFunction {
                            name: f.name().to_string(),
                            return_type: f.elem().output().to_python(),
                            args: match f.inputs() {
                                either::Either::Left(args) => anyhow::bail!("Python codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_python()))
                                    .collect(),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<PythonFunction>>>>()?
            .into_iter()
            .flatten().collect();
        Ok(PythonClient { funcs: functions })
    }
}
