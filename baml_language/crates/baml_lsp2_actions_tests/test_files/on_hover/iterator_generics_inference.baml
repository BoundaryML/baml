class Done {}

interface Iterable<T, E> {
  function iter(self) -> Iterator<T, E> throws never
}

interface Iterator<T, E> requires Iterable<T, E> {
  function next(self) -> T | Done throws E

  function map<R, E2>(self, fn: (T) -> R throws E2) -> Iterator<R, E | E2> throws never {
    Map<T, R, E, E2> { iter: self, func: fn }
  }

  function filter<E2>(self, predicate: (T) -> bool throws E2) -> Iterator<T, E | E2> throws never {
    Filter<T, E, E2> { iter: self, predicate: predicate }
  }

  function filter_map<R, E2>(self, fn: (T) -> R? throws E2) -> Iterator<R, E | E2> throws never {
    FilterMap<T, R, E, E2> { iter: self, func: fn }
  }

  function flat_map<R, E2, E3>(self, fn: (T) -> Iterable<R, E3> throws E2) -> Iterator<R, E | E2 | E3> throws never {
    FlatMap<T, R, E, E2, E3> { iter: self, func: fn, inner: Done {} }
  }

  function step_by(self, n: int) -> Iterator<T, E> throws never {
    StepBy<T, E> { iter: self, n: n, first: true }
  }

  function some<E2>(self, predicate: (T) -> bool throws E2) -> bool throws E | E2 {
    true
  }

  function every<E2>(self, predicate: (T) -> bool throws E2) -> bool throws E | E2 {
    true
  }

  function find<E2>(self, predicate: (T) -> bool throws E2) -> T? throws E | E2 {
    null
  }

  function collect(self) -> T[] throws E {
    []
  }
}

function flatten<R, E, E3>(src: Iterator<Iterable<R, E3>, E>) -> Iterator<R, E | E3> throws never {
  Flatten<R, E, E3> { iter: src, inner: Done {} }
}

class ArrayIterator<T> {
  arr: T[]
  idx: int

  function new(arr: T[]) -> ArrayIterator<T> throws never {
    ArrayIterator<T> { arr: arr, idx: 0 }
  }

  implements Iterable<T, never> {
    function iter(self) -> Iterator<T, never> throws never { self }
  }

  implements Iterator<T, never> {
    function next(self) -> T | Done throws never {
      Done {}
    }
  }
}

class Map<T, R, E, E2> {
  iter: Iterator<T, E>
  func: (T) -> R throws E2

  implements Iterable<R, E | E2> {
    function iter(self) -> Iterator<R, E | E2> throws never { self }
  }

  implements Iterator<R, E | E2> {
    function next(self) -> R | Done throws E | E2 {
      Done {}
    }
  }
}

class Filter<T, E, E2> {
  iter: Iterator<T, E>
  predicate: (T) -> bool throws E2

  implements Iterable<T, E | E2> {
    function iter(self) -> Iterator<T, E | E2> throws never { self }
  }

  implements Iterator<T, E | E2> {
    function next(self) -> T | Done throws E | E2 {
      Done {}
    }
  }
}

class FilterMap<T, R, E, E2> {
  iter: Iterator<T, E>
  func: (T) -> R? throws E2

  implements Iterable<R, E | E2> {
    function iter(self) -> Iterator<R, E | E2> throws never { self }
  }

  implements Iterator<R, E | E2> {
    function next(self) -> R | Done throws E | E2 {
      Done {}
    }
  }
}

class FlatMap<T, R, E, E2, E3> {
  iter: Iterator<T, E>
  func: (T) -> Iterable<R, E3> throws E2
  inner: Iterator<R, E3> | Done

  implements Iterable<R, E | E2 | E3> {
    function iter(self) -> Iterator<R, E | E2 | E3> throws never { self }
  }

  implements Iterator<R, E | E2 | E3> {
    function next(self) -> R | Done throws E | E2 | E3 {
      Done {}
    }
  }
}

class Flatten<R, E, E3> {
  iter: Iterator<Iterable<R, E3>, E>
  inner: Iterator<R, E3> | Done

  implements Iterable<R, E | E3> {
    function iter(self) -> Iterator<R, E | E3> throws never { self }
  }

  implements Iterator<R, E | E3> {
    function next(self) -> R | Done throws E | E3 {
      Done {}
    }
  }
}

class StepBy<T, E> {
  iter: Iterator<T, E>
  n: int
  first: bool

  implements Iterable<T, E> {
    function iter(self) -> Iterator<T, E> throws never { self }
  }

  implements Iterator<T, E> {
    function next(self) -> T | Done throws E {
      Done {}
    }
  }
}

implements<T> Iterable<T, never> for T[] {
  function iter(self) -> Iterator<T, never> throws never {
    ArrayIterator.new(self)
  }
}

interface OutOfBodyValue<T> {
  function get(self) -> T throws unknown
}

class OutOfBodyBox<T> {
  value: T

  function new(value: T) -> OutOfBodyBox<T> throws never {
    OutOfBodyBox<T> { value: value }
  }
}

implements<T> OutOfBodyValue<T> for OutOfBodyBox<T> {
  function get(self) -> T throws unknown {
    self.value
  }
}

function hovers() -> void {
  let source<[CURSOR] = ArrayIterator.new([1, 2, 3])
  let sourceMethod<[CURSOR] = source.next

  let someLarge<[CURSOR] = ArrayIterator.new([1, 2, 3]).some((x: int) -> bool { x > 2 })
  let everySmall<[CURSOR] = ArrayIterator.new([1, 2, 3]).every((x: int) -> bool { x < 4 })
  let found<[CURSOR] = ArrayIterator.new([1, 2, 3, 4]).find((x: int) -> bool { x % 2 == 0 })

  let mapped<[CURSOR] = ArrayIterator.new([1, 2, 3]).map((x: int) -> int { x * 2 })
  let filtered<[CURSOR] = ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool { x % 2 == 0 })
  let filterMapped<[CURSOR] = ArrayIterator.new([1, 2, 3, 4]).filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  })
  let flatMapped<[CURSOR] = ArrayIterator.new([1, 2]).flat_map((x: int) -> Iterable<int, never> {
    ArrayIterator.new([x, x + 10])
  })

  let stepped<[CURSOR] = ArrayIterator.new([1, 2, 3, 4]).step_by(2)
  let arrayIter<[CURSOR] = [1, 2, 3].as<Iterable<int, never>>.iter()
  let collected<[CURSOR] = arrayIter.collect()

  let explicitMap<[CURSOR] : Iterator<int, never> = Map<int, int, never, never> {
    iter: ArrayIterator.new([1, 2, 3]),
    func: (x: int) -> int { x * 3 },
  }

  let iterableItems: Iterable<int, never>[] = [
    ArrayIterator.new([1, 2]),
    ArrayIterator.new([3, 4]),
  ]
  let outer<[CURSOR] = ArrayIterator.new(iterableItems)
  let flattened<[CURSOR] = flatten(outer)

  let boxed<[CURSOR] = OutOfBodyBox.new("hello")
  let boxedValue<[CURSOR] = boxed.get()
}

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: E, E2
//     ╭─[iterator_generics_inference.baml:30:83]
//  30 │ ╭─▶   function some<E2>(self, predicate: (T) -> bool throws E2) -> bool throws E | E2 {
//  31 │ │       true
//  32 │ ╰─▶   }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E2
//     ╭─[iterator_generics_inference.baml:34:84]
//  34 │ ╭─▶   function every<E2>(self, predicate: (T) -> bool throws E2) -> bool throws E | E2 {
//  35 │ │       true
//  36 │ ╰─▶   }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E2
//     ╭─[iterator_generics_inference.baml:38:81]
//  38 │ ╭─▶   function find<E2>(self, predicate: (T) -> bool throws E2) -> T? throws E | E2 {
//  39 │ │       null
//  40 │ ╰─▶   }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E
//     ╭─[iterator_generics_inference.baml:42:42]
//  42 │ ╭─▶   function collect(self) -> T[] throws E {
//  43 │ │       []
//  44 │ ╰─▶   }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E2
//     ╭─[iterator_generics_inference.baml:79:51]
//  79 │ ╭─▶     function next(self) -> R | Done throws E | E2 {
//  80 │ │         Done {}
//  81 │ ╰─▶     }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E2
//     ╭─[iterator_generics_inference.baml:94:51]
//  94 │ ╭─▶     function next(self) -> T | Done throws E | E2 {
//  95 │ │         Done {}
//  96 │ ╰─▶     }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E2
//      ╭─[iterator_generics_inference.baml:109:51]
//  109 │ ╭─▶     function next(self) -> R | Done throws E | E2 {
//  110 │ │         Done {}
//  111 │ ╰─▶     }
//      ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E2, E3
//      ╭─[iterator_generics_inference.baml:125:56]
//  125 │ ╭─▶     function next(self) -> R | Done throws E | E2 | E3 {
//  126 │ │         Done {}
//  127 │ ╰─▶     }
//      ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E, E3
//      ╭─[iterator_generics_inference.baml:140:51]
//  140 │ ╭─▶     function next(self) -> R | Done throws E | E3 {
//  141 │ │         Done {}
//  142 │ ╰─▶     }
//      ╰────
// E0097
//
//   ⚠ extraneous throws declaration: E
//      ╭─[iterator_generics_inference.baml:156:46]
//  156 │ ╭─▶     function next(self) -> T | Done throws E {
//  157 │ │         Done {}
//  158 │ ╰─▶     }
//      ╰────
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//      ╭─[iterator_generics_inference.baml:181:42]
//  181 │ ╭─▶   function get(self) -> T throws unknown {
//  182 │ │       self.value
//  183 │ ╰─▶   }
//      ╰────
//
//- on_hover expressions
// hover at iterator_generics_inference.baml:187:13
// ```baml
// source: ArrayIterator<int>
// ```
// hover at iterator_generics_inference.baml:188:19
// ```baml
// sourceMethod: () -> int | Done throws never
// ```
// hover at iterator_generics_inference.baml:190:16
// ```baml
// someLarge: bool
// ```
// hover at iterator_generics_inference.baml:191:17
// ```baml
// everySmall: bool
// ```
// hover at iterator_generics_inference.baml:192:12
// ```baml
// found: int | null
// ```
// hover at iterator_generics_inference.baml:194:13
// ```baml
// mapped: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:195:15
// ```baml
// filtered: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:196:19
// ```baml
// filterMapped: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:203:17
// ```baml
// flatMapped: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:207:14
// ```baml
// stepped: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:208:16
// ```baml
// arrayIter: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:209:16
// ```baml
// collected: int[]
// ```
// hover at iterator_generics_inference.baml:211:18
// ```baml
// explicitMap: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:220:12
// ```baml
// outer: ArrayIterator<Iterable<int, never>>
// ```
// hover at iterator_generics_inference.baml:221:16
// ```baml
// flattened: Iterator<int, never>
// ```
// hover at iterator_generics_inference.baml:223:12
// ```baml
// boxed: OutOfBodyBox<string>
// ```
// hover at iterator_generics_inference.baml:224:17
// ```baml
// boxedValue: string
// ```
