use internal_baml_parser_database::walkers::ClientWalker;
use internal_baml_schema_ast::ast::{WithDocumentation, WithName};

use serde::Serialize;
use serde_json::json;

use crate::generate::generate_base::traits::WithToObject;

use super::file::clean_file_name;

use super::template::render_template;
use super::traits::{TargetLanguage, WithFileName};
use super::{file::File, traits::JsonHelper};

#[derive(Serialize)]
struct ClientJson<'a> {
    name: &'a str,
    kwargs: ClientKwargs<'a>,
    options: Vec<KV>,
    doc_string: Option<&'a str>,
}

#[derive(Serialize)]
struct KV {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct ClientKwargs<'a> {
    provider: String,
    retry_policy: &'a str,
    redactions: Vec<String>,
}

impl JsonHelper for ClientWalker<'_> {
    fn json(&self, f: &mut File, lang: TargetLanguage) -> serde_json::Value {
        let props = self.properties();
        let opts = props
            .options
            .iter()
            .map(|(k, v)| KV {
                key: k.clone(),
                value: v.to_object(f, lang),
            })
            .collect::<Vec<_>>();

        let retry_policy = props
            .retry_policy
            .as_ref()
            .map(|(policy, _)| {
                f.add_import("..configs.retry_policy", &policy);
                policy.as_str()
            })
            .unwrap_or("None");

        let redactions = props
            .options
            .iter()
            .filter_map(|(k, v)| {
                if v.is_env_expression() {
                    return Some(format!("\"{}\"", k));
                }
                None
            })
            .collect::<Vec<_>>();

        json!(ClientJson {
            name: self.name(),
            kwargs: ClientKwargs {
                provider: format!("\"{}\"", props.provider.0),
                retry_policy,
                redactions: redactions,
            },
            options: opts,
            doc_string: self.ast_client().documentation(),
        })
    }
}

impl WithFileName for ClientWalker<'_> {
    fn file_name(&self) -> String {
        format!("client_{}", clean_file_name(self.name()))
    }

    fn to_py_file(&self, fc: &mut super::file::FileCollector) {
        fc.start_py_file("clients", "__init__");
        fc.complete_file();

        fc.start_py_file("clients", self.file_name());
        let json = self.json(fc.last_file(), TargetLanguage::Python);
        render_template(
            TargetLanguage::Python,
            super::template::HSTemplate::Client,
            fc.last_file(),
            json,
        );
        fc.complete_file();
    }

    fn to_ts_file(&self, fc: &mut super::file::FileCollector) {
        todo!()
    }
}
