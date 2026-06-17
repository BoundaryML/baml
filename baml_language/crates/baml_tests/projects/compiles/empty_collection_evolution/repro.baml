/// Builds a string map from an empty literal, then reads the inserted header.
///
/// Takes no parameters and returns the stored header value. Throws no declared
/// errors; this fixture exists to ensure empty map literals evolve from
/// subsequent `set` calls.
function EmptyMapLiteralSetThenGet() -> string {
  let headers = {};
  headers.set("a", "b");
  headers.get("a") ?? ""
}

/// Builds an integer array from an empty literal, then reads the inserted item.
///
/// Takes no parameters and returns the first pushed integer. Throws no declared
/// errors; this fixture guards the corresponding empty array evolution path.
function EmptyArrayLiteralPushThenRead() -> int {
  let values = [];
  values.push(1);
  values[0]
}
