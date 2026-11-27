// Calls with a trace attachment. Native code records no trace: the call is
// the call without it, and the options it was given are nothing.

class TracedCounter {
  base: int

  function add(self, n: int) -> int {
    if (n <= 0) {
      self.base
    } else {
      1 + self.add(n - 1)
    }
  }
}
interface TracedShape {
  function area(self) -> int throws never
}
class TracedSquare {
  side: int

  implements TracedShape {
    function area(self) -> int throws never {
      self.side * self.side
    }
  }
}
class TracedRect {
  w: int
  h: int

  implements TracedShape {
    function area(self) -> int throws never {
      self.w * self.h
    }
  }
}

function traced_sum(n: int, by: int) -> int {
  if (n <= 0) {
    return 0;
  }
  by + traced_sum(n - 1, by)
}
function traced_shape(i: int) -> TracedShape {
  if (i % 2 == 0) {
    TracedSquare { side: i }
  } else {
    TracedRect { w: i, h: 2 }
  }
}

function traced_call_hidden() -> int { traced_sum(count(3), 2, $trace = trace.hidden()) }
function traced_call_span() -> int { traced_sum(count(3), 2, $trace = trace.span()) }
function traced_method_call() -> int {
  TracedCounter { base: count(2) }.add(count(3), $trace = trace.timing())
}
function traced_interface_call() -> int {
  traced_shape(count(3)).area($trace = trace.hidden())
    + traced_shape(count(4)).area($trace = trace.span())
}
function traced_options_reused() -> int {
  let options = trace.span();
  traced_sum(count(2), 5, $trace = options) + traced_sum(count(1), 1, $trace = options)
}
function traced_options_built() -> int {
  traced_sum(count(2), 5, $trace = trace.options().span(output = false))
    + traced_sum(count(1), 1, $trace = trace.empty_span())
    + traced_sum(count(2), 3, $trace = trace.options(mode = trace.Mode.Timing))
}
