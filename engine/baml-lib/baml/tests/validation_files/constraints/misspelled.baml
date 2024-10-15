class Foo {
  // A constraint that didn't use Jinja Expression syntax.
  age int @check("this < 10", still_baby)
}

// error: Error validating: A constraint must have one Jinja argument such as {{ expr }}, and optionally one String label
//   -->  constraints/misspelled.baml:3
//    | 
//  2 |   // A constraint that didn't use Jinja Expression syntax.
//  3 |   age int @check("this < 10", still_baby)
//    | 
