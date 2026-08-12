// Types defined in namespace foo

class NsClass {
  value string
}

enum NsEnum {
  Alpha
  Beta
}

type NsAlias = string | int

// Cross-namespace references to root types

class NsRefToTop {
  inner root.Inner
  status root.Status
  items root.Inner[]
  alias_ref root.ClassAlias
}
