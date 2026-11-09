//! Random declarations use concrete class layouts selected by the pack.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin};
pub(crate) fn supports(path: &str) -> bool {
    matches!(
        path,
        "baml.random.Xoshiro256PlusPlus._new"
            | "baml.random.ChaCha20._new"
            | "baml.random.Rng$for$Xoshiro256PlusPlus.random"
            | "baml.random.Rng$for$Xoshiro256PlusPlus.random_int"
            | "baml.random.Rng$for$ChaCha20.random"
            | "baml.random.Rng$for$ChaCha20.random_int"
            | "baml.random.Rng$for$SystemRandom.random"
            | "baml.random.Rng$for$SystemRandom.random_int"
    )
}
pub(crate) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let method = d.path.rsplit('.').next().unwrap();
    let (params, ret) = match method {
        "_new" => (
            vec![T::Uint8Array],
            T::Named(
                if d.path.contains("ChaCha20") {
                    "ChaCha20"
                } else {
                    "Xoshiro256PlusPlus"
                }
                .into(),
            ),
        ),
        "random" => (vec![T::Int], T::Uint8Array),
        "random_int" => (vec![], T::Int),
        _ => unreachable!(),
    };
    let pipeline = if d.path.contains("SystemRandom") {
        BuiltinPipeline::Io
    } else {
        BuiltinPipeline::Vm
    };
    if d.pipeline != pipeline
        || !d.generics.is_empty()
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
    {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}: {:?} -> {:?}",
            d.path,
            d.params.iter().map(|p| &p.ty).collect::<Vec<_>>(),
            d.return_type
        ));
    }
    Ok(String::new())
}
