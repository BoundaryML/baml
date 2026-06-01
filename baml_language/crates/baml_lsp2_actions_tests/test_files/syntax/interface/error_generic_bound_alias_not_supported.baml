interface Converter<T> {
  function convert(self) -> T
}

function read_int<T extends Converter<int> as Ints>(m: T) -> int {
  return m.as<Converter<int>>.convert()
}

//----
//- diagnostics
// error: generic bound aliases are not supported; use `.as<Interface<...>>()` at call sites
//    ╭─[ interface_error_generic_bound_alias_not_supported.baml:5:44 ]
//    │
//  5 │ function read_int<T extends Converter<int> as Ints>(m: T) -> int {
//    │                                            ─┬  
//    │                                             ╰── generic bound aliases are not supported; use `.as<Interface<...>>()` at call sites
//    │ 
//    │ Note: Error code: E0010
// ───╯
