//! Specialize the ordinary call graph once at the host boundary. Global off
//! must be distinguished from an unobserved function at low/hidden levels.
pub(super) fn prologue(
    object: usize,
    params: &str,
    arguments: &str,
    returned: &str,
    captures: &str,
    attribute: &str,
    guard: &str,
) -> String {
    format!(
        "#[inline(always)] unsafe fn f{object}<const OBSERVE:bool>(cx:rt::calls::Context,telemetry_site:usize,{params})->R<{returned}>{{unsafe{{if !OBSERVE{{return f{object}_body::<false>(cx,{arguments});}}let mode=rt::telemetry::mode(&O{object});if mode==0{{return f{object}_body::<true>(cx,{arguments});}}if mode!=1{{return f{object}_observed(cx,telemetry_site,mode,{arguments});}}let frame=rt::telemetry::enter_observed(telemetry_site);let mut result=f{object}_body::<true>(cx,{arguments});frame.finish(&mut result);result}}}}\n\
         #[inline(never)] unsafe fn f{object}_observed(cx:rt::calls::Context,telemetry_site:usize,mode:u8,{params})->R<{returned}>{{unsafe{{if mode>3{{rt::telemetry::evidence::observe_captured(telemetry_site,mode&3,{captures},||f{object}_body::<true>(cx,{arguments}))}}else{{rt::telemetry::observe_captured(telemetry_site,mode,{captures},||f{object}_body::<true>(cx,{arguments}))}}}}}}\n\
         {attribute}unsafe fn f{object}_body<const OBSERVE:bool>(cx:rt::calls::Context,{params})->R<{returned}>{{unsafe{{{guard}\n"
    )
}
