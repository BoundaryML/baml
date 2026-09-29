fn main() -> std::io::Result<()> {
    baml_proto_codegen::compile(&["src/value/proto/bamlvalue.proto"], &["src/value/proto"])
}
