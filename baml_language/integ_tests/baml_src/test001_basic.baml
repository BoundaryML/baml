class Test001Basic {
  name string
  skills string[]
}

class Inner001 {
  value string
}

// Tests that @stream.done keeps original types at all nesting depths.
class StreamDoneNested {
  // Top-level class: should be types.Inner001, not stream_types.Inner001
  done_class Inner001 @stream.done
  // List of classes: should be types.Inner001[], not stream_types.Inner001[]
  done_list Inner001[] @stream.done
  // Map with class values: should be map<string, types.Inner001>
  done_map map<string, Inner001> @stream.done
  // Regular field (no @stream.done): should use stream variant
  regular Inner001
}

function ExtractTest001Basic(text: string) -> Test001Basic {
  client GPT4oMini
  prompt #"
    Extract a resume from the following text.
    Return the name and a list of skills.

    Respond in JSON with this exact schema:
    {"name": "string", "skills": ["string", ...]}

    Text:
    {{ text }}

    JSON:
  "#
}
