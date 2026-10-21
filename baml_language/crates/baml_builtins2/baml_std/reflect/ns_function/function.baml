/// One parameter of a function type: its name where the signature gives
/// one, its declared type, and whether it may be omitted.
class Parameter {
    name: string?,
    type: root.Type,
    optional: bool,
}
