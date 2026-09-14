// Phase 5 fixture: non-generic `to_string` plus generic `from_string<User>`.
// MIR snapshot must show no type arg for serialization and `LoadType(User)`
// followed by a call with `ntypeargs=1` for deserialization.

class User {
  name string
  age int
}

function serialize_user(u: User) -> string throws baml.json.SerializationError {
  baml.json.to_string(u)
}

function deserialize_user(s: string) -> User throws baml.json.ParseError | baml.json.DecodeError {
  baml.json.from_string<User>(s)
}
