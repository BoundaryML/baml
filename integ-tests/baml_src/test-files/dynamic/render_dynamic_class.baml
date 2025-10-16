enum RenderStatusEnum {
  ACTIVE @alias("currently active")
  INACTIVE @alias("currently inactive")
  @@dynamic
}

class RenderTestClass {
  name string
  status RenderStatusEnum
  @@dynamic
}

function RenderDynamicClass(input: RenderTestClass) -> string {
  client GPT35
  prompt #"
    Input class data: {{ input }}
  "#
}