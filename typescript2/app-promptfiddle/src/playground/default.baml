// Review an invoice with plain BAML logic or extract one from text with an LLM.

//# review the invoice
function ReviewInvoice(invoice: Invoice) -> Report {
  let issues: ValidationIssue[] = [];

  //# check the math
  let line_total = LineTotal(invoice.line_items);
  let difference = line_total - invoice.total;
  if (difference < 0.0) {
    difference = -difference;
  };

  if (difference > 0.02) {
    //## flag a mismatch
    issues.push(ValidationIssue {
      path: "total",
      severity: "error",
      message: "line item sum does not match total",
    });
  };

  //# check the due date
  if (invoice.due_date == null) {
    //## flag a missing due date
    issues.push(ValidationIssue {
      path: "due_date",
      severity: "warning",
      message: "invoice has no due date",
    });
  };

  //# score the risk
  let risk: RiskTier = RiskTier.Low;
  if (invoice.total > 25000.0) {
    //## block large invoices
    risk = RiskTier.Block;
  } else if (invoice.due_date == null) {
    //## review incomplete invoices
    risk = RiskTier.Review;
  };

  Report {
    risk,
    issues,
    total: invoice.total,
  }
}

//# add up the line items
function LineTotal(items: LineItem[]) -> float {
  let total = 0.0;
  for (let item in items) {
    total += item.quantity * item.price;
  }
  total
}

// This path calls an LLM. The other functions and tests run locally.
function ReviewFromText(text: string) -> Report {
  ReviewInvoice(ExtractInvoice(text))
}

function ExtractInvoice(text: string) -> Invoice {
  client: "openai/gpt-5.4-mini"
  prompt: `
    Extract a structured invoice from the text below.

    ${ctx.output_format}

    ${role("user")}
    ${text}
  `
}

class LineItem {
  name: string,
  quantity: int,
  price: float,
}

class Invoice {
  vendor: string,
  total: float,
  due_date: string?,
  line_items: LineItem[],
}

class ValidationIssue {
  path: string,
  severity: string,
  message: string,
}

enum RiskTier {
  Low,
  Review,
  Block,
}

class Report {
  risk: RiskTier,
  issues: ValidationIssue[],
  total: float,
}

testset "invoice review" {
  test "line totals multiply and sum" {
    let items = [
      LineItem { name: "Widget", quantity: 3, price: 10.0 },
      LineItem { name: "Gizmo", quantity: 2, price: 49.5 },
    ];
    assert.equal(LineTotal(items), 129.0);
  }

  test "mismatched totals are flagged" {
    let report = ReviewInvoice(Invoice {
      vendor: "Acme",
      total: 1247.5,
      due_date: "2026-09-01",
      line_items: [
        LineItem { name: "Widget", quantity: 3, price: 10.0 },
      ],
    });
    assert.equal(report.issues[0].path, "total");
  }

  test "large invoices are blocked" {
    let report = ReviewInvoice(Invoice {
      vendor: "BigCo",
      total: 50000.0,
      due_date: "2026-09-01",
      line_items: [],
    });
    assert.equal(report.risk, RiskTier.Block);
  }
}
