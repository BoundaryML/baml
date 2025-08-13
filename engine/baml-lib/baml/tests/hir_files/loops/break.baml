fn Factorial(mut limit: int) -> int {

	let mut result = 1;


	while true {
		if limit == 0 {
			break;
		}
		result = result * limit;
		limit = limit - 1;
	}

	result
}

fn Nested() -> int {
    let mut a = 5;
    while true {
        while true {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
     // should be 2
    a
}

// function Factorial(limit) {
//   var result = 1;
//   while true {
//   if limit == 0 { {
//     break;
//     } }
//     result = result * limit;
//     limit = limit - 1;
//   }
//   return result;
// }
//
// function Nested() {
//   var a = 5;
//   while true {
//   while true {
//     a = a + 1;
//       break;
//     }
//     a = a + 1;
//     break;
//   }
//   return a;
// }
//
// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// enum std::HttpMethod {
//   Get
// }