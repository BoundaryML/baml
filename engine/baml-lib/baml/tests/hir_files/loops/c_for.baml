fn SumToTen() -> int {
	let mut s = 0;

	for (let mut i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

fn NoCondition() -> int {
	let mut s = 0;

	for (let mut i = 0; ; s += i) {
	}

	s
}

fn NoAfter() -> int {
	let mut s = 0;

	for (let mut i = 0; i <= 10;) {
	}

	s
}


fn OnlyCond() -> int {
	let mut s = 0;

	for (; false;) {
	}

	s
}

 
fn Nothing() -> int {
	let mut s = 0;

	for (;;) {
	}

	s
}

// function SumToTen() {
//   var s = 0;
//   {
//   var i = 1;
//     for (;i <= 10;i += 1;) { s += i;}
//   }
//   return s;
// }
//
// function NoCondition() {
//   var s = 0;
//   {
//   var i = 0;
//     for (;;s += i;) { }
//   }
//   return s;
// }
//
// function NoAfter() {
//   var s = 0;
//   {
//   var i = 0;
//     while i <= 10 {
//     
//     }
//   }
//   return s;
// }
//
// function OnlyCond() {
//   var s = 0;
//   while false {
//   
//   }
//   return s;
// }
//
// function Nothing() {
//   var s = 0;
//   for (;;) { }
//   return s;
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
