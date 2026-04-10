// === Additional member receiver throws propagation shapes ===

class EdgeHandler<E> {
  run: () -> null throws E
}

class EdgeHolder<T> {
  item: T
}

type EdgeStringHandler = EdgeHandler<string>

class EdgeService {
  primary: EdgeHandler<string>

  function call_self_field(self) -> null {
    self.primary.run()
  }

  function call_self_alias(self) -> null {
    let me = self
    me.primary.run()
  }
}

function make_edge_handler() -> EdgeHandler<string> {
  EdgeHandler { run: () -> null { throw "edge boom" } }
}

function call_returned_receiver() -> null {
  make_edge_handler().run()
}

function call_typed_local_receiver() -> null {
  let h: EdgeHandler<string> = make_edge_handler()
  h.run()
}

function call_nested_alias_receiver(h: EdgeHandler<string>) -> null {
  let a = h
  let b = a
  b.run()
}

function call_field_chained_receiver(holder: EdgeHolder<EdgeStringHandler>) -> null {
  holder.item.run()
}
