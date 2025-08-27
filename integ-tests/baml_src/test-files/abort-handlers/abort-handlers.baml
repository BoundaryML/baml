// Test functions for abort handler functionality
// These functions are designed to fail and retry to test cancellation

retry_policy AbortTestConstantRetry {
  max_retries 5
  strategy {
    type constant_delay
    delay_ms 100
  }
}

retry_policy AbortTestExponentialRetry {
  max_retries 5
  strategy {
    type exponential_backoff
    initial_delay_ms 100
    multiplier 2
    max_delay_ms 1000
  }
}

client AbortTestRetryConstant {
  provider openai
  retry_policy AbortTestConstantRetry
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

client AbortTestRetryExponential {
  provider openai
  retry_policy AbortTestExponentialRetry
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

client AbortTestFallback {
  provider fallback
  options {
    strategy [
      AbortTestRetryConstant
      AbortTestRetryExponential
    ]
  }
}

// Force failure by asking for something impossible
function FnFailRetryConstantDelay(retries: int, delay_ms: int) -> string {
  client AbortTestRetryConstant
  prompt #"
    This is a test that should always fail.
    Please return an error by throwing an exception.
    DO NOT return any valid response.
    RETRIES: {{ retries }}
    DELAY: {{ delay_ms }}
  "#
}

function FnFailRetryExponentialDelay(retries: int, initial_delay_ms: int) -> string {
  client AbortTestRetryExponential
  prompt #"
    This is a test that should always fail.
    Please return an error by throwing an exception.
    DO NOT return any valid response.
    RETRIES: {{ retries }}
    INITIAL_DELAY: {{ initial_delay_ms }}
  "#
}

function TestAbortFallbackChain(input: string) -> string {
  client AbortTestFallback
  prompt #"
    Tell me a 200-word story about tigers.

    Input: {{ input }}
  "#
}

// A simple function that should succeed for testing normal operation
function ExtractName(text: string) -> string {
  client openai/gpt-4o-mini
  prompt #"
    Extract the person's name from this text: {{ text }}
    Return only the name, nothing else.
  "#
}