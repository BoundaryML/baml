use super::*;

pub(super) fn emit(
    mut out: &mut String,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
    has_globals: bool,
) {
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    let driver = if has_globals {
        "rt::driver_observed_initialized"
    } else {
        "rt::driver_observed"
    };
    let initialize = if has_globals {
        ",initialize_program"
    } else {
        ""
    };
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{initialize_globals()?;let cx=rt::calls::Context::current();if rt::telemetry::active(){{f{entry}::<true>(cx,0)}}else{{f{entry}::<false>(cx,0)}}}}}}\nfn main()->std::process::ExitCode{{{builtin_init}{driver}(DESCRIPTORS,entry,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}){initialize})}}\n").unwrap();
}
