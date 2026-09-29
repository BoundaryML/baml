fn main() -> std::io::Result<()> {
    baml_proto_codegen::compile(&["proto/recording.proto"], &["proto"])
}
