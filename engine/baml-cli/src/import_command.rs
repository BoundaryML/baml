use std::{collections::HashMap, io::Write, path::PathBuf};

use baml_lib::{Configuration, ValidatedSchema};
use colored::Colorize;
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::errors::CliError;

/*
Content will be of the form:
{
  "version": "1",
  "content": {
    "type": "test",
    "functionName": "FooBar",
    "testName": "test_1",
    "input": {
      "name": "multi",
      "fields": { "takeaways": "List[str]", "title": "str", "author": "None" },
      "value": [
        "[\"Bay4 Energy is unique in that it provides full-service capabilities to manage renewable energy projects from development through the entire 25-30 year lifecycle.\", \"Bay4 Energy's services include performance management, contract management, financial management, independent operations and maintenance management, and engineering services.\", \"Bay4 Energy has a national presence and experience working with utilities across the United States.\", \"Bay4 reviews project designs before construction to identify potential concerns.\", \"Bay4 visits construction sites to ensure work follows manufacturer specifications, protecting future warranty rights.\", \"Bay4 recommends best practices based on experience to maintain systems long-term.\", \"The quote is from Evan Christenson, Managing Director of RC Energy Group.\"]",
        "\"Home: Asset Performance Management Renewable Energy Services - Bay4 Energy\"",
        "null"
      ]
    }
  }
}

or

{
  "version": "1",
  "content": {
    "type": "test",
    "testName": "test_1",
    "functionName": "FooBar",
    "input": {"name":"single","fields":{"document_text":"str"},"value":"\"Solar Asset Monitoring When choosing a solution for on-demand production, irradiance and weather data, solar stakeholders should consider several key factors: Is the solar data produced by an experienced staff of scientists, researchers & engineers with a deep bench of scientific expertise? When sourcing a solar asset performance and monitoring dataset solution, choose a solution that is supported by a broad team of experienced solar industry experts that incorporates the industry\\u2019s leading solar data. Does the solar data build investor confidence? A solar asset and performance monitoring solution should be proven to validate energy projections. It should successfully benchmark solar fleet performance with monthly summaries of expected asset performance based on available irradiance. Does the solar data improve field services & customer support? Solar stakeholders should embrace solutions that can keep field teams and customers informed by reliably integrating hourly solar performance estimates and weather data. Is the solar data accessible by diagnostic software? To scale solar asset monitoring, stakeholders should integrate solar data with diagnostic software. With integration, it\\u2019s possible to generate expected power output measurements of individual or fleets of PV systems\\u2014all at a lower cost and more accurately than using ground measurements. SolarAnywhere\\u00ae SystemCheck\\u00ae estimates distributed PV energy production in real time, giving owners & operators a critical performance benchmarking tool SolarAnywhere Data (Sites) offers the most bankable solar data for PV project financing & asset management\""}
  }
}
*/

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "version", content = "content")]
pub enum Test {
    #[serde(rename = "1")]
    V1(TestV1),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TestV1 {
    #[serde(rename = "type")]
    pub test_type: String,
    #[serde(rename = "testName")]
    pub name: Option<String>,
    #[serde(rename = "functionName")]
    pub function_name: String,
    #[serde(rename = "input")]
    pub input: TestInput,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "name")]
pub enum TestInput {
    #[serde(rename = "multi")]
    Multi {
        #[serde(rename = "fields")]
        fields: Value,
        #[serde(rename = "value")]
        value: Vec<String>,
    },
    #[serde(rename = "single")]
    Single {
        #[serde(rename = "fields")]
        fields: Value,
        #[serde(rename = "value")]
        value: String,
    },
}

pub fn run(
    content: &str,
    baml_dir: &PathBuf,
    _config: &Configuration,
    schema: ValidatedSchema,
) -> Result<(), CliError> {
    // Parse base64 encoded content
    let decoded = base64::decode(content)
        .map_err(|e| CliError::StringError(format!("Expected encoded string: {}\n", e)))?;

    // Parse content as JSON
    let test: Test = serde_json::from_slice(&decoded)
        .map_err(|e| CliError::StringError(format!("Invalid content: {}", e)))?;

    match test {
        Test::V1(v1) => {
            if let Some(fn_walker) = schema.db.find_function_by_name(&v1.function_name) {
                let test_case_input: Result<Value, CliError> = match v1.input {
                    TestInput::Single { fields, value } => {
                        let parsed = serde_json::from_str::<Value>(&value)?;
                        if fn_walker.is_positional_args() {
                            Ok(parsed)
                        } else {
                            let field_name = match &fields {
                                Value::Object(x) => {
                                    if x.len() != 1 {
                                        return Err(CliError::StringError(format!(
                                            "Invalid fields: {:?}",
                                            x
                                        )));
                                    }
                                    Ok(x.keys().next().unwrap())
                                }
                                item => Err(CliError::StringError(format!(
                                    "Invalid fields: {:?}",
                                    item
                                ))),
                            }?;

                            let mut map = HashMap::new();
                            map.insert(field_name, parsed);
                            Ok(serde_json::json!(map))
                        }
                    }
                    TestInput::Multi { fields, value } => {
                        let field_name = match &fields {
                            Value::Object(x) => Ok(x
                                .keys()
                                .into_iter()
                                .enumerate()
                                .map(|(i, x)| (x.to_string(), value[i].to_string()))
                                .collect::<Vec<(String, String)>>()),
                            item => {
                                Err(CliError::StringError(format!("Invalid fields: {:?}", item)))
                            }
                        }?;

                        let mut map = HashMap::new();
                        for (field_name, value) in field_name {
                            let parsed = serde_json::from_str::<Value>(&value)?;
                            map.insert(field_name, parsed);
                        }
                        Ok(serde_json::json!(map))
                    }
                };

                let test_case_content = json!({
                  "input": test_case_input?
                });

                // Ask the user for a name if not provided
                let test_name = match v1.name {
                    Some(name) => name,
                    None => {
                        let mut name = String::new();
                        println!("Enter a name for this test case:");
                        std::io::stdin().read_line(&mut name).map_err(|e| {
                            CliError::StringError(format!("Failed to read line: {}", e))
                        })?;
                        name.trim().to_string()
                    }
                };

                let target_dir = baml_dir.join("__tests__").join(&v1.function_name);
                let target_file = target_dir.join(format!("{}.json", test_name));

                // write to file
                std::fs::create_dir_all(target_dir).map_err(|e| {
                    CliError::StringError(format!("Failed to create directory: {}", e))
                })?; // create directory if it doesn't exist
                let mut file = std::fs::File::create(&target_file)
                    .map_err(|e| CliError::StringError(format!("Failed to create file: {}", e)))?;
                file.write_all(serde_json::to_string_pretty(&test_case_content)?.as_bytes())
                    .map_err(|e| CliError::StringError(format!("Failed to write file: {}", e)))?;

                info!("Created test case: {}", target_file.display());

                println!(
                    "{}\nbaml test run -i \"{}::{}\"",
                    "To run this test:".dimmed(),
                    v1.function_name,
                    test_name
                );

                Ok(())
            } else {
                Err(CliError::StringError(format!(
                    "Function not found: {}",
                    v1.function_name
                )))
            }
        }
    }
}
