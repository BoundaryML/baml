function Foo1(arg: int) -> float {
  client Foo
}

function Foo2(arg: int) -> float {
  prompt #"..."#
}

function Foo3(arg: int) -> float {
  client bar
  prompt "..."
}


function Foo4(arg: int) -> float {
  client bar
  prompt
}


function Foo5(arg: int) -> float {
  client
  prompt #"..."#
}


function Foo6(arg: int) -> float {
  client
  prompt
}

// error: Error validating Function "Foo4": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:17
//    | 
// 16 |   client bar
// 17 |   prompt
//    | 
// error: Error validating Function "Foo5": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:22
//    | 
// 21 | function Foo5(arg: int) -> float {
// 22 |   client
//    | 
// error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:28
//    | 
// 27 | function Foo6(arg: int) -> float {
// 28 |   client
//    | 
