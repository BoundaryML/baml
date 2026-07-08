// scenario 02 — structured output: ../../../../../llm-provider/ideas/scenarios/02-structured-output/README.md
// SAP recovers the declared type from the provider's raw reply.
// Live tier = openai_structured_live_call / strict_live_extraction.

class S02Person {
    name: string,
    age: int,
}

client<llm> DeadS02 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function ExtractPerson(doc: string) -> S02Person {
  client DeadS02
  prompt #"Extract the person from: {{ doc }}"#
}

test "02: SAP parses the provider reply into the declared class" {
    let p = ExtractPerson("Ada, 36", client = EchoProvider { reply: "{\"name\": \"Ada\", \"age\": 36}" }) catch (e) {
        _ => S02Person { name: "failed", age: -1 },
    };
    assert.equal(p.name, "Ada")
    assert.equal(p.age, 36)
}

test "02: SAP is schema-aligned — prose around the JSON still parses" {
    let p = ExtractPerson("x", client = EchoProvider { reply: "Sure! Here you go: {\"name\": \"Grace\", \"age\": 45} hope that helps" }) catch (e) {
        _ => S02Person { name: "failed", age: -1 },
    };
    assert.equal(p.name, "Grace")
}
