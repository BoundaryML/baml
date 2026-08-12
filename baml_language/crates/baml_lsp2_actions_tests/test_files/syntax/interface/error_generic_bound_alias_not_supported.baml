interface Converter<T> {
  function convert(self) -> T throws never
}

function read_int<T extends Converter<int> as Ints>(m: T) -> int {
  return m.as<Converter<int>>.convert()
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[interface_error_generic_bound_alias_not_supported.baml:5:44]
//  5 │ function read_int<T extends Converter<int> as Ints>(m: T) -> int {
//    ·                                            ─┬
//    ·                                             ╰── generic bound aliases are not supported; use `.as<Interface<...>>()` at call sites
//    ╰────
