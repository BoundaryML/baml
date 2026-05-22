// Depth-1 collision consumer — see ../README.md.

class Ipsum {
  bar1 root.symbol_collisions.foo.Bar
  bar2 root.symbol_collisions.fizz.foo.Bar
  bar3 root.symbol_collisions.fizz.buzz.foo.Bar
}

function MakeIpsum(
  bar1: root.symbol_collisions.foo.Bar,
  bar2: root.symbol_collisions.fizz.foo.Bar,
  bar3: root.symbol_collisions.fizz.buzz.foo.Bar,
) -> Ipsum {
  Ipsum { bar1: bar1, bar2: bar2, bar3: bar3 }
}

function RoundTripIpsum(i: Ipsum) -> Ipsum {
  i
}
