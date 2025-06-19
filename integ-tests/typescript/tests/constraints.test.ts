import { b } from "./test-setup";

describe("Constraint Tests", () => {
  it("should handle checks in return types", async () => {
    const res = await b.PredictAge("Greg");
    expect(res.certainty.checks.unreasonably_certain.status).toBe("failed");
  });

  it("should handle checks in returned unions", async () => {
    const res = await b.ExtractContactInfo(
      "Reach me at 111-222-3333, or robert@boundaryml.com if needed",
    );
    expect(res.primary.value).toBe("111-222-3333");
    expect(res.secondary?.value).toBe("robert@boundaryml.com");
  });

  it("should handle block-level checks", async () => {
    const res = await b.MakeBlockConstraint();
    expect(res.checks.cross_field.status).toBe("failed");
  });

  it("should handle nested-block-level checks", async () => {
    const res = await b.MakeNestedBlockConstraint();
    expect(res.nbc.checks.cross_field.status).toBe("succeeded");
  });
});
