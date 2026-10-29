class CsvRow {
  name string
  count int
}

class OtherCsvRow {
  name string
}

// A generic class the runtime keeps to itself crosses as a reference at its
// type arguments, and comes back as itself.
function int_parse_cache() -> baml.sap._ParseCache<int> {
  baml.sap._new_parse_cache<int>()
}

function same_parse_cache(cache: baml.sap._ParseCache<int>) -> baml.sap._ParseCache<int> {
  cache
}
