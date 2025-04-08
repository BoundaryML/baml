// As class dep
type JsonValue = int | float | string | bool | JsonValue[] | map<string, JsonValue>

class JsonClass {
    value JsonValue
}