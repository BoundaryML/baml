use super::{BTreeSet, Gen, no};

pub(super) fn emit(out: &mut String, builtins: &BTreeSet<String>) -> Gen<()> {
    if !builtins.is_empty() {
        let (mut declarations, io, _) =
            baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
        declarations.extend(io);
        out.push_str(&baml_builtins2_codegen::generate_rt(
            &declarations,
            builtins,
        )?);
    }
    Ok(())
}
