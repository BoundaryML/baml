use crate::generate::test_request::de::Deserializer;
use serde::de::{self};
use serde::Deserialize;
use serde_json::Value;

mod generate_python;
mod template;

#[derive(Deserialize)]
pub struct TestRequest {
    functions: Vec<TestFunction>,
}

#[derive(Deserialize)]
struct TestFunction {
    name: String,
    tests: Vec<Test>,
}

#[derive(Deserialize)]
struct Test {
    name: String,
    #[allow(unused)]
    impls: Vec<String>,
    params: TestParam,
}

enum TestParam {
    Positional(String),
    Keyword(Vec<(String, String)>),
}

impl<'de> Deserialize<'de> for TestParam {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawParam {
            #[serde(rename = "type")]
            param_type: String,
            value: Value,
        }

        let RawParam { param_type, value } = RawParam::deserialize(deserializer)?;

        match param_type.as_str() {
            "positional" => match value {
                Value::String(s) => Ok(TestParam::Positional(s)),
                _ => Err(de::Error::custom("Expected a string for positional type")),
            },
            "named" => match value {
                Value::Array(arr) => {
                    let mut params = Vec::new();
                    for val in arr {
                        if let Value::Object(obj) = val {
                            let name = obj
                                .get("name")
                                .and_then(Value::as_str)
                                .ok_or_else(|| de::Error::custom("Expected a string for name"))?
                                .to_owned();
                            let value = obj
                                .get("value")
                                .and_then(Value::as_str)
                                .ok_or_else(|| de::Error::custom("Expected a string for value"))?
                                .to_owned();
                            params.push((name, value));
                        } else {
                            return Err(de::Error::custom("Expected an object for named type"));
                        }
                    }
                    Ok(TestParam::Keyword(params))
                }
                _ => Err(de::Error::custom("Expected an array for named type")),
            },
            _ => Err(de::Error::unknown_field(
                &param_type,
                &["positional", "named"],
            )),
        }
    }
}
