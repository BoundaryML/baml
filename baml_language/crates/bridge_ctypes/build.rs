fn main() -> std::io::Result<()> {
    let proto_dir = std::path::Path::new("types");
    let protos = baml_proto_codegen::schemas(proto_dir)?;
    baml_proto_codegen::compile(&protos, &[proto_dir])
}
