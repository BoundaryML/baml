// 21 — extract memories, persist ordinary values, inject them into the next task.

// Task sketch: s21_ExtractMemories(conversation) -> s21_Memories.

// Manual expansion reference for task `s21_ExtractMemories`:
// - `s21_ExtractMemories_request(...)` models
//   `s21_ExtractMemories.request(...)`.
// - `s21_ExtractMemories_manual(...)` models the plain
//   `s21_ExtractMemories(...)` call, lowered through `root.ai.run`.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s21_ExtractMemories_request(
  conversation: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate { name: "memory-extractor", reply: `{"items":[]}`, attributes: {} },
) -> root.ai.Request<s21_Memories> {
  root.ai.task_request<s21_Memories>(
    provider, "s21_ExtractMemories", "ns_ai_scenarios", { "conversation": conversation },
    [], root.ai.RequestOptions {}, {},
    root.ai.prompt_template(`Extract durable facts.\n${conversation}`, true),
  )
}

function s21_ExtractMemories_manual(
  conversation: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate { name: "memory-extractor", reply: `{"items":[]}`, attributes: {} },
) -> s21_Memories throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s21_Memories>(s21_ExtractMemories_request(conversation, provider = provider))
}

// Task sketch: s21_AnswerWithMemory(memories, question) -> string.

// Manual expansion reference for task `s21_AnswerWithMemory`:
// - `s21_AnswerWithMemory_request(...)` models
//   `s21_AnswerWithMemory.request(...)`.
// - `s21_AnswerWithMemory_manual(...)` models the plain
//   `s21_AnswerWithMemory(...)` call, lowered through `root.ai.run`.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s21_AnswerWithMemory_request(
  memories: string,
  question: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate { name: "memory-answerer", reply: `"unknown"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s21_AnswerWithMemory", "ns_ai_scenarios",
    { "memories": memories, "question": question }, [], root.ai.RequestOptions {}, {},
    root.ai.prompt_template(`Use these memories:\n${memories}\n\nUser: ${question}`, false),
  )
}

function s21_AnswerWithMemory_manual(
  memories: string,
  question: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate { name: "memory-answerer", reply: `"unknown"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s21_AnswerWithMemory_request(memories, question, provider = provider))
}

test "21: extractor returns typed memory values" {
  let memories = s21_ExtractMemories_manual(
    "I am vegetarian and my dog is Bella.",
    provider = root.ai.FixtureGenerate {
      name: "fixture", reply: `{"items":[{"subject":"user","fact":"is vegetarian"},{"subject":"user","fact":"has a dog named Bella"}]}`, attributes: {},
    },
  );
  assert.equal(memories.items.length(), 2)
  assert.contains(s21_render_memories(memories.items), "Bella")
}

test "21: the next request contains recalled application memory" {
  let request = s21_AnswerWithMemory_request("- user: is vegetarian", "What should I cook?");
  assert.contains(baml.llm.prompt_to_text(request.prompt), "is vegetarian")
}

test "21: empty memory has an explicit floor" {
  assert.equal(s21_render_memories([]), "(no memories yet)")
}

testset "integ-test-21-memory" {
  test "21 live: a stated fact is extracted, stored, and recalled to answer later" {
    let model: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let extracted = s21_ExtractMemories_manual(
      "My favorite color is teal and I live in Lisbon.", provider = model,
    );
    let answer = s21_AnswerWithMemory_manual(
      s21_render_memories(extracted.items), "Where do I live? Reply with just the city.", provider = model,
    );
    assert.is_true(answer.to_lower_case().includes("lisbon"))
  }
}
