// These names collide with locals owned by the generated Go function body.
// They must keep their original wire keys while receiving safe Go locals.
function reserved_args(
  ctx: string,
  result: int,
  err: bool,
  zero: string,
  bootstrap: string,
  select: string,
  range: string,
  chan: string,
  goto: string,
  fallthrough: string,
) -> string {
  ctx
}
