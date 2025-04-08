require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"


b = Baml.Client

describe "Expose Parser Tests" do
  it "test_parse_llm_response" do
    llm_response = '
      ```json
      {
        "len": 5,
        "head": {
          "data": 1,
          "next": {
            "data": 2,
            "next": {
              "data": 3,
              "next": {
                "data": 4,
                "next": {
                  "data": 5,
                  "next": null
                }
              }
            }
          }
        }
      }
      ```
    '

    parsed = b.parse.BuildLinkedList(llm_response: llm_response)

    expected = {
      "head" => {
        "data" => 1,
        "next" => {
          "data" => 2,
          "next" => {
            "data" => 3,
            "next" => {
              "data" => 4,
              "next" => {
                "data" => 5,
                "next" => nil
              }
            }
          }
        }
      },
      "len" => 5
    }

    # TODO: Baml types in Ruby don't implement equality so we use this hack.
    assert_equal parsed.to_json, expected.to_json
  end

  it "test_parse_llm_stream" do
    stream = '
      ```json
      {
          "name": "John Doe",
          "email": "john.doe@example.com",
      ```
    '

    parsed = b.parse_stream.ExtractResume(llm_response: stream)

    expected = {
      "name" => "John Doe",
      "email" => "john.doe@example.com",
      "phone" => nil,
      "experience" => [],
      "education" => [],
      "skills" => []
    }

    # TODO: Baml types in Ruby don't implement equality so we use this hack.
    assert_equal parsed.to_json, expected.to_json
  end
end
