import { registerEnumDeserializer } from "../src/deserializer/deserializer";

enum Category {
    ONE = "ONE",
    TWO = "TWO"
}


registerEnumDeserializer({
    type: "string",
    title: "Category",
    enum: ["ONE", "TWO"]
}, {});

export { Category };