use super::*;

#[test]
fn test_signature() {
    const BAML_SRC: &str = include_str!("../../test_data/test.baml");
    let db = ParserDatabase::new(BamlSource::new(BamlSourceType::String(BAML_SRC.to_string())));
    let ir = BamlIr::new(db);
    let signature = ir.signature();
    println!("{}", signature);
}