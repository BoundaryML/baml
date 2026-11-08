use super::*;

pub(super) fn emit(
    mut out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
) {
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{let cx=rt::calls::Context::current();if rt::telemetry::active(){{f{entry}::<true>(cx,0)}}else{{f{entry}::<false>(cx,0)}}}}}}\nfn main()->std::process::ExitCode{{{builtin_init}rt::driver_observed(DESCRIPTORS,entry,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}))}}\n").unwrap();
}
