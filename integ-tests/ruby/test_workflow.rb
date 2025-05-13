require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"


b = Baml.Client

describe "Workflows" do
    it "should run workflows" do
        workflow = b.LLMEcho(input: "Hello, world!")
        assert_equal "Hello, world!", workflow
    end
end
