function SumToTen() -> int {
	let s = 0;

	for (let i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

function NoCondition() -> int {
	let s = 0;

	for (let i = 0; ; s += i) {
	}

	s
}

function NoAfter() -> int {
	let s = 0;

	for (let i = 0; i <= 10;) {
	}

	s
}


function OnlyCond() -> int {
	let s = 0;

	for (; false;) {
	}

	s
}


function Nothing() -> int {
	let s = 0;

	for (;;) {
	}

	s
}

// function SumToTen() {
//   let s = 0;
//   {
//   let i = 1;
//     for (;i <= 10;{
//     i += 1;
//     }) { s += i;}
//   }
//
//   s
// }
//
// function NoCondition() {
//   let s = 0;
//   {
//   let i = 0;
//     for (;;{
//     s += i;
//     }) { }
//   }
//
//   s
// }
//
// function NoAfter() {
//   let s = 0;
//   {
//   let i = 0;
//     while i <= 10 {
//     
//     }
//   }
//
//   s
// }
//
// function OnlyCond() {
//   let s = 0;
//   while false {
//   
//   }
//
//   s
// }
//
// function Nothing() {
//   let s = 0;
//   for (;;) { }
//
//   s
// }
