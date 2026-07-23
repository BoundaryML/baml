// Deterministic test providers used by the scenario reference suite. Keeping
// them in root.ai.testing avoids accidental dependencies between numbered
// scenarios while making each fake's wire contract explicit.
function fake_value<T, V>(value: V) -> T throws baml.errors.UnknownError {
  let encoded = baml.json.to_string<V>(value) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["fake value could not be encoded as JSON"],
    },
  };
  baml.json.from_string<T>(encoded) catch (e) {
    _ => throw baml.errors.UnknownError {
      data: e,
      message: ["fake value did not match the requested output type"],
    },
  }
}

class FakeGenerateProvider {
  name: string,
  reply: string,
  attributes: map<string, unknown>,
  model: string?,
  usage: Usage?,

  implements Provider {
    function display_name(self) -> string throws never { self.name }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      // Fakes carry exact JSON wire values. Real unconstrained providers
      // use their model-output parser; fake data must not rely on SAP repair.
      let value: T = baml.json.from_string<T>(self.reply) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["fake response did not match the requested output type"],
        },
      };
      Response<T> {
        value: value,
        meta: Meta {
          provider: self.name,
          model: self.model ?? "fake",
          request_id: null,
          finish_reason: "stop",
          usage: self.usage ?? Usage { input_tokens: 3, output_tokens: 2 },
          attributes: self.attributes,
          raw: null,
        },
      }
    }
  }
}

function fake(
  reply: string,
  name: string = "fake",
  attributes: map<string, unknown> = {},
) -> Provider throws never {
  FakeGenerateProvider {
    name: name,
    reply: reply,
    attributes: attributes,
    model: null,
    usage: null,
  }
}

function metered_fake(
  reply: string,
  input_tokens: int,
  output_tokens: int,
  name: string = "fake",
) -> Provider throws never {
  FakeGenerateProvider {
    name: name,
    reply: reply,
    attributes: {},
    model: null,
    usage: Usage { input_tokens: input_tokens, output_tokens: output_tokens },
  }
}
