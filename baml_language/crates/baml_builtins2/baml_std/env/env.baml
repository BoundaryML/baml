function get(key: string) -> string? {
  baml.env.get(key)
}

function get_or_panic(key: string) -> string {
  let val = baml.env.get(key)
  if val == null {
    baml.sys.panic("env var not found: " + key)
  }
  val
}