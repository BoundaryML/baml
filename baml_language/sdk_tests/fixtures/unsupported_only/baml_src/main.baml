// Every callable in this fixture is deliberately outside the current Go
// wire-type algebra. The generated package must still compile without
// dangling imports or partial declarations.
class Payload {
  values (string | int)[]
}

function round_trip_payload(payload: Payload) -> Payload {
  payload
}

function optional_default(value: (string | int)[] = []) -> (string | int)[] {
  value
}

function generic_identity<T>(value: T) -> T {
  value
}
