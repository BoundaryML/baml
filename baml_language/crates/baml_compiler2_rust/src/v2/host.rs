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
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{f{entry}(0)}}}}\nfn main()->std::process::ExitCode{{{builtin_init}rt::driver_observed(DESCRIPTORS,entry,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}))}}\n").unwrap();
    if !diagnostic_barrier().is_empty() {
        out.push_str("#[inline(always)] fn diag_boundary_work() {}\n");
    }
}
