// Shared offline Generate fixture for the BEPv2 scenario references.
// It implements the semantic capability directly: there are no fake HTTP
// build/send/parse stages. Other scenarios reuse it to keep their examples small.
class BepV2FixtureGenerate {
  name: string,
  reply: string,
  attributes: map<string, unknown>,

  implements root.ai.Provider {
    // Fixture names are observable provider identities, just like the
    // provider recorded in response metadata. Routing a fixture through a
    // Provider-typed value must not collapse that identity to the interface's
    // generic default name.
    function display_name(self) -> string throws never { self.name }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let value: T = baml.sap.parse<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["fixture response did not match the requested output type"],
        },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: self.name,
          model: "fixture",
          request_id: null,
          finish_reason: "stop",
          usage: root.ai.Usage { input_tokens: 3, output_tokens: 2 },
          attributes: self.attributes,
          raw: null,
        },
      }
    }
  }
}

function bepv2_fixture(reply: string) -> root.ai.Provider throws never {
  BepV2FixtureGenerate { name: "fixture", reply: reply, attributes: {} }
}

// Build the lazy prompt recipe without the reserved `prompt` tag. Any ordinary
// function containing that tag is currently classified as a declarative task.
function bepv2_prompt_template(
  text: string,
  with_output_format: bool,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    if (with_output_format) {
      baml.llm.assemble_prompt_ast(["", "\n", ""], [text, ctx.output_format])
    } else {
      baml.llm.assemble_prompt_ast(["", ""], [text])
    }
  }
}
