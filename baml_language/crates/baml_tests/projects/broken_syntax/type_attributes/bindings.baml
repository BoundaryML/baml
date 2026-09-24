// A scoped type binding has no field for an attribute to attach to.

function attribute_on_the_right_hand_side(t: reflect.Type) -> int {
  type T = unreflect(t) @alias("x")
  1
}
