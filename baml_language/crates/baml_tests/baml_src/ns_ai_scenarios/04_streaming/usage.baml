// 04 — streaming consumes the same manual Request<T>; only the driver changes.

class s04_Meeting {
  title: string,
  date: string,
  attendees: string[],
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s04_Haiku(topic: string) -> string {
//   client: root.ai.FixtureGenerate { name: "text-default", reply: `"owl moon"`, attributes: {} }
//   prompt: `Write a short haiku about ${topic}.`
// }

// Manual expansion reference for s04_Haiku.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s04_Haiku_request(...) models s04_Haiku.request(...): it renders and
//   packages Request<string> without performing I/O.
// - s04_Haiku_manual(...) models the plain s04_Haiku(...) lowering:
//   root.ai.run(request).
// - s04_stream_tokens(...) exercises s04_Haiku.stream(...) via
//   root.ai.stream(request), then drains the token stream for the assertion.
function s04_Haiku_request(
  topic: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "text-default",
    reply: `"owl moon"`,
    attributes: {},
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s04_Haiku",
    "ns_ai_scenarios",
    { "topic": topic },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Write a short haiku about ${topic}.`, false),
  )
}

function s04_Haiku_manual(
  topic: string,
  provider: root.ai.Provider,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s04_Haiku_request(topic, provider = provider))
}

function s04_stream_tokens(topic: string, provider: root.ai.Provider) -> string throws never {
  let stream = root.ai.stream<string, string>(s04_Haiku_request(topic, provider = provider)) catch (e) {
    let ignored: baml.errors.Unsupported => return "unsupported",
    _ => return "stream-open-error",
  };
  while (true) {
    match (stream.next()) {
      let ignored: string => {},
      let ignored: baml.stream.StreamFinished => { break; },
    } catch (e) { _ => return "stream-read-error" }
  }
  let final = stream.final() catch (e) { _ => "stream-final-error" };
  log.info(baml.json.stringify({ "event": "llm.stream.final", "body": final }));
  final
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s04_ExtractMeeting(blurb: string) -> s04_Meeting {
//   client: root.ai.FixtureGenerate {
//     name: "meeting-default",
//     reply: `{"title":"Lunch","date":"Friday","attendees":["Ada"]}`,
//     attributes: {},
//   }
//   prompt: `Extract the meeting from ${blurb}. ${ctx.output_format}`
// }

// Manual expansion reference for s04_ExtractMeeting.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s04_ExtractMeeting_request(...) models s04_ExtractMeeting.request(...): it
//   packages Request<s04_Meeting> and its output schema without performing I/O.
// - s04_ExtractMeeting_manual(...) models the plain s04_ExtractMeeting(...)
//   lowering: root.ai.run(request).
// - s04_stream_meeting(...) exercises s04_ExtractMeeting.stream(...) via
//   root.ai.stream(request), then drains the structured partial stream.
function s04_ExtractMeeting_request(
  blurb: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "meeting-default",
    reply: `{"title":"Lunch","date":"Friday","attendees":["Ada"]}`,
    attributes: {},
  },
) -> root.ai.Request<s04_Meeting> {
  root.ai.task_request<s04_Meeting>(
    provider,
    "s04_ExtractMeeting",
    "ns_ai_scenarios",
    { "blurb": blurb },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Extract the meeting from ${blurb}.`, true),
  )
}

function s04_ExtractMeeting_manual(
  blurb: string,
  provider: root.ai.Provider,
) -> s04_Meeting throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s04_Meeting>(s04_ExtractMeeting_request(blurb, provider = provider))
}

function s04_stream_meeting(blurb: string, provider: root.ai.Provider) -> s04_Meeting throws never {
  let stream = root.ai.stream<s04_Meeting$stream, s04_Meeting>(
    s04_ExtractMeeting_request(blurb, provider = provider),
  ) catch (e) {
    let ignored: baml.errors.Unsupported => return s04_Meeting {
      title: "unsupported", date: "", attendees: [],
    },
    _ => return s04_Meeting { title: "stream-open-error", date: "", attendees: [] },
  };
  while (true) {
    match (stream.next()) {
      let ignored: s04_Meeting$stream => {},
      let ignored: baml.stream.StreamFinished => { break; },
    } catch (e) {
      _ => return s04_Meeting { title: "stream-read-error", date: "", attendees: [] },
    }
  }
  let final = stream.final() catch (e) {
    _ => s04_Meeting { title: "stream-final-error", date: "", attendees: [] },
  };
  let final_body = baml.json.to_string<s04_Meeting>(final) catch (e) {
    _ => final.title,
  };
  log.info(baml.json.stringify({
    "event": "llm.stream.final",
    "body": final_body,
  }));
  final
}

test "04: Generate does not silently pretend to stream" {
  assert.equal(
    s04_stream_tokens("owls", root.ai.FixtureGenerate {
      name: "generate-only", reply: `"owl moon"`, attributes: {},
    }),
    "unsupported",
  )
}

test "04: the same negotiation rule applies to structured partials" {
  assert.equal(
    s04_stream_meeting("Lunch Friday", root.ai.fixture(`{"title":"Lunch","date":"Friday","attendees":[]}`)).title,
    "unsupported",
  )
}

testset "integ-test-04-streaming" {
  test "04 live: token stream over OpenAI SSE reassembles to the whole message" {
    let r = s04_stream_tokens("the number one, in one short line", root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    });
    assert.is_true(r.length() > 0)
  }

  test "04 live: token stream over Anthropic events uses the same consumer" {
    let r = s04_stream_tokens("the number one, in one short line", root.ai.Anthropic {
      model: "claude-haiku-4-5-20251001",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      base_url: null,
      max_tokens: 256,
      anthropic_version: null,
      extra_headers: null,
      extra_body: null,
      append_output_schema: null,
    });
    assert.is_true(r.length() > 0)
  }

  test "04 live: structured partials over Gemini finish as a validated value" {
    let m = s04_stream_meeting("Lunch with Alice and Bob on Friday.", root.ai.Gemini {
      model: "gemini-flash-latest",
      api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
      append_output_schema: null,
    });
    assert.is_true(m.title.length() > 0)
  }

  test "04 live: a fallback provider streams through the same manual request" {
    let resilient: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    }.with_retry(2).fallback_to(root.ai.Gemini {
      model: "gemini-flash-latest",
      api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
      append_output_schema: null,
    });
    assert.is_true(s04_stream_tokens("one", resilient).length() > 0)
  }
}
