class Color {
  red int
  green int
  blue int

  function get_red(self) -> int {
    self.red
  }

  function get_green(self) -> int {
    self.green
  }

  function get_blue(self) -> int {
    self.blue
  }
}

function foo() -> int {
  let colors = [
    Color { red: 255, green: 0, blue: 0 },
    Color { red: 0, green: 255, blue: 0 },
    Color { red: 0, green: 0, blue: 255 },
  ];

  let first_red = colors[0].get_red();
  let second_green = colors[1].green;

  let sum = colors[0].red + colors[1].green + colors[2].blue;

  let nested = [colors[0].get_red(), colors[1].get_green(), colors[2].get_blue()];

  sum + nested[0] + nested[1] + nested[2]
}

// Make sure only real fields get the highlighting
function fo() -> int {
  let colors = [
    Color { re: 255, gree: 0, blu: 0 },
    Color { re: 0, gree: 255, blu: 0 },
    Color { re: 0, gree: 0, blu: 255 },
  ];

  let first_red = colors[0].get_re();
  let second_green = colors[1].gree;

  let sum = colors[0].re + colors[1].gree + colors[2].blu;

  let nested = [colors[0].get_re(), colors[1].get_gree(), colors[2].get_blu()];

  sum + nested[0] + nested[1] + nested[2]
}

//----
//- diagnostics
// Error: Type `Color` has no field `get_re`
//     ╭─[ class.baml:44:19 ]
//     │
//  44 │   let first_red = colors[0].get_re();
//     │                   ────────┬───────  
//     │                           ╰───────── Type `Color` has no field `get_re`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `gree`
//     ╭─[ class.baml:45:22 ]
//     │
//  45 │   let second_green = colors[1].gree;
//     │                      ───────┬──────  
//     │                             ╰──────── Type `Color` has no field `gree`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `re`
//     ╭─[ class.baml:47:13 ]
//     │
//  47 │   let sum = colors[0].re + colors[1].gree + colors[2].blu;
//     │             ──────┬─────  
//     │                   ╰─────── Type `Color` has no field `re`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `gree`
//     ╭─[ class.baml:47:28 ]
//     │
//  47 │   let sum = colors[0].re + colors[1].gree + colors[2].blu;
//     │                            ───────┬──────  
//     │                                   ╰──────── Type `Color` has no field `gree`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `blu`
//     ╭─[ class.baml:47:45 ]
//     │
//  47 │   let sum = colors[0].re + colors[1].gree + colors[2].blu;
//     │                                             ──────┬──────  
//     │                                                   ╰──────── Type `Color` has no field `blu`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `get_re`
//     ╭─[ class.baml:49:17 ]
//     │
//  49 │   let nested = [colors[0].get_re(), colors[1].get_gree(), colors[2].get_blu()];
//     │                 ────────┬───────  
//     │                         ╰───────── Type `Color` has no field `get_re`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `get_gree`
//     ╭─[ class.baml:49:37 ]
//     │
//  49 │   let nested = [colors[0].get_re(), colors[1].get_gree(), colors[2].get_blu()];
//     │                                     ─────────┬────────  
//     │                                              ╰────────── Type `Color` has no field `get_gree`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Color` has no field `get_blu`
//     ╭─[ class.baml:49:59 ]
//     │
//  49 │   let nested = [colors[0].get_re(), colors[1].get_gree(), colors[2].get_blu()];
//     │                                                           ────────┬────────  
//     │                                                                   ╰────────── Type `Color` has no field `get_blu`
//     │ 
//     │ Note: Error code: E0007
// ────╯
//
//- semantic_tokens
// class.baml:1:1 (keyword) len=5 "class"
// class.baml:1:7 (class) len=5 "Color"
// class.baml:2:3 (property) len=3 "red"
// class.baml:2:7 (type) len=3 "int"
// class.baml:3:3 (property) len=5 "green"
// class.baml:3:9 (type) len=3 "int"
// class.baml:4:3 (property) len=4 "blue"
// class.baml:4:8 (type) len=3 "int"
// class.baml:6:3 (keyword) len=8 "function"
// class.baml:6:12 (function) len=7 "get_red"
// class.baml:6:20 (parameter) len=4 "self"
// class.baml:6:29 (type) len=3 "int"
// class.baml:7:5 (parameter) len=4 "self"
// class.baml:7:10 (property) len=3 "red"
// class.baml:10:3 (keyword) len=8 "function"
// class.baml:10:12 (function) len=9 "get_green"
// class.baml:10:22 (parameter) len=4 "self"
// class.baml:10:31 (type) len=3 "int"
// class.baml:11:5 (parameter) len=4 "self"
// class.baml:11:10 (property) len=5 "green"
// class.baml:14:3 (keyword) len=8 "function"
// class.baml:14:12 (function) len=8 "get_blue"
// class.baml:14:21 (parameter) len=4 "self"
// class.baml:14:30 (type) len=3 "int"
// class.baml:15:5 (parameter) len=4 "self"
// class.baml:15:10 (property) len=4 "blue"
// class.baml:19:1 (keyword) len=8 "function"
// class.baml:19:10 (function) len=3 "foo"
// class.baml:19:19 (type) len=3 "int"
// class.baml:20:3 (keyword) len=3 "let"
// class.baml:20:7 (variable) len=6 "colors"
// class.baml:20:14 (operator) len=1 "="
// class.baml:21:5 (class) len=5 "Color"
// class.baml:21:13 (property) len=3 "red"
// class.baml:21:18 (number) len=3 "255"
// class.baml:21:23 (property) len=5 "green"
// class.baml:21:30 (number) len=1 "0"
// class.baml:21:33 (property) len=4 "blue"
// class.baml:21:39 (number) len=1 "0"
// class.baml:22:5 (class) len=5 "Color"
// class.baml:22:13 (property) len=3 "red"
// class.baml:22:18 (number) len=1 "0"
// class.baml:22:21 (property) len=5 "green"
// class.baml:22:28 (number) len=3 "255"
// class.baml:22:33 (property) len=4 "blue"
// class.baml:22:39 (number) len=1 "0"
// class.baml:23:5 (class) len=5 "Color"
// class.baml:23:13 (property) len=3 "red"
// class.baml:23:18 (number) len=1 "0"
// class.baml:23:21 (property) len=5 "green"
// class.baml:23:28 (number) len=1 "0"
// class.baml:23:31 (property) len=4 "blue"
// class.baml:23:37 (number) len=3 "255"
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
// class.baml:27:32 (property) len=5 "green"
// class.baml:29:3 (keyword) len=3 "let"
// class.baml:29:7 (variable) len=3 "sum"
// class.baml:29:11 (operator) len=1 "="
// class.baml:29:13 (variable) len=6 "colors"
// class.baml:29:20 (number) len=1 "0"
// class.baml:29:23 (property) len=3 "red"
// class.baml:29:27 (operator) len=1 "+"
// class.baml:29:29 (variable) len=6 "colors"
// class.baml:29:36 (number) len=1 "1"
// class.baml:29:39 (property) len=5 "green"
// class.baml:29:45 (operator) len=1 "+"
// class.baml:29:47 (variable) len=6 "colors"
// class.baml:29:54 (number) len=1 "2"
// class.baml:29:57 (property) len=4 "blue"
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
// class.baml:36:1 (comment) len=50 "// Make sure only real fields get the highlighting"
// class.baml:37:1 (keyword) len=8 "function"
// class.baml:37:10 (function) len=2 "fo"
// class.baml:37:18 (type) len=3 "int"
// class.baml:38:3 (keyword) len=3 "let"
// class.baml:38:7 (variable) len=6 "colors"
// class.baml:38:14 (operator) len=1 "="
// class.baml:39:5 (class) len=5 "Color"
// class.baml:39:17 (number) len=3 "255"
// class.baml:39:28 (number) len=1 "0"
// class.baml:39:36 (number) len=1 "0"
// class.baml:40:5 (class) len=5 "Color"
// class.baml:40:17 (number) len=1 "0"
// class.baml:40:26 (number) len=3 "255"
// class.baml:40:36 (number) len=1 "0"
// class.baml:41:5 (class) len=5 "Color"
// class.baml:41:17 (number) len=1 "0"
// class.baml:41:26 (number) len=1 "0"
// class.baml:41:34 (number) len=3 "255"
// class.baml:44:3 (keyword) len=3 "let"
// class.baml:44:7 (variable) len=9 "first_red"
// class.baml:44:17 (operator) len=1 "="
// class.baml:44:19 (variable) len=6 "colors"
// class.baml:44:26 (number) len=1 "0"
// class.baml:45:3 (keyword) len=3 "let"
// class.baml:45:7 (variable) len=12 "second_green"
// class.baml:45:20 (operator) len=1 "="
// class.baml:45:22 (variable) len=6 "colors"
// class.baml:45:29 (number) len=1 "1"
// class.baml:47:3 (keyword) len=3 "let"
// class.baml:47:7 (variable) len=3 "sum"
// class.baml:47:11 (operator) len=1 "="
// class.baml:47:13 (variable) len=6 "colors"
// class.baml:47:20 (number) len=1 "0"
// class.baml:47:26 (operator) len=1 "+"
// class.baml:47:28 (variable) len=6 "colors"
// class.baml:47:35 (number) len=1 "1"
// class.baml:47:43 (operator) len=1 "+"
// class.baml:47:45 (variable) len=6 "colors"
// class.baml:47:52 (number) len=1 "2"
// class.baml:49:3 (keyword) len=3 "let"
// class.baml:49:7 (variable) len=6 "nested"
// class.baml:49:14 (operator) len=1 "="
// class.baml:49:17 (variable) len=6 "colors"
// class.baml:49:24 (number) len=1 "0"
// class.baml:49:37 (variable) len=6 "colors"
// class.baml:49:44 (number) len=1 "1"
// class.baml:49:59 (variable) len=6 "colors"
// class.baml:49:66 (number) len=1 "2"
// class.baml:51:3 (variable) len=3 "sum"
// class.baml:51:7 (operator) len=1 "+"
// class.baml:51:9 (variable) len=6 "nested"
// class.baml:51:16 (number) len=1 "0"
// class.baml:51:19 (operator) len=1 "+"
// class.baml:51:21 (variable) len=6 "nested"
// class.baml:51:28 (number) len=1 "1"
// class.baml:51:31 (operator) len=1 "+"
// class.baml:51:33 (variable) len=6 "nested"
// class.baml:51:40 (number) len=1 "2"
