/// Companion class for the built-in `type` primitive.
///
/// The BAML `type` primitive represents a runtime type value returned by
/// `reflect.type_of<T>()`. This class provides methods on `type` values,
/// mirroring how `class String` provides methods on `string`.
///
/// Users never construct `Type` directly; they receive `type` values via
/// `reflect.type_of<User>()` and call methods on them.
class TypeValue {
    /// Returns the type name as a human-readable string.
    //baml:vm
    function to_string(self) -> string {
        $rust_function
    }
}
