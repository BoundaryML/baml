require_relative 'test_helper'
require_relative "baml_client/client"

class TestEnvVarsWithoutSetup < Minitest::Test
  def setup
    # Initialize BAML client
    @b = Baml.Client
  end

  def test_env_vars_remain_unchanged_without_options
    # Store initial state
    initial_vars = ENV.to_h.dup
    
    # Call ExtractPeople without any baml_options
    result = @b.ExtractPeople(text: "John and Jane went to the store")
    
    # Verify environment variables are exactly the same as before
    assert_equal initial_vars, ENV.to_h
  end
end

class TestEnvVarsWithSetup < Minitest::Test
  def setup
    # Save original environment variables
    @original_env = ENV.to_h.dup
    
    # Set up test environment variables
    ENV['OPENAI_API_KEY'] = 'sk-system-key'
    ENV['ANTHROPIC_API_KEY'] = 'sk-ant-system-key'
    ENV['AZURE_OPENAI_API_KEY'] = 'azure-system-key'

    # Initialize BAML client
    @b = Baml.Client
  end

  def teardown
    # Restore original environment variables
    ENV.clear
    @original_env.each { |k, v| ENV[k] = v }
  end

  def test_system_env_vars_are_preserved
    # Make a request without any baml_options
    error = assert_raises(RuntimeError) do
      @b.ExtractPeople(text: "John and Jane went to the store")
    end
    assert_includes error.message, "Incorrect API key provided"
  end

  def test_user_env_vars_override_system_vars
    # Test with user-provided env vars
    user_vars = {
      'OPENAI_API_KEY' => 'sk-user-key',
      'AZURE_OPENAI_API_KEY' => 'azure-user-key'
    }
    
    error = assert_raises(RuntimeError) do
      @b.ExtractPeople(
        text: "John and Jane went to the store",
        baml_options: { env: user_vars }
      )
    end
    assert_includes error.message, "Incorrect API key provided"
  end

  def test_env_vars_are_merged_correctly
    # Test with some user-provided env vars
    user_vars = {
      'OPENAI_API_KEY' => 'sk-user-key',
      'AZURE_OPENAI_API_KEY' => 'azure-user-key'
    }
    
    # Test OpenAI request
    error = assert_raises(RuntimeError) do
      @b.ExtractPeople(
        text: "John and Jane went to the store",
        baml_options: { env: user_vars }
      )
    end
    assert_includes error.message, "Incorrect API key provided"

    # Test Anthropic request
    error = assert_raises(RuntimeError) do
      @b.TestRoundRobinStrategy(
        input: "test",
        baml_options: { env: user_vars }
      )
    end
    assert_includes error.message, "invalid x-api-key"
  end

  def test_nil_env_vars_handling
    # Test that nil env_vars are handled gracefully
    error = assert_raises(RuntimeError) do
      @b.ExtractPeople(
        text: "John and Jane went to the store",
        baml_options: { env: nil }
      )
    end
    assert_includes error.message, "Incorrect API key provided"
  end
end
