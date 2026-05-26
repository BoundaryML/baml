// Depth-4 collision consumer — see ../../../../../README.md.

class Deep {
  here root.symbol_collisions.foo.Bar
  there root.symbol_collisions.fizz.foo.Bar
  further root.symbol_collisions.fizz.buzz.foo.Bar
  nested root.symbol_collisions.lorem.Ipsum
}

function MakeDeep(
  here: root.symbol_collisions.foo.Bar,
  there: root.symbol_collisions.fizz.foo.Bar,
  further: root.symbol_collisions.fizz.buzz.foo.Bar,
  nested: root.symbol_collisions.lorem.Ipsum,
) -> Deep {
  Deep { here: here, there: there, further: further, nested: nested }
}

function RoundTripDeep(d: Deep) -> Deep {
  d
}
