class Foo {
  inner string
}

class Bar  {
  inner2 string
}

type A = Foo | Bar

function UseA(xs: A[]) -> int {
  client "openai/gpt-4o"
  prompt #"
    {% for x in xs %}
    {{ x.inner }}
    {% endfor %}
  "#
}

// warning: property 'inner' does not exist on Bar in type alias A
//   -->  client/class_alias.baml:15
//    | 
// 14 |     {% for x in xs %}
// 15 |     {{ x.inner }}
//    | 
