import TypeBuilder, { FieldType } from "../baml_client/type_builder";
import { b, b_sync } from "./test-setup";

describe("Dynamic Enum Request Tests", () => {
  it("should include dynamic enum values in RenderDynamicEnum request (direct options)", async () => {
    const tb = new TypeBuilder();
    
    // Add values to RenderTestEnum
    tb.RenderTestEnum.addValue("MOTORCYCLE").alias("motorized two-wheeler");
    
    const request = await b.request.RenderDynamicEnum(
      "BIKE",
      "MOTORCYCLE",
      { tb }
    );
    
    const requestBody = request.body.json();
    expect(requestBody.model).toBe("gpt-4o-mini");
    expect(requestBody.messages).toHaveLength(1);
    expect(requestBody.messages[0].role).toBe("system");
    
    // Verify the enum values are included in the schema/prompt
    const messageContent = requestBody.messages[0].content[0].text;
    expect(messageContent).toBe(`"RenderTestEnum.BIKE" renders as: two-wheeled bike
"other" renders as: MOTORCYCLE

Available dynamic enum values:
  - BIKE: two-wheeled bike
  - SCOOTER: kick scooter

Enum comparison tests:

'bike' is equal to RenderTestEnum.BIKE, as expected

'bike' is not equal to RenderTestEnum.SCOOTER, as expected

'bike' equals "BIKE", as expected

'bike' is not equal to "SCOOTER", as expected

Multiple value tests:

'bike' is equal to RenderTestEnum.BIKE or RenderTestEnum.SCOOTER, as expected

'other' is not equal to RenderTestEnum.BIKE, as expected

'other' is MOTORCYCLE, as expected

'other' is MOTORCYCLE, as expected
`)
  });

  it("should include dynamic enum values in RenderDynamicEnum request (withOptions default)", async () => {
    const tb = new TypeBuilder();
    tb.RenderTestEnum.addValue("CHRISTMAS_SLEIGH").alias("motorized two-wheeler");

    const myB = b.withOptions({ tb });
    const request = await myB.request.RenderDynamicEnum(
      "BIKE",
      "CHRISTMAS_SLEIGH"
    );

    const requestBody = request.body.json();
    const messageContent = requestBody.messages[0].content[0].text;
    console.log(messageContent);
    expect(messageContent).toContain("Available dynamic enum values:");
    expect(messageContent).toContain("CHRISTMAS_SLEIGH");
  });

  it("should include dynamic class properties in RenderDynamicClass request", async () => {
    const tb = new TypeBuilder();
    
    // Add values to RenderStatusEnum and properties to RenderTestClass
    tb.RenderStatusEnum.addValue("PENDING").alias("waiting for action");
    tb.RenderTestClass.addProperty("priority", tb.string()).alias("task priority level");
    
    const request = await b.request.RenderDynamicClass(
      { name: "test-item", status: "PENDING", priority: "high" },
      { tb }
    );
    
    const requestBody = request.body.json();
    expect(requestBody.model).toBe("gpt-4o-mini");
    expect(requestBody.messages).toHaveLength(1);
    expect(requestBody.messages[0].role).toBe("system");
    
    // Verify the class properties are included in the schema/prompt
    const messageContent = requestBody.messages[0].content[0].text;
    // TODO: when alias rendering is correctly implemented for dynamic enums and classes
    // we can uncomment this. For now we add a test to ensure stability of the output format.
//     expect(messageContent).toEqual(`Input class data:
// {
//     "name": "test-item",
//     "status": "waiting for action",
//     "task priority level": "high"
// }
// `)
    expect(messageContent).toEqual(`Input class data: {
    "name": "test-item",
    "status": PENDING,
    "priority": "high",
}`)
  });
});