require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"

b = Baml.Client

describe "ruby<->baml integration tests" do

  it "tests dynamic clients" do

    # puts 'loaded baml spec'
    # puts Gem.loaded_specs[:baml]
    # puts 'loaded baml spec after'

    c = Baml::Ffi::ClientRegistry
    cb = Baml::Ffi::ClientRegistry.new
    cb.add_llm_client("MyClient", "openai", { model: "gpt-3.5-turbo" })
    cb.set_primary("MyClient")

    capitol = await BAML::ExpectFailure.new(
      baml_options: { client_registry: cb }
    )
    assert_match(/london/, capitol.downcase)
  end
end
