require 'minitest/autorun'
require 'minitest/reporters'
require 'net/http'
require 'uri'
require 'json'

require_relative "baml_client/client"

JOHN_DOE_TEXT_RESUME = <<~RESUME
    John Doe
    johndoe@example.com
    (123) 456-7890
    Software Engineer
    Python, JavaScript, SQL

    Education
    University of California, Berkeley (Berkeley, CA)
    Master's in Computer Science

    Experience
    Software Engineer at Google (2020 - Present)
RESUME

JOHN_DOE_PARSED_RESUME = {
  name: "John Doe",
  email: "johndoe@example.com",
  phone: "(123) 456-7890",
  experience: ["Software Engineer at Google (2020 - Present)"],
  education: [
    {
      institution: "University of California, Berkeley",
      location: "Berkeley, CA",
      degree: "Master's",
      major: ["Computer Science"],
      graduation_date: nil
    }
  ],
  skills: ["Python", "JavaScript", "SQL"]
}

b = Baml.Client

describe "Modular API Tests" do
  it "test_modular_openai_gpt4_manual_http_request" do
    baml_req = b.request.ExtractResume2(resume: JOHN_DOE_TEXT_RESUME)

    uri = URI.parse(baml_req.url)
    http = Net::HTTP.new(uri.host, uri.port)
    http.use_ssl = uri.scheme == 'https'

    req = Net::HTTP::Post.new(uri.path)
    req.initialize_http_header(baml_req.headers)
    # Binary buffer would be baml_req.body.raw.pack("C*")
    req.body = baml_req.body.json.to_json

    response = http.request(req)

    parsed = b.parse.ExtractResume2(
      llm_response: JSON.parse(response.body)["choices"][0]["message"]["content"]
    )

    assert_equal parsed.to_json, JOHN_DOE_PARSED_RESUME.to_json
  end
end
