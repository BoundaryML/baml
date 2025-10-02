function Foo() -> int {
  let x = 5 @emit;
  let y = 10 @emit(skip_def=true);
  let z: int = 20 @emit(when=MyFunction, name=foo_bar);
  10
}


function MyFunction(prev: int, next: int) -> bool {
  true
}

// function Foo() {
//   let x = 5 @emit(name=x);
//   let y = 10 @emit(name=y);
//   let z: int = 20 @emit(when=MyFunction, name=foo_bar);
//
//   10
// }
//
// function MyFunction(prev, next) {
//   true
// }
//
// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// enum std::HttpMethod {
//   Get
// }
