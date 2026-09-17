function hello_world() -> string {
    "Hello World"
}

function nested_payload(workload: string, payload_size: int) -> baml.json.json {
    {
        "workload": workload,
        "nested": { "level1": { "level2": { "payload": "x".repeat(payload_size) } } },
    }
}

function json_64k() -> baml.json.json {
    nested_payload("json-64k", 65467)
}

function json_1m() -> baml.json.json {
    nested_payload("json-1m", 1048508)
}

function image_base64(value: image) -> string {
    let path = value.file() ?? baml.sys.panic("expected an image created with Image.from_file()");
    baml.fs.open(path, "r").bytes().to_base64()
}

function force_gc() -> null {
    baml.sys.collect_garbage()
}
