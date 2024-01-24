import exp from "constants";
import { registerEnumDeserializer } from "../src/baml_lib/deserializer/deserializer";

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