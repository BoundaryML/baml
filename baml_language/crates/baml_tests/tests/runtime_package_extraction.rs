//! BEP-066 P-7: package function extraction uses ordinary function subtyping,
//! with an inferred throw-set wildcard when the contract omits `throws`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn p7_omitted_throws_contract_infers_a_wildcard() {
    let output = baml_test!(
        r####"
function main() -> string throws unknown {
  let pkg = reflect.Package.compile({ "main.baml": #"
function Risky(value: string) -> string throws string {
  value
}
"# })
  let risky = pkg.get_function<(string) -> string>("root.Risky")
    ?? throw "missing root.Risky"
  risky("accepted")
}
"####
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("accepted".into()))
    );
}

#[tokio::test]
async fn extraction_uses_function_subtyping_and_throw_wildcard() {
    let output = baml_test!(
        r####"
type NeverThrowContract = (string) -> (string | int) throws never

function main() -> string throws unknown {
  let pkg = reflect.Package.compile({ "main.baml": #"
function Flexible(value: unknown) -> string throws string {
  "accepted"
}
"# })

  // The target accepts a wider input and returns a narrower output. Its
  // declared throw remains admissible because this contract omits `throws`.
  let inferred = pkg.get_function<(string) -> (string | int)>("root.Flexible")
    ?? throw "omitted throws rejected ordinary function subtyping"

  let explicit_never = pkg.get_function<NeverThrowContract>("root.Flexible") catch (e) {
    baml.reflect.errors.CompilationError => e.diagnostics[0].code
  }
  if explicit_never is string {
    inferred("input").to_string() + "|" + explicit_never
  } else {
    "explicit throws never accepted a throwing function"
  }
}
"####
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("accepted|E0001".into()))
    );
}
