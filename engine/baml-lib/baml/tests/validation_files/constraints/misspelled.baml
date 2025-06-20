class Foo {
  // A constraint that didn't use Jinja Expression syntax.
  age int @check("this < 10", still_baby)
}

// error: Error validating: 
// A check or assert has an optional identifier and a Jinja argument. e.g.:
// @check( valid_name_length, {{ this|length > 0 }} )
// @assert( {{ this|length > 0 }} )
// @assert( valid_name_length, {{ this|length > 0 }} )
//                         
//   -->  constraints/misspelled.baml:3
//    | 
//  2 |   // A constraint that didn't use Jinja Expression syntax.
//  3 |   age int @check("this < 10", still_baby)
//    | 
