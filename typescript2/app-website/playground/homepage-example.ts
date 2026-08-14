// The homepage hero playground example: one flat invoice-review workflow,
// a single helper, and an optional LLM extraction step that feeds the same
// pipeline. Main logic on top; data model and tests at the bottom.
// `//#` header comments name the sections — they become the nodes of the
// graph view (a header inside an if-branch or loop pulls the whole branch
// into the graph; a header above a function names its call sites).
// Verified end-to-end with `baml check` + `baml test` (5/5 pass).

export const DEFAULT_BAML = `// run baml in your browser

// One flat workflow: validate the invoice, branch on what we find.
// The //# headers become named nodes in the graph view.
//# review the invoice
function ReviewInvoice(inv: Invoice) -> Report {
  let issues: ValidationIssue[] = [];

  //# check the math
  let line_total = LineTotal(inv.line_items);
  let diff = line_total - inv.total;
  if (diff < 0.0) { diff = -diff; };

  if (diff > 0.02) {
    //## flag a mismatch
    issues.push(ValidationIssue {
      path: "total",
      severity: "error",
      message: "line item sum does not match total",
    });
  };

  //# check the dates
  if (inv.due_date == null) {
    //## flag a missing due date
    issues.push(ValidationIssue {
      path: "due_date",
      severity: "warn",
      message: "missing due date",
    });
  };

  //# score the risk
  let risk = RiskTier.Low;
  if (inv.total > 25000.0) {
    //## block big invoices
    risk = RiskTier.Block;
  } else if (inv.due_date == null) {
    //## flag missing due dates
    risk = RiskTier.Review;
  };

  Report { risk: risk, issues: issues, total: inv.total }
}

function Main() -> Report {
  ReviewInvoice(Invoice {
    vendor: "Acme",
    total: 1247.50,
    due_date: null,
    line_items: [
      LineItem { name: "Widget", quantity: 3, price: 10.00 },
      LineItem { name: "Gizmo",  quantity: 2, price: 49.50 },
    ],
  })
}

//# add up the line items
function LineTotal(items: LineItem[]) -> float {
  let total = 0.0;
  for (let item in items) {
    total += item.quantity * item.price;
  }
  total
}

// The same pipeline from raw text: LLM extraction feeds the review.
// Needs OPENAI_API_KEY -- set it via the key icon, then run this.
function ReviewFromText(text: string) -> Report {
  let inv = ExtractInvoice(text);
  ReviewInvoice(inv)
}

function ExtractInvoice(text: string) -> Invoice {
  client: "openai/gpt5.5"
  prompt: #"
    Extract a structured invoice from the text below.

    {{ ctx.output_format }}

    {{ _.role("user") }}
    {{ text }}
  "#
}

// -- the data model ---------------------------------------------------

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

// -- tests are plain code: call the function, assert on the result ----

testset "invoice pipeline" {
  test "line totals multiply and sum" {
    let items = [
      LineItem { name: "Widget", quantity: 3, price: 10.0 },
      LineItem { name: "Gizmo", quantity: 2, price: 49.5 },
    ];
    assert.equal(LineTotal(items), 129.0);
  }

  test "mismatched total is flagged" {
    let report = ReviewInvoice(Invoice {
      vendor: "Acme",
      total: 1247.5,
      due_date: "2026-06-01",
      line_items: [LineItem { name: "Widget", quantity: 3, price: 10.0 }],
    });
    assert.equal(report.issues[0].path, "total");
  }

  test "large totals are blocked" {
    let report = ReviewInvoice(Invoice {
      vendor: "BigCo",
      total: 50000.0,
      due_date: "2026-06-01",
      line_items: [],
    });
    assert.equal(report.risk, RiskTier.Block);
  }

  test "clean invoice is low risk" {
    let report = ReviewInvoice(Invoice {
      vendor: "Acme",
      total: 129.0,
      due_date: "2026-06-01",
      line_items: [
        LineItem { name: "Widget", quantity: 3, price: 10.0 },
        LineItem { name: "Gizmo", quantity: 2, price: 49.5 },
      ],
    });
    assert.equal(report.risk, RiskTier.Low);
  }

  test "full pipeline report" {
    assert.equal(Main().risk, RiskTier.Review);
  }
}
`;

// Per-function example args. Filled into the args input when the user switches
// function in the sidebar. Objects that should be coerced to a BAML class
// instance (not a map) carry a `$baml: { type: 'ClassName' }` marker — the
// pkg-proto encoder honours it and emits a `classValue` so the runtime gets a
// typed instance instead of a map.
const lineItem = (name: string, quantity: number, price: number) => ({
  $baml: { type: 'LineItem' },
  name,
  quantity,
  price,
});

const invoiceArg = (
  total: number,
  due_date: string | null,
  items: Array<{ name: string; quantity: number; price: number }>,
) => ({
  $baml: { type: 'Invoice' },
  vendor: 'Acme',
  total,
  due_date,
  line_items: items.map((i) => lineItem(i.name, i.quantity, i.price)),
});

const SAMPLE_TEXT =
  'Vendor: Acme. Total: $1247.50. Due 2026-06-01. Items: Widget x3 @ $10, Gizmo x2 @ $49.50.';

export const EXAMPLE_ARGS: Record<string, string> = {
  ReviewInvoice: JSON.stringify(
    {
      inv: invoiceArg(1247.5, null, [
        { name: 'Widget', quantity: 3, price: 10.0 },
        { name: 'Gizmo', quantity: 2, price: 49.5 },
      ]),
    },
    null,
    2,
  ),
  LineTotal: JSON.stringify(
    {
      items: [lineItem('Widget', 3, 10.0), lineItem('Gizmo', 2, 49.5)],
    },
    null,
    2,
  ),
  Main: '{}',
  ReviewFromText: JSON.stringify({ text: SAMPLE_TEXT }, null, 2),
  ExtractInvoice: JSON.stringify({ text: SAMPLE_TEXT }, null, 2),
};
