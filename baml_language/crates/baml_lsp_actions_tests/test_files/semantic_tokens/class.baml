class Color {
  r int
  g int
  b int

  function get_red(self) -> int {
    self.r
  }

  function get_green(self) -> int {
    self.g
  }

  function get_blue(self) -> int {
    self.b
  }
}

function foo() -> int {
  let colors = [
    Color { r: 255, g: 0, b: 0 },
    Color { r: 0, g: 255, b: 0 },
    Color { r: 0, g: 0, b: 255 },
  ];

  let first_red = colors[0].get_red();
  let second_green = colors[1].g;

  let sum = colors[0].r + colors[1].g + colors[2].b;

  let nested = [colors[0].get_red(), colors[1].get_green(), colors[2].get_blue()];

  sum + nested[0] + nested[1] + nested[2]
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// class.baml:1:1 (keyword) len=5 "class"
// class.baml:1:7 (class) len=5 "Color"
// class.baml:2:3 (property) len=1 "r"
// class.baml:2:5 (type) len=3 "int"
// class.baml:3:3 (property) len=1 "g"
// class.baml:3:5 (type) len=3 "int"
// class.baml:4:3 (property) len=1 "b"
// class.baml:4:5 (type) len=3 "int"
// class.baml:6:3 (keyword) len=8 "function"
// class.baml:6:12 (function) len=7 "get_red"
// class.baml:6:20 (parameter) len=4 "self"
// class.baml:6:29 (type) len=3 "int"
// class.baml:7:5 (parameter) len=4 "self"
// class.baml:7:10 (property) len=1 "r"
// class.baml:10:3 (keyword) len=8 "function"
// class.baml:10:12 (function) len=9 "get_green"
// class.baml:10:22 (parameter) len=4 "self"
// class.baml:10:31 (type) len=3 "int"
// class.baml:11:5 (parameter) len=4 "self"
// class.baml:11:10 (property) len=1 "g"
// class.baml:14:3 (keyword) len=8 "function"
// class.baml:14:12 (function) len=8 "get_blue"
// class.baml:14:21 (parameter) len=4 "self"
// class.baml:14:30 (type) len=3 "int"
// class.baml:15:5 (parameter) len=4 "self"
// class.baml:15:10 (property) len=1 "b"
// class.baml:19:1 (keyword) len=8 "function"
// class.baml:19:10 (function) len=3 "foo"
// class.baml:19:19 (type) len=3 "int"
// class.baml:20:3 (keyword) len=3 "let"
// class.baml:20:7 (variable) len=6 "colors"
// class.baml:20:14 (operator) len=1 "="
// class.baml:21:5 (class) len=5 "Color"
// class.baml:21:13 (property) len=1 "r"
// class.baml:21:16 (number) len=3 "255"
// class.baml:21:21 (property) len=1 "g"
// class.baml:21:24 (number) len=1 "0"
// class.baml:21:27 (property) len=1 "b"
// class.baml:21:30 (number) len=1 "0"
// class.baml:22:5 (class) len=5 "Color"
// class.baml:22:13 (property) len=1 "r"
// class.baml:22:16 (number) len=1 "0"
// class.baml:22:19 (property) len=1 "g"
// class.baml:22:22 (number) len=3 "255"
// class.baml:22:27 (property) len=1 "b"
// class.baml:22:30 (number) len=1 "0"
// class.baml:23:5 (class) len=5 "Color"
// class.baml:23:13 (property) len=1 "r"
// class.baml:23:16 (number) len=1 "0"
// class.baml:23:19 (property) len=1 "g"
// class.baml:23:22 (number) len=1 "0"
// class.baml:23:25 (property) len=1 "b"
// class.baml:23:28 (number) len=3 "255"
// class.baml:26:3 (keyword) len=3 "let"
// class.baml:26:7 (variable) len=9 "first_red"
// class.baml:26:17 (operator) len=1 "="
// class.baml:26:19 (variable) len=6 "colors"
// class.baml:26:26 (number) len=1 "0"
// class.baml:26:29 (function) len=7 "get_red"
// class.baml:27:3 (keyword) len=3 "let"
// class.baml:27:7 (variable) len=12 "second_green"
// class.baml:27:20 (operator) len=1 "="
// class.baml:27:22 (variable) len=6 "colors"
// class.baml:27:29 (number) len=1 "1"
// class.baml:27:32 (property) len=1 "g"
// class.baml:29:3 (keyword) len=3 "let"
// class.baml:29:7 (variable) len=3 "sum"
// class.baml:29:11 (operator) len=1 "="
// class.baml:29:13 (variable) len=6 "colors"
// class.baml:29:20 (number) len=1 "0"
// class.baml:29:23 (property) len=1 "r"
// class.baml:29:25 (operator) len=1 "+"
// class.baml:29:27 (variable) len=6 "colors"
// class.baml:29:34 (number) len=1 "1"
// class.baml:29:37 (property) len=1 "g"
// class.baml:29:39 (operator) len=1 "+"
// class.baml:29:41 (variable) len=6 "colors"
// class.baml:29:48 (number) len=1 "2"
// class.baml:29:51 (property) len=1 "b"
// class.baml:31:3 (keyword) len=3 "let"
// class.baml:31:7 (variable) len=6 "nested"
// class.baml:31:14 (operator) len=1 "="
// class.baml:31:17 (variable) len=6 "colors"
// class.baml:31:24 (number) len=1 "0"
// class.baml:31:27 (function) len=7 "get_red"
// class.baml:31:38 (variable) len=6 "colors"
// class.baml:31:45 (number) len=1 "1"
// class.baml:31:48 (function) len=9 "get_green"
// class.baml:31:61 (variable) len=6 "colors"
// class.baml:31:68 (number) len=1 "2"
// class.baml:31:71 (function) len=8 "get_blue"
// class.baml:33:3 (variable) len=3 "sum"
// class.baml:33:7 (operator) len=1 "+"
// class.baml:33:9 (variable) len=6 "nested"
// class.baml:33:16 (number) len=1 "0"
// class.baml:33:19 (operator) len=1 "+"
// class.baml:33:21 (variable) len=6 "nested"
// class.baml:33:28 (number) len=1 "1"
// class.baml:33:31 (operator) len=1 "+"
// class.baml:33:33 (variable) len=6 "nested"
// class.baml:33:40 (number) len=1 "2"
