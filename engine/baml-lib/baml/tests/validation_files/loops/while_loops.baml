function GCD(a: int, b: int) -> int {

	while (a != b) {
	   if (a > b) {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

function Foo() -> int {

    let x = true;
    let y = false;

    let a = 0;

    while (a < 10) {
        let b = if (x) {
            let c = 5;
            5
         } else if (y) {
            6
         } else {
            7
         };

         a = a + 1;
    }

    3
}
