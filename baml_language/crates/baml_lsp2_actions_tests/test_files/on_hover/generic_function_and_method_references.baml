function f<T>(x: T) -> void {
}

class Wrapper<T> {
  inner: T

  function value(self) -> T {
    return self.inner
  }
}

interface MyInterface {
  function myMethod(self) -> int throws never
}

class MyClass {
  implements MyInterface {
    function myMethod(self) -> int {
      return 0
    }
  }
}

function example() -> void {
  let g<[CURSOR] = f<int>

  let a = Wrapper<string> { inner: "asdf" }
  let boundValue<[CURSOR] = a.value

  let classMethod<[CURSOR] = MyClass.myMethod
  let inferredInterfaceMethod<[CURSOR] = MyInterface.myMethod

  let myInstance = MyClass {}
  let instanceMethod<[CURSOR] = myInstance.myMethod
  let instanceProjectedMethod<[CURSOR] = myInstance.as<MyInterface>.myMethod
}

//----
//- diagnostics
// E0002
//
//   × cannot infer type parameter `Self`
//     ╭─[generic_function_and_method_references.baml:31:33]
//  31 │   let inferredInterfaceMethod = MyInterface.myMethod
//     ·                                 ────────────────────
//     ╰────
//
//- on_hover expressions
// hover at generic_function_and_method_references.baml:25:8
// ```baml
// g: (x: int) -> void throws never
// ```
// hover at generic_function_and_method_references.baml:28:17
// ```baml
// boundValue: () -> string throws never
// ```
// hover at generic_function_and_method_references.baml:30:18
// ```baml
// classMethod: (self: MyClass) -> int throws never
// ```
// hover at generic_function_and_method_references.baml:31:30
// ```baml
// inferredInterfaceMethod: (self: !error) -> int throws never
// ```
// hover at generic_function_and_method_references.baml:34:21
// ```baml
// instanceMethod: () -> int throws never
// ```
// hover at generic_function_and_method_references.baml:35:30
// ```baml
// instanceProjectedMethod: () -> int throws never
// ```
