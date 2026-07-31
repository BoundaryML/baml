use bex_project::query::{
    DiffRequest, FileId, FunctionDictionary, FunctionIdentity, HttpFile, HttpRangeResponse,
    LeftHeavyRequest, ObservePoll, SandwichRequest, SearchRequest, Viewport,
};
use js_sys::{Object, Reflect, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = ObserveEngine)]
pub struct WasmObserveEngine {
    inner: bex_project::query::ObserveEngine,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRegistrationWire {
    file: String,
    url: String,
    committed_len: u64,
    generation: u64,
    validator: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeResponseWire {
    file: String,
    generation: u64,
    start: u64,
    end_exclusive: u64,
    total_len: u64,
    validator: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunFilesWire {
    files: Vec<String>,
    partition_id: Option<u32>,
    request_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineWire {
    #[serde(flatten)]
    run: RunFilesWire,
    start_ns: u64,
    end_ns: u64,
    pixel_width: u32,
    lanes: u16,
    max_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeftHeavyWire {
    #[serde(flatten)]
    run: RunFilesWire,
    pixel_width: u32,
    max_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SandwichWire {
    #[serde(flatten)]
    run: RunFilesWire,
    function_id: u32,
    caller_depth: u16,
    callee_depth: u16,
    max_rows: usize,
    max_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityWire {
    function_id: u32,
    definition_key: String,
    fqn: String,
    def_content_hash: String,
}

impl IdentityWire {
    fn into_identity(self) -> Result<FunctionIdentity, JsValue> {
        Ok(FunctionIdentity {
            function_id: self.function_id,
            definition_key: self.definition_key,
            fqn: self.fqn,
            def_content_hash: decode_hash(&self.def_content_hash)?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchWire {
    #[serde(flatten)]
    run: RunFilesWire,
    text: String,
    max_rows: usize,
    max_bytes: usize,
    dictionary: Vec<IdentityWire>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffWire {
    left_files: Vec<String>,
    left_partition_id: Option<u32>,
    left_dictionary: Vec<IdentityWire>,
    right_files: Vec<String>,
    right_partition_id: Option<u32>,
    right_dictionary: Vec<IdentityWire>,
    max_rows: usize,
    max_bytes: usize,
    request_id: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeRequestWire {
    file: String,
    url: String,
    start: u64,
    end_exclusive: u64,
    range_header: String,
    if_range: Option<String>,
    generation: u64,
}

#[wasm_bindgen]
impl WasmObserveEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(cache_bytes: usize, max_range_bytes: u64) -> Self {
        Self {
            inner: bex_project::query::ObserveEngine::new(cache_bytes, max_range_bytes),
        }
    }

    #[wasm_bindgen(js_name = registerFile)]
    pub fn register_file(&self, manifest_json: &str) -> Result<(), JsValue> {
        let wire: FileRegistrationWire = serde_json::from_str(manifest_json).map_err(js_error)?;
        self.inner
            .register_file(HttpFile {
                file: parse_file(&wire.file)?,
                url: wire.url,
                committed_len: wire.committed_len,
                generation: wire.generation,
                validator: wire.validator,
            })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = supplyRange)]
    pub fn supply_range(&self, response_json: &str, body: &[u8]) -> Result<(), JsValue> {
        let wire: RangeResponseWire = serde_json::from_str(response_json).map_err(js_error)?;
        self.inner
            .supply_range(HttpRangeResponse {
                file: parse_file(&wire.file)?,
                generation: wire.generation,
                start: wire.start,
                end_exclusive: wire.end_exclusive,
                total_len: wire.total_len,
                validator: wire.validator,
                body: body.to_vec(),
            })
            .map_err(js_error)
    }

    pub fn timeline(&self, request_json: &str) -> Result<JsValue, JsValue> {
        let request: TimelineWire = serde_json::from_str(request_json).map_err(js_error)?;
        poll_to_js(
            self.inner
                .timeline(
                    &parse_files(&request.run.files)?,
                    request.run.partition_id,
                    Viewport {
                        start_ns: request.start_ns,
                        end_ns: request.end_ns,
                        pixel_width: request.pixel_width,
                        lanes: request.lanes,
                        max_bytes: request.max_bytes,
                    },
                    request.run.request_id,
                )
                .map_err(js_error)?,
        )
    }

    #[wasm_bindgen(js_name = leftHeavy)]
    pub fn left_heavy(&self, request_json: &str) -> Result<JsValue, JsValue> {
        let request: LeftHeavyWire = serde_json::from_str(request_json).map_err(js_error)?;
        poll_to_js(
            self.inner
                .left_heavy(
                    &parse_files(&request.run.files)?,
                    request.run.partition_id,
                    LeftHeavyRequest {
                        pixel_width: request.pixel_width,
                        max_bytes: request.max_bytes,
                    },
                    request.run.request_id,
                )
                .map_err(js_error)?,
        )
    }

    pub fn sandwich(&self, request_json: &str) -> Result<JsValue, JsValue> {
        let request: SandwichWire = serde_json::from_str(request_json).map_err(js_error)?;
        poll_to_js(
            self.inner
                .sandwich(
                    &parse_files(&request.run.files)?,
                    request.run.partition_id,
                    SandwichRequest {
                        function_id: request.function_id,
                        caller_depth: request.caller_depth,
                        callee_depth: request.callee_depth,
                        max_rows: request.max_rows,
                        max_bytes: request.max_bytes,
                    },
                    request.run.request_id,
                )
                .map_err(js_error)?,
        )
    }

    pub fn search(&self, request_json: &str) -> Result<JsValue, JsValue> {
        let request: SearchWire = serde_json::from_str(request_json).map_err(js_error)?;
        let dictionary = dictionary(request.dictionary)?;
        poll_to_js(
            self.inner
                .search(
                    &parse_files(&request.run.files)?,
                    request.run.partition_id,
                    &dictionary,
                    &SearchRequest {
                        text: request.text,
                        max_rows: request.max_rows,
                        max_bytes: request.max_bytes,
                    },
                    request.run.request_id,
                )
                .map_err(js_error)?,
        )
    }

    pub fn diff(&self, request_json: &str) -> Result<JsValue, JsValue> {
        let request: DiffWire = serde_json::from_str(request_json).map_err(js_error)?;
        poll_to_js(
            self.inner
                .diff(
                    &parse_files(&request.left_files)?,
                    request.left_partition_id,
                    &dictionary(request.left_dictionary)?,
                    &parse_files(&request.right_files)?,
                    request.right_partition_id,
                    &dictionary(request.right_dictionary)?,
                    DiffRequest {
                        max_rows: request.max_rows,
                        max_bytes: request.max_bytes,
                    },
                    request.request_id,
                )
                .map_err(js_error)?,
        )
    }

    #[wasm_bindgen(js_name = explainBql)]
    pub fn explain_bql(&self, bql: &str) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner.explain_bql(bql).map_err(js_error)?).map_err(js_error)
    }

    #[wasm_bindgen(getter, js_name = retainedBytes)]
    pub fn retained_bytes(&self) -> usize {
        self.inner.retained_bytes()
    }
}

fn dictionary(values: Vec<IdentityWire>) -> Result<FunctionDictionary, JsValue> {
    Ok(FunctionDictionary {
        functions: values
            .into_iter()
            .map(IdentityWire::into_identity)
            .collect::<Result<_, _>>()?,
    })
}

fn parse_files(files: &[String]) -> Result<Vec<FileId>, JsValue> {
    files.iter().map(|file| parse_file(file)).collect()
}

fn parse_file(file: &str) -> Result<FileId, JsValue> {
    file.parse::<u64>()
        .map(FileId)
        .map_err(|_| JsValue::from_str("file id must be an unsigned decimal string"))
}

fn decode_hash(hash: &str) -> Result<[u8; 32], JsValue> {
    if hash.len() != 64 {
        return Err(JsValue::from_str(
            "defContentHash must contain 64 hexadecimal characters",
        ));
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hash[index * 2..index * 2 + 2], 16)
            .map_err(|_| JsValue::from_str("defContentHash contains invalid hexadecimal"))?;
    }
    Ok(output)
}

fn poll_to_js(poll: ObservePoll) -> Result<JsValue, JsValue> {
    let object = Object::new();
    match poll {
        ObservePoll::Frame(bytes) => {
            Reflect::set(
                &object,
                &JsValue::from_str("kind"),
                &JsValue::from_str("frame"),
            )?;
            Reflect::set(
                &object,
                &JsValue::from_str("frame"),
                &Uint8Array::from(bytes.as_slice()),
            )?;
        }
        ObservePoll::NeedData { requests } => {
            Reflect::set(
                &object,
                &JsValue::from_str("kind"),
                &JsValue::from_str("needData"),
            )?;
            let requests = requests
                .into_iter()
                .map(|request| RangeRequestWire {
                    file: request.file.0.to_string(),
                    url: request.url,
                    start: request.start,
                    end_exclusive: request.end_exclusive,
                    range_header: request.range_header,
                    if_range: request.if_range,
                    generation: request.generation,
                })
                .collect::<Vec<_>>();
            Reflect::set(
                &object,
                &JsValue::from_str("requests"),
                &serde_wasm_bindgen::to_value(&requests).map_err(js_error)?,
            )?;
        }
    }
    Ok(object.into())
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}
