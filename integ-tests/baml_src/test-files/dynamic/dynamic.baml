class DynamicClassOne {
  hi string
  @@dynamic
}

enum DynEnumOne {
  @@dynamic
}

enum DynEnumTwo {
  @@dynamic
}

class SomeClassNestedDynamic {
  hi string
  @@dynamic

}

class DynamicClassTwo {
  hi string
  some_class SomeClassNestedDynamic
  status DynEnumOne
  @@dynamic
}