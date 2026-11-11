//! Reproducible declaration inventory; no checked-in report to become stale.
fn main() {
    use baml_builtins2_codegen::{
        RtDisposition, classify_builtin, implementation_strategy, native_inventory,
    };
    let declarations = native_inventory().expect("stdlib declarations");
    let mut counts = std::collections::BTreeMap::<_, (usize, usize, usize)>::new();
    for d in &declarations {
        let (group, status) = classify_builtin(d);
        let count = counts.entry(group).or_default();
        count.0 += 1;
        count.1 += usize::from(matches!(
            status,
            RtDisposition::TypedAdapter
                | RtDisposition::CompilerIntrinsic
                | RtDisposition::RuntimeService
        ));
        count.2 += usize::from(matches!(status, RtDisposition::Partial(_)));
        println!(
            "{}\t{group:?}\t{status:?}\t{:?}",
            d.path,
            implementation_strategy(d)
        );
    }
    eprintln!(
        "total: {}; (total, ported, partial): {counts:?}",
        declarations.len()
    );
}
