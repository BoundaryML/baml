type ProjectId = int

function NormalAlias(pid: ProjectId) -> string {
  client: "openai/gpt-4o"
  prompt: `Pid: ${pid}. Generate a fake name for it.`
}

type A = float
type B = A
type C = B

function LongerAlias(c: C) -> string {
  client: "openai/gpt-4o"
  prompt: `${c}`
}

type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function RecursiveAliases(j: JsonValue) -> string {
  client: "openai/gpt-4o"
  prompt: `${j}`
}

type I = J
type J = I

function InvalidAlias(i: I) -> string {
  client: "openai/gpt-4o"
  prompt: `${i}`
}

//----
//- diagnostics
// <no-diagnostics-expected>
