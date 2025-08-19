import TypeBuilder from "../baml_client/type_builder";
import { b, b_sync } from "./test-setup";

describe("Dynamic Enum Request Tests", () => {
  it("should include dynamic enum values in RenderDynamicEnum request", async () => {
    const tb = new TypeBuilder();
    
    // Add values to existing DynEnumOne
    tb.DynEnumThree.addValue("TRIPOD").alias("for use with cameras");
    
    const request = await b.request.RenderDynamicEnum(
      "TRICYCLE",
      "TRIPOD",
      { tb }
    );
    
    const requestBody = request.body.json();
    expect(requestBody.model).toBe("gpt-4o-mini");
    expect(requestBody.messages).toHaveLength(1);
    expect(requestBody.messages[0].role).toBe("system");
    
    // Verify the enum values are included in the schema/prompt
    const messageContent = requestBody.messages[0].content[0].text;
    expect(messageContent).toBe(`"DynEnumThree.TRICYCLE" renders as: bike with three wheels
"other" renders as: for use with cameras

Available dynamic enum values:
  - TRICYCLE: bike with three wheels
  - TRIANGLE: TRIANGLE

Enum comparison tests:

DynEnumThree matches TRICYCLE enum value, as expected

DynEnumThree is not TRIANGLE, as expected

DynEnumThree equals TRICYCLE string, as expected

DynEnumThree is not equal to TRIANGLE string, as expected

Multiple value tests:

DynEnumThree is either TRICYCLE or TRIANGLE

Other is not TRICYCLE

Other is TRIPOD
`)
  });
});