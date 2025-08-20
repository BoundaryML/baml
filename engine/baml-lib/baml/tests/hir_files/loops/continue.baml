fn Factorial(mut limit: int) -> int {

	let mut result = 1;


    // used to make the loop break without relying on `break` implementation.
    let mut should_continue = true;
	while should_continue {
		result = result * limit;
		limit = limit - 1;

        if limit != 0 {
            continue;
        } else {
            should_continue = false;
        }

	}

	result
}

fn ContinueNested() -> int {
    while true {
        while true {
            continue;
        }
    }
}

// function Factorial(limit) {
//   var result = 1;
//   var should_continue = true;
//   while should_continue {
//   result = result * limit;
//     limit = limit - 1;
//     if limit != 0 {
//     continue;
//     } else {
//     should_continue = false;
//     } 
//   }
//   return result;
// }
//
// function ContinueNested() {
//   while true {
//   while true {
//     continue;
//     }
//   }
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
