function MayFail(x: int) -> string {
  "ok"
}

function UnknownBindingTypes(x: int) -> string {
  MayFail(x) catch (e: any) {
    _ => "caught any"
  } catch (other: unknown) {
    _ => "caught unknown"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
