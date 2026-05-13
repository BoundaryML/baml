// Testsets / tests for the pipeline functions. Mirrors the
// nested-testset shape from `baml_tests/projects/compiles/testset_nested`
// and pins that the codegen pipeline handles `test`/`testset` blocks
// that reference real BAML functions. These tests aren't invoked by
// pytest — they're exercised at codegen time via the `compiles`
// discipline.

testset "handle order" {
  testset "each union arm" {
    test "pending arm" {
      let result = HandleOrder(3);
      assert.not_null(result);
    }

    test "completed arm" {
      let result = HandleOrder(1);
      assert.not_null(result);
    }

    test "cancelled arm" {
      let result = HandleOrder(2);
      assert.not_null(result);
    }
  }
}

// Stage-only smoke — exercises each leaf stage and matches on its union
// directly inside the test body.
testset "stage-only smoke" {
  test "fetch order returns a known arm" {
    let order = FetchOrder(1);
    let label = match (order) {
      let p: Pending => "pending",
      let c: Completed => "completed",
      let x: Cancelled => "cancelled"
    };
    assert.not_null(label);
  }

  test "charge payment returns a known arm" {
    let payment = ChargePayment(50);
    let label = match (payment) {
      let charged: Charged => "charged",
      let declined: Declined => "declined"
    };
    assert.not_null(label);
  }
}

// Dynamic test generation — mirrors `testset_dynamic` from
// `baml_tests/projects/compiles/`. Each iteration is its own test.
testset "parametric handle order" {
  let ids = [1, 2, 3, 4, 5];

  for (let id in ids) {
    test "handles order id" {
      let result = HandleOrder(id);
      assert.not_null(result);
    }
  }
}
