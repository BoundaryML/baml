//! Execution tests for bound and unbound method closure types.
//!
//! These tests verify that bound method references (`p.get_name`) and unbound
//! method references (`Person.get_name`) compile to correct bytecode and execute
//! at runtime, returning the expected values.

use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn bound_formatter_mapper() {
    let output = baml_test!(
        r#"
        class Formatter {
          prefix string
          function format(self, text: string) -> string { self.prefix + text }
        }
        function main() -> string[] {
          let fmt = Formatter { prefix: ">> " };
          let transform = fmt.format;
          ["a", "b"].map(transform)
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::Array {
            element_type: Ty::string(),
            items: vec![
                BexExternalValue::String(">> a".into()),
                BexExternalValue::String(">> b".into()),
            ],
        }
    );
}

#[tokio::test]
async fn unbound_in_map() {
    let output = baml_test!(
        r#"
        class Employee {
          first string
          last string
          function full_name(self) -> string { self.first + " " + self.last }
        }
        function main() -> string[] {
          let employees = [
            Employee { first: "Alice", last: "Smith" },
            Employee { first: "Bob", last: "Jones" },
          ];
          employees.map(Employee.full_name)
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::Array {
            element_type: Ty::string(),
            items: vec![
                BexExternalValue::String("Alice Smith".into()),
                BexExternalValue::String("Bob Jones".into()),
            ],
        }
    );
}

#[tokio::test]
async fn bound_validator_mapped() {
    let output = baml_test!(
        r#"
        class RangeCheck {
          min int
          max int
          function is_valid(self, n: int) -> bool { n >= self.min && n <= self.max }
        }
        function main() -> bool[] {
          let valid = RangeCheck { min: 10, max: 50 };
          let check = valid.is_valid;
          [5, 15, 25, 75].map(check)
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::Array {
            element_type: Ty::bool(),
            items: vec![
                BexExternalValue::Bool(false),
                BexExternalValue::Bool(true),
                BexExternalValue::Bool(true),
                BexExternalValue::Bool(false),
            ],
        }
    );
}

#[tokio::test]
async fn mutation_after_bind() {
    let output = baml_test!(
        r#"
        class Counter {
          value int
          function increment(self) -> int { self.value += 1; self.value }
        }
        function main() -> int {
          let c = Counter { value: 0 };
          let inc = c.increment;
          c.value = 100;
          inc()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(101));
}

#[tokio::test]
async fn repeated_bound_calls() {
    let output = baml_test!(
        r#"
        class Counter {
          value int
          function increment(self) -> int { self.value += 1; self.value }
        }
        function main() -> int {
          let c = Counter { value: 0 };
          let inc = c.increment;
          inc();
          inc();
          inc()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(3));
}

#[tokio::test]
async fn field_chain_bind() {
    let output = baml_test!(
        r#"
        class Encoder {
          charset string
          function encode(self, data: string) -> string { "[" + self.charset + "]" + data }
        }
        class Config {
          encoder Encoder
        }
        function main() -> string {
          let cfg = Config { encoder: Encoder { charset: "utf8" } };
          let enc = cfg.encoder.encode;
          enc("hi")
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("[utf8]hi".into())
    );
}

#[tokio::test]
async fn strategy_swap() {
    let output = baml_test!(
        r#"
        class Calculator {
          op string
          function compute(self, a: int, b: int) -> int {
            if self.op == "add" { a + b } else { a * b }
          }
        }
        function apply(f: (int, int) -> int, a: int, b: int) -> int { f(a, b) }
        function main() -> int[] {
          let add_calc = Calculator { op: "add" };
          let mul_calc = Calculator { op: "mul" };
          let do_add = add_calc.compute;
          let do_mul = mul_calc.compute;
          [apply(do_add, 3, 4), apply(do_mul, 3, 4)]
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![BexExternalValue::Int(7), BexExternalValue::Int(12),],
        }
    );
}

#[tokio::test]
async fn generic_unwrap() {
    let output = baml_test!(
        r#"
        class Box<T> {
          value T
          function unwrap(self) -> T { self.value }
        }
        function main() -> int {
          let b = Box { value: 42 };
          let get = b.unwrap;
          get()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn bound_in_lambda() {
    let output = baml_test!(
        r#"
        class Employee {
          first string
          last string
          function greet(self, greeting: string) -> string { greeting + ", " + self.first }
        }
        function main() -> string {
          let emp = Employee { first: "Alice", last: "Smith" };
          let make_greeter = () -> { emp.greet };
          let greeter = make_greeter();
          greeter("Hello")
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Hello, Alice".into())
    );
}

#[tokio::test]
async fn bound_throwing_method_reference_catches_error() {
    let output = baml_test!(
        r#"
        class Worker {
          factor int
          function risky(self, value: int) -> int throws string {
            if (value < 0) { throw "negative" }
            self.factor * value
          }
        }
        function main() -> int {
          let worker = Worker { factor: 2 };
          let run = worker.risky;
          run(-1) catch (e) {
            "negative" => -1,
            _ => -2
          }
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(-1));
}
